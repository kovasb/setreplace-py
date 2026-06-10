//! Incremental hypergraph pattern matcher, mirroring libSetReplace's
//! `HypergraphMatcher`.
//!
//! The matcher maintains the set of *all* possible matches (rule + ordered
//! list of input tokens) at the current state. When tokens are created, new
//! matches involving them are discovered; when tokens are consumed, matches
//! involving them are removed.
//!
//! Match selection follows libSetReplace exactly: matches are kept in a
//! sorted queue of buckets, each bucket holding matches that are equivalent
//! under the ordering specification, with buckets ordered by it. The next
//! event is drawn **uniformly at random from the first bucket** — i.e.
//! randomness is the implicit final tie-breaker. If the specification ends
//! with [`OrderingFunction::Any`], the first bucket's front element is chosen
//! deterministically instead.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use crate::atoms_index::AtomsIndex;
use crate::error::Error;
use crate::pcg::Pcg32;
use crate::rule::{Atom, Rule};
use crate::system::{Token, TokenId};

/// Event-ordering functions, named as in SetReplace's
/// `"EventOrderingFunction"` option.
///
/// An ordering specification is a sequence of these; each is applied in turn
/// to break ties left by the previous ones. Any remaining ambiguity is
/// resolved uniformly at random (seeded), unless the specification ends with
/// [`OrderingFunction::Any`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderingFunction {
    /// Match whose sorted input-token IDs are lexicographically smallest
    /// (prefers matches containing the oldest edges).
    OldestEdge,
    /// Reverse of [`OrderingFunction::OldestEdge`].
    LeastOldEdge,
    /// Match whose descending-sorted input-token IDs are lexicographically
    /// smallest (prefers matches whose *newest* edge is as old as possible).
    /// First element of the default ordering.
    LeastRecentEdge,
    /// Reverse of [`OrderingFunction::LeastRecentEdge`].
    NewestEdge,
    /// Match whose input-token IDs, in the order they instantiate the rule's
    /// input edges, are lexicographically smallest.
    RuleOrdering,
    /// Reverse of [`OrderingFunction::RuleOrdering`].
    ReverseRuleOrdering,
    /// Match with the smallest rule index.
    RuleIndex,
    /// Reverse of [`OrderingFunction::RuleIndex`].
    ReverseRuleIndex,
    /// No-op: randomness is already the implicit final tie-breaker, so this
    /// merely documents intent (SetReplace maps `"Random"` to nothing).
    Random,
    /// When last in the specification, picks an unspecified but deterministic
    /// representative from the final bucket instead of a random one.
    Any,
}

/// The default `"EventOrderingFunction"` of `WolframModel` / `SetReplace`:
/// `{"LeastRecentEdge", "RuleOrdering", "RuleIndex"}`.
pub fn default_event_ordering() -> Vec<OrderingFunction> {
    vec![
        OrderingFunction::LeastRecentEdge,
        OrderingFunction::RuleOrdering,
        OrderingFunction::RuleIndex,
    ]
}

/// Internal ordering primitives (libSetReplace's `OrderingFunction` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrdFn {
    SortedIds,
    ReverseSortedIds,
    InputIds,
    RuleIdx,
    Any,
}

fn translate_spec(spec: &[OrderingFunction]) -> (Vec<(OrdFn, bool)>, bool) {
    let mut out = Vec::new();
    for f in spec {
        let pair = match f {
            OrderingFunction::OldestEdge => (OrdFn::SortedIds, false),
            OrderingFunction::LeastOldEdge => (OrdFn::SortedIds, true),
            OrderingFunction::LeastRecentEdge => (OrdFn::ReverseSortedIds, false),
            OrderingFunction::NewestEdge => (OrdFn::ReverseSortedIds, true),
            OrderingFunction::RuleOrdering => (OrdFn::InputIds, false),
            OrderingFunction::ReverseRuleOrdering => (OrdFn::InputIds, true),
            OrderingFunction::RuleIndex => (OrdFn::RuleIdx, false),
            OrderingFunction::ReverseRuleIndex => (OrdFn::RuleIdx, true),
            // SetReplace maps "Random" to Nothing: it removes itself from the
            // spec, leaving the implicit random tie-break to do the work.
            OrderingFunction::Random => continue,
            OrderingFunction::Any => (OrdFn::Any, false),
        };
        out.push(pair);
    }
    let ends_with_any = matches!(out.last(), Some((OrdFn::Any, _)));
    (out, ends_with_any)
}

/// A potential event: a rule and the tokens instantiating its input edges
/// (in rule input order). Two matches with the same token *set* but different
/// assignment order are distinct.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Match {
    pub(crate) rule: usize,
    pub(crate) inputs: Vec<TokenId>,
}

/// Sort key placing a match in the bucket queue. Comparison applies the
/// ordering chain (libSetReplace's `MatchComparator`); matches the chain
/// cannot distinguish compare as equal and therefore share a bucket.
/// Vector comparison is lexicographic with a shorter prefix ordered first,
/// matching `compareVectors` — i.e. Rust's `Ord` on `Vec`.
struct QueueKey {
    spec: Rc<[(OrdFn, bool)]>,
    rule: usize,
    inputs: Vec<TokenId>,
    sorted: Vec<TokenId>,
    rev_sorted: Vec<TokenId>,
}

impl QueueKey {
    fn new(spec: Rc<[(OrdFn, bool)]>, m: &Match) -> Self {
        let mut sorted = m.inputs.clone();
        sorted.sort_unstable();
        let mut rev_sorted = m.inputs.clone();
        rev_sorted.sort_unstable_by(|a, b| b.cmp(a));
        QueueKey {
            spec,
            rule: m.rule,
            inputs: m.inputs.clone(),
            sorted,
            rev_sorted,
        }
    }
}

impl Ord for QueueKey {
    fn cmp(&self, other: &Self) -> Ordering {
        for &(f, reverse) in self.spec.iter() {
            let ord = match f {
                OrdFn::SortedIds => self.sorted.cmp(&other.sorted),
                OrdFn::ReverseSortedIds => self.rev_sorted.cmp(&other.rev_sorted),
                OrdFn::InputIds => self.inputs.cmp(&other.inputs),
                OrdFn::RuleIdx => self.rule.cmp(&other.rule),
                // `Any` never participates in comparisons (libSetReplace's
                // comparator returns "equal" for it); it only switches the
                // final pick from random to deterministic.
                OrdFn::Any => Ordering::Equal,
            };
            let ord = if reverse { ord.reverse() } else { ord };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for QueueKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QueueKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for QueueKey {}

/// One equivalence class of matches. The vector gives O(1) random selection;
/// the position map gives O(1) order-perturbing deletion (libSetReplace's
/// `Bucket`).
#[derive(Default)]
struct Bucket {
    items: Vec<Rc<Match>>,
    positions: HashMap<Rc<Match>, usize>,
}

pub(crate) struct Matcher {
    spec: Rc<[(OrdFn, bool)]>,
    ends_with_any: bool,
    queue: BTreeMap<QueueKey, Bucket>,
    token_to_matches: HashMap<TokenId, BTreeSet<Rc<Match>>>,
    rng: Pcg32,
}

impl Matcher {
    pub(crate) fn new(ordering: &[OrderingFunction], random_seed: u64) -> Self {
        let (spec, ends_with_any) = translate_spec(ordering);
        Matcher {
            spec: Rc::from(spec),
            ends_with_any,
            queue: BTreeMap::new(),
            token_to_matches: HashMap::new(),
            rng: Pcg32::new(random_seed),
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Match> {
        self.queue
            .values()
            .flat_map(|b| b.items.iter())
            .map(|rc| rc.as_ref())
    }

    /// Finds and indexes all matches involving at least one of `new_ids`.
    ///
    /// Mirrors `HypergraphMatcher::addMatchesInvolvingTokens`: each new token
    /// is tried as an anchor at every input position of every rule; the
    /// remaining positions are completed through the atoms index, choosing at
    /// each step the position with the fewest candidate tokens. Duplicate
    /// discoveries (a match containing several new tokens) are deduplicated on
    /// insertion.
    pub(crate) fn add_matches_involving_tokens(
        &mut self,
        new_ids: &[TokenId],
        rules: &[Rule],
        tokens: &[Token],
        index: &AtomsIndex,
    ) -> Result<(), Error> {
        for (rule_idx, rule) in rules.iter().enumerate() {
            for anchor_pos in 0..rule.inputs.len() {
                for &t in new_ids {
                    let mut map = HashMap::new();
                    if !unify_edge(&rule.inputs[anchor_pos], &tokens[t].atoms, &mut map) {
                        continue;
                    }
                    let mut patterns = rule.inputs.clone();
                    substitute(&mut patterns, &map);
                    let mut partial: Vec<Option<TokenId>> = vec![None; rule.inputs.len()];
                    partial[anchor_pos] = Some(t);
                    self.complete_match(rule_idx, &mut partial, &patterns, tokens, index)?;
                }
            }
        }
        Ok(())
    }

    fn complete_match(
        &mut self,
        rule_idx: usize,
        partial: &mut Vec<Option<TokenId>>,
        patterns: &[Vec<Atom>],
        tokens: &[Token],
        index: &AtomsIndex,
    ) -> Result<(), Error> {
        if partial.iter().all(|p| p.is_some()) {
            let inputs = partial.iter().map(|p| p.unwrap()).collect();
            self.insert_match(Match {
                rule: rule_idx,
                inputs,
            });
            return Ok(());
        }

        let (pos, candidates) = next_best_input(partial, patterns, index)?;
        for &cand in &candidates {
            // A token cannot instantiate two input edges of the same event.
            if partial.contains(&Some(cand)) {
                continue;
            }
            let mut map = HashMap::new();
            if !unify_edge(&patterns[pos], &tokens[cand].atoms, &mut map) {
                continue;
            }
            let mut new_patterns = patterns.to_vec();
            substitute(&mut new_patterns, &map);
            partial[pos] = Some(cand);
            self.complete_match(rule_idx, partial, &new_patterns, tokens, index)?;
            partial[pos] = None;
        }
        Ok(())
    }

    fn insert_match(&mut self, m: Match) {
        let rc = Rc::new(m);
        let key = QueueKey::new(self.spec.clone(), &rc);
        let bucket = self.queue.entry(key).or_default();
        if bucket.positions.contains_key(&rc) {
            return; // duplicate discovery of the same match
        }
        bucket.positions.insert(rc.clone(), bucket.items.len());
        bucket.items.push(rc.clone());
        for &t in &rc.inputs {
            self.token_to_matches.entry(t).or_default().insert(rc.clone());
        }
    }

    /// Removes every match that uses any of the given tokens. Deletion order
    /// is canonical so the evolution stays deterministic (bucket-internal
    /// order is perturbed by deletions).
    pub(crate) fn remove_matches_involving_tokens(&mut self, ids: &[TokenId]) {
        let mut to_delete: BTreeSet<Rc<Match>> = BTreeSet::new();
        for t in ids {
            if let Some(set) = self.token_to_matches.get(t) {
                to_delete.extend(set.iter().cloned());
            }
        }
        for m in &to_delete {
            self.delete_match(m);
        }
    }

    fn delete_match(&mut self, m: &Rc<Match>) {
        let key = QueueKey::new(self.spec.clone(), m);
        if let Some(bucket) = self.queue.get_mut(&key) {
            if let Some(idx) = bucket.positions.remove(m) {
                // O(1) order-perturbing removal from the vector.
                let last = bucket.items.len() - 1;
                bucket.items.swap(idx, last);
                bucket.items.pop();
                if idx < bucket.items.len() {
                    bucket.positions.insert(bucket.items[idx].clone(), idx);
                }
            }
            if bucket.items.is_empty() {
                self.queue.remove(&key);
            }
        }
        for t in &m.inputs {
            if let Some(set) = self.token_to_matches.get_mut(t) {
                set.remove(m);
                if set.is_empty() {
                    self.token_to_matches.remove(t);
                }
            }
        }
    }

    /// The match the next event should instantiate, per the ordering spec:
    /// a uniformly random element of the first bucket, or its front element
    /// if the spec ends with `Any`.
    pub(crate) fn next_match(&mut self) -> Option<Match> {
        let (_, bucket) = self.queue.first_key_value()?;
        let chosen = if self.ends_with_any {
            &bucket.items[0]
        } else {
            &bucket.items[self.rng.gen_index(bucket.items.len())]
        };
        Some((**chosen).clone())
    }
}

/// Picks the unfilled input position with the fewest candidate tokens
/// (libSetReplace's `nextBestInputAndTokensToTry`). Candidates for a position
/// are the indexed tokens containing *all* of its concrete atoms. Positions
/// whose pattern has no concrete atoms (after substitution of variables bound
/// so far) cannot be looked up; if no position is eligible, the rule's inputs
/// are disconnected, which is unsupported.
fn next_best_input(
    partial: &[Option<TokenId>],
    patterns: &[Vec<Atom>],
    index: &AtomsIndex,
) -> Result<(usize, Vec<TokenId>), Error> {
    let mut best: Option<(usize, Vec<TokenId>)> = None;
    for (i, pattern) in patterns.iter().enumerate() {
        if partial[i].is_some() {
            continue;
        }
        let concrete: BTreeSet<Atom> = pattern.iter().copied().filter(|&a| a > 0).collect();
        if concrete.is_empty() {
            continue;
        }
        let mut counts: HashMap<TokenId, usize> = HashMap::new();
        for &atom in &concrete {
            if let Some(toks) = index.tokens_containing(atom) {
                for &t in toks {
                    *counts.entry(t).or_insert(0) += 1;
                }
            }
        }
        let mut cands: Vec<TokenId> = counts
            .iter()
            .filter(|&(_, &c)| c == concrete.len())
            .map(|(&t, _)| t)
            .collect();
        cands.sort_unstable();
        if best.as_ref().is_none_or(|(_, b)| cands.len() < b.len()) {
            best = Some((i, cands));
        }
    }
    best.ok_or(Error::DisconnectedInputs)
}

/// Matches one pattern edge against one token's atoms, extending `map`
/// (variable → atom). Mirrors libSetReplace's
/// `substituteMissingAtomsIfPossible`: repeated variables must agree, and
/// concrete atoms (including ones substituted earlier) must be equal.
pub(crate) fn unify_edge(
    pattern: &[Atom],
    atoms: &[Atom],
    map: &mut HashMap<Atom, Atom>,
) -> bool {
    if pattern.len() != atoms.len() {
        return false;
    }
    for (&p, &a) in pattern.iter().zip(atoms.iter()) {
        let current = map.get(&p).copied().unwrap_or(p);
        if current < 0 {
            map.insert(current, a);
        } else if current != a {
            return false;
        }
    }
    true
}

/// Applies a variable binding to every atom of every edge in place.
pub(crate) fn substitute(patterns: &mut [Vec<Atom>], map: &HashMap<Atom, Atom>) {
    for edge in patterns.iter_mut() {
        for atom in edge.iter_mut() {
            if let Some(&v) = map.get(atom) {
                *atom = v;
            }
        }
    }
}
