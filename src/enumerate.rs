//! Enumeration of all inequivalent rules of a given signature, replicating
//! the Wolfram Physics Project's `EnumerateWolframModelRules`.
//!
//! A *signature* gives the number of hyperedges of each arity on each side,
//! e.g. 2₂ → 3₂ (`{{2, 2}} -> {{3, 2}}` in WL notation) is two binary edges
//! rewriting to three. Two rules are equivalent if one becomes the other by
//! renaming elements and reordering edges within each side; the canonical
//! representative is the lexicographically first form with elements numbered
//! from 1 (wolframphysics.org, "The Representation of Rules").
//!
//! Output order matches `EnumerateWolframModelRules`: sorted by the
//! element-novelty pattern (which slots, scanning the rule left to right,
//! introduce a previously unseen element), then lexicographically — verified
//! against captured ground truth in tests/fixtures/.

use std::collections::HashMap;

use crate::error::Error;
use crate::rule::{Atom, Rule};

/// A rule signature: hyperedge `(count, arity)` groups for each side.
/// `{{2, 2}} -> {{3, 2}}` is `inputs: [(2, 2)], outputs: [(3, 2)]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSignature {
    pub inputs: Vec<(usize, usize)>,
    pub outputs: Vec<(usize, usize)>,
}

/// Connectivity constraints, as in `EnumerateWolframModelRules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    /// No constraint.
    None,
    /// The default ("Automatic"): the left-hand side must be connected, and
    /// the rule as a whole (both sides together) must be connected.
    LeftConnected,
    /// "All": additionally the right-hand side must be connected on its own.
    All,
}

#[derive(Debug, Clone)]
pub struct EnumerationOptions {
    pub connectivity: Connectivity,
    /// Maximum number of distinct elements in a rule (`None` = unlimited).
    pub max_elements: Option<usize>,
}

impl Default for EnumerationOptions {
    fn default() -> Self {
        EnumerationOptions {
            connectivity: Connectivity::LeftConnected,
            max_elements: None,
        }
    }
}

/// All inequivalent rules of the signature, in `EnumerateWolframModelRules`
/// order. Elements become pattern variables in the returned [`Rule`]s
/// (canonical element k ↦ variable -k).
pub fn enumerate_rules(
    signature: &RuleSignature,
    options: &EnumerationOptions,
) -> Result<Vec<Rule>, Error> {
    let lhs_arities = expand_signature_side(&signature.inputs)?;
    let rhs_arities = expand_signature_side(&signature.outputs)?;
    if lhs_arities.is_empty() {
        return Err(Error::InvalidRule(
            "signature must have at least one input edge".into(),
        ));
    }

    let slot_count: usize = lhs_arities.iter().chain(rhs_arities.iter()).sum();
    let max_elements = options.max_elements.unwrap_or(slot_count).min(slot_count);

    let mut found: Vec<Vec<usize>> = Vec::new();
    let mut slots = vec![0usize; slot_count];
    generate(0, 0, max_elements, &mut slots, &mut |slots: &[usize]| {
        let (lhs, rhs) = split_edges(slots, &lhs_arities, &rhs_arities);
        if !passes_connectivity(&lhs, &rhs, options.connectivity) {
            return;
        }
        if is_canonical(&lhs, &rhs) {
            found.push(slots.to_vec());
        }
    });

    // EnumerateWolframModelRules order: novelty pattern, then lexicographic.
    found.sort_by(|a, b| {
        novelty_pattern(a)
            .cmp(&novelty_pattern(b))
            .then_with(|| a.cmp(b))
    });

    Ok(found
        .into_iter()
        .map(|slots| {
            let (lhs, rhs) = split_edges(&slots, &lhs_arities, &rhs_arities);
            let to_vars = |edges: Vec<Vec<usize>>| -> Vec<Vec<Atom>> {
                edges
                    .into_iter()
                    .map(|e| e.into_iter().map(|k| -(k as Atom)).collect())
                    .collect()
            };
            Rule {
                inputs: to_vars(lhs),
                outputs: to_vars(rhs),
            }
        })
        .collect())
}

/// The canonical integer form (elements numbered from 1) of a rule produced
/// by [`enumerate_rules`] — i.e. the form `EnumerateWolframModelRules`
/// prints. Inverse of the variable encoding.
pub fn canonical_integer_form(rule: &Rule) -> (Vec<Vec<i64>>, Vec<Vec<i64>>) {
    let convert = |edges: &[Vec<Atom>]| -> Vec<Vec<i64>> {
        edges
            .iter()
            .map(|e| e.iter().map(|&a| if a < 0 { -a } else { a }).collect())
            .collect()
    };
    (convert(&rule.inputs), convert(&rule.outputs))
}

fn expand_signature_side(groups: &[(usize, usize)]) -> Result<Vec<usize>, Error> {
    // EnumerateWolframModelRules sorts each side's signature groups by
    // descending arity (ReverseSortBy[#, Last]) before generating.
    let mut groups = groups.to_vec();
    groups.sort_by_key(|&(_, arity)| std::cmp::Reverse(arity));
    let mut arities = Vec::new();
    for &(count, arity) in &groups {
        if arity == 0 {
            return Err(Error::InvalidRule(
                "signature arities must be at least 1".into(),
            ));
        }
        arities.extend(std::iter::repeat_n(arity, count));
    }
    Ok(arities)
}

/// Restricted-growth generation: each slot takes an existing element or the
/// next unused one, so every renaming class is generated exactly once per
/// edge ordering.
fn generate(
    index: usize,
    seen: usize,
    max_elements: usize,
    slots: &mut Vec<usize>,
    emit: &mut impl FnMut(&[usize]),
) {
    if index == slots.len() {
        emit(slots);
        return;
    }
    let limit = (seen + 1).min(max_elements);
    for value in 1..=limit {
        slots[index] = value;
        generate(index + 1, seen.max(value), max_elements, slots, emit);
    }
}

fn split_edges(
    slots: &[usize],
    lhs_arities: &[usize],
    rhs_arities: &[usize],
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut edges = Vec::with_capacity(lhs_arities.len() + rhs_arities.len());
    let mut at = 0;
    for &arity in lhs_arities.iter().chain(rhs_arities.iter()) {
        edges.push(slots[at..at + arity].to_vec());
        at += arity;
    }
    let rhs = edges.split_off(lhs_arities.len());
    (edges, rhs)
}

fn passes_connectivity(lhs: &[Vec<usize>], rhs: &[Vec<usize>], connectivity: Connectivity) -> bool {
    match connectivity {
        Connectivity::None => true,
        Connectivity::LeftConnected => {
            edges_connected(lhs.iter()) && edges_connected(lhs.iter().chain(rhs.iter()))
        }
        Connectivity::All => {
            edges_connected(lhs.iter())
                && edges_connected(rhs.iter())
                && edges_connected(lhs.iter().chain(rhs.iter()))
        }
    }
}

/// Whether the hyperedges form a single connected component (edges sharing
/// an element are connected). Zero edges count as connected.
fn edges_connected<'a>(edges: impl Iterator<Item = &'a Vec<usize>>) -> bool {
    let edges: Vec<&Vec<usize>> = edges.collect();
    if edges.len() <= 1 {
        return true;
    }
    let mut component: HashMap<usize, usize> = HashMap::new();
    let mut parent: Vec<usize> = (0..edges.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for (i, edge) in edges.iter().enumerate() {
        for &element in edge.iter() {
            match component.get(&element) {
                Some(&j) => {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                    if a != b {
                        parent[a] = b;
                    }
                }
                None => {
                    component.insert(element, i);
                }
            }
        }
    }
    let root = find(&mut parent, 0);
    (1..edges.len()).all(|i| find(&mut parent, i) == root)
}

/// A candidate is canonical iff it equals its canonical form.
fn is_canonical(lhs: &[Vec<usize>], rhs: &[Vec<usize>]) -> bool {
    let (cl, cr) = canonical_form(lhs, rhs);
    cl == lhs && cr == rhs
}

/// The canonical form, replicating `xFindCanonicalWolframModel` exactly:
/// each side's edges are sorted by descending arity and split into arity
/// groups; within each group the candidate edge orderings come from a greedy
/// search that minimizes the number of distinct elements covered by every
/// prefix (ties kept, then narrowed to the orderings with the smallest
/// group-locally renumbered form); finally, the combination of group
/// orderings whose whole-rule renumbered sequence is lexicographically
/// smallest wins, and the rule is renumbered by first occurrence.
fn canonical_form(lhs: &[Vec<usize>], rhs: &[Vec<usize>]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let arity_groups = |side: &[Vec<usize>]| -> Vec<Vec<Vec<usize>>> {
        let mut edges = side.to_vec();
        edges.sort_by_key(|e| std::cmp::Reverse(e.len())); // stable
        let mut groups: Vec<Vec<Vec<usize>>> = Vec::new();
        for edge in edges {
            match groups.last_mut() {
                Some(group) if group[0].len() == edge.len() => group.push(edge),
                _ => groups.push(vec![edge]),
            }
        }
        groups
    };
    let lhs_groups = arity_groups(lhs);
    let rhs_groups = arity_groups(rhs);
    let lhs_group_count = lhs_groups.len();
    let groups: Vec<Vec<Vec<usize>>> = lhs_groups.into_iter().chain(rhs_groups).collect();

    let candidates: Vec<Vec<Vec<Vec<usize>>>> = groups.iter().map(|g| miser_orderings(g)).collect();

    // Cross product of per-group candidates; minimize the renumbered flat
    // sequence of the whole rule.
    let mut best: Option<(Vec<usize>, Vec<usize>)> = None; // (key, combo indices)
    let mut combo = vec![0usize; candidates.len()];
    loop {
        let flat: Vec<usize> = combo
            .iter()
            .enumerate()
            .flat_map(|(g, &c)| candidates[g][c].iter().flatten().copied())
            .collect();
        let key = renumber_flat(&flat);
        if best.as_ref().is_none_or(|(bk, _)| key < *bk) {
            best = Some((key, combo.clone()));
        }
        // odometer
        let mut at = 0;
        loop {
            if at == combo.len() {
                break;
            }
            combo[at] += 1;
            if combo[at] < candidates[at].len() {
                break;
            }
            combo[at] = 0;
            at += 1;
        }
        if at == combo.len() {
            break;
        }
    }
    let (_, combo) = best.unwrap();

    // Rebuild the chosen arrangement and renumber by first occurrence.
    let chosen: Vec<&Vec<Vec<usize>>> = combo
        .iter()
        .enumerate()
        .map(|(g, &c)| &candidates[g][c])
        .collect();
    let mut map: HashMap<usize, usize> = HashMap::new();
    let mut next = 0usize;
    let mut renumbered_groups: Vec<Vec<Vec<usize>>> = Vec::new();
    for group in chosen {
        renumbered_groups.push(
            group
                .iter()
                .map(|edge| {
                    edge.iter()
                        .map(|&e| {
                            *map.entry(e).or_insert_with(|| {
                                next += 1;
                                next
                            })
                        })
                        .collect()
                })
                .collect(),
        );
    }
    let rhs_edges: Vec<Vec<usize>> = renumbered_groups
        .split_off(lhs_group_count)
        .into_iter()
        .flatten()
        .collect();
    let lhs_edges: Vec<Vec<usize>> = renumbered_groups.into_iter().flatten().collect();
    (lhs_edges, rhs_edges)
}

/// `MiserTermsInTuples`: candidate orderings of one arity group's edges.
/// Identical edges stay adjacent (they are gathered and moved as a unit).
fn miser_orderings(group: &[Vec<usize>]) -> Vec<Vec<Vec<usize>>> {
    // Gather identical edges in first-appearance order.
    let mut distinct: Vec<(Vec<usize>, usize)> = Vec::new();
    for edge in group {
        match distinct.iter_mut().find(|(d, _)| d == edge) {
            Some((_, count)) => *count += 1,
            None => distinct.push((edge.clone(), 1)),
        }
    }
    let m = distinct.len();
    let distinct_count = |seq: &[usize]| -> usize {
        let mut elements: Vec<usize> = seq
            .iter()
            .flat_map(|&i| distinct[i].0.iter().copied())
            .collect();
        elements.sort_unstable();
        elements.dedup();
        elements.len()
    };

    let mut seqs: Vec<Vec<usize>> = (0..m).map(|i| vec![i]).collect();
    for _ in 0..m.saturating_sub(1) {
        // Keep only the prefixes covering the fewest distinct elements...
        let min = seqs.iter().map(|s| distinct_count(s)).min().unwrap();
        seqs.retain(|s| distinct_count(s) == min);
        // ...then extend each by every unused edge.
        seqs = seqs
            .iter()
            .flat_map(|s| {
                (0..m)
                    .filter(|i| !s.contains(i))
                    .map(|i| {
                        let mut t = s.clone();
                        t.push(i);
                        t
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
    }

    let mut arrangements: Vec<Vec<Vec<usize>>> = seqs
        .iter()
        .map(|s| {
            s.iter()
                .flat_map(|&i| std::iter::repeat_n(distinct[i].0.clone(), distinct[i].1))
                .collect()
        })
        .collect();
    arrangements.sort();
    arrangements.dedup();
    // Keep the orderings tied for the smallest group-local renumbered form.
    let key = |arr: &Vec<Vec<usize>>| -> Vec<usize> {
        renumber_flat(&arr.iter().flatten().copied().collect::<Vec<_>>())
    };
    let min = arrangements.iter().map(key).min().unwrap();
    arrangements.retain(|a| key(a) == min);
    arrangements
}

/// First-occurrence renumbering of a flat element sequence (`DelDup`).
fn renumber_flat(flat: &[usize]) -> Vec<usize> {
    let mut map: HashMap<usize, usize> = HashMap::new();
    let mut next = 0usize;
    flat.iter()
        .map(|&e| {
            *map.entry(e).or_insert_with(|| {
                next += 1;
                next
            })
        })
        .collect()
}

/// Which slots introduce a previously unseen element (true = new).
fn novelty_pattern(slots: &[usize]) -> Vec<bool> {
    let mut seen = 0usize;
    slots
        .iter()
        .map(|&v| {
            if v > seen {
                seen = v;
                true
            } else {
                false
            }
        })
        .collect()
}
