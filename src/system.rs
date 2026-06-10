//! The hypergraph substitution system (libSetReplace's
//! `HypergraphSubstitutionSystem` + `TokenEventGraph`), restricted to
//! single-history evolution.

use std::collections::{BTreeSet, HashMap};

use crate::atoms_index::AtomsIndex;
use crate::error::Error;
use crate::matcher::{default_event_ordering, unify_edge, Matcher, OrderingFunction};
use crate::rule::{Atom, Rule};

/// Identifies a token (hyperedge instance). Tokens are numbered in creation
/// order starting from 0; consumed tokens keep their record forever.
pub type TokenId = usize;

/// Identifies an event. Event 0 is the initial pseudo-event that creates the
/// initial state; real events are numbered from 1 in application order.
pub type EventId = usize;

/// Generation: the causal-graph layer. Initial tokens are generation 0; an
/// event's generation is `max(input generations) + 1`, and its outputs
/// inherit it.
pub type Generation = i64;

const DISABLED: i64 = i64::MAX;

/// A token: one hyperedge instance in the evolution's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub atoms: Vec<Atom>,
    pub creator_event: EventId,
    /// `None` while the token is part of the current state.
    pub destroyer_event: Option<EventId>,
    pub generation: Generation,
}

/// An applied substitution event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Index of the rule that fired; `None` for the initial pseudo-event.
    pub rule: Option<usize>,
    pub inputs: Vec<TokenId>,
    pub outputs: Vec<TokenId>,
    pub generation: Generation,
}

/// Limits at which evolution stops (libSetReplace `StepSpecification`).
/// `None` means unlimited. The default runs until a fixed point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepSpec {
    /// Maximum number of events (`"MaxEvents"`).
    pub max_events: Option<u64>,
    /// Maximum generation (`"MaxGenerations"`). Tokens of this generation are
    /// never matched, so evolution stops once every match would exceed it.
    pub max_generations: Option<u64>,
    /// Maximum number of distinct atoms in the final state (`"MaxVertices"`).
    /// Checked *before* applying each event; the event that would exceed it is
    /// not applied.
    pub max_vertices: Option<u64>,
    /// Maximum number of tokens any single atom may appear in
    /// (`"MaxVertexDegree"`).
    pub max_vertex_degree: Option<u64>,
    /// Maximum number of tokens in the final state (`"MaxEdges"`).
    pub max_edges: Option<u64>,
}

impl StepSpec {
    /// Limit by event count only.
    pub fn events(n: u64) -> StepSpec {
        StepSpec {
            max_events: Some(n),
            ..StepSpec::default()
        }
    }

    /// Limit by generation count only.
    pub fn generations(n: u64) -> StepSpec {
        StepSpec {
            max_generations: Some(n),
            ..StepSpec::default()
        }
    }
}

/// Why the last call to [`HypergraphSystem::evolve`] stopped. Names follow
/// the Wolfram Language `"TerminationReason"` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    /// No evolution has been requested yet.
    NotTerminated,
    MaxEvents,
    MaxGenerations,
    MaxVertices,
    MaxVertexDegree,
    MaxEdges,
    /// No matches remain: the system reached a fixed point.
    FixedPoint,
}

/// Construction-time options.
#[derive(Debug, Clone)]
pub struct EvolutionOptions {
    /// The `"EventOrderingFunction"` chain; defaults to
    /// `{LeastRecentEdge, RuleOrdering, RuleIndex}`.
    pub event_ordering: Vec<OrderingFunction>,
    /// Seed for the random tie-breaker (only consulted when the ordering
    /// chain leaves ties).
    pub random_seed: u64,
}

impl Default for EvolutionOptions {
    fn default() -> Self {
        EvolutionOptions {
            event_ordering: default_event_ordering(),
            random_seed: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct InternalSpec {
    max_events: i64,
    max_generations: i64,
    max_vertices: i64,
    max_vertex_degree: i64,
    max_edges: i64,
}

impl InternalSpec {
    /// libSetReplace initializes to all-zero: "don't evolve unless asked to".
    fn zero() -> Self {
        InternalSpec {
            max_events: 0,
            max_generations: 0,
            max_vertices: 0,
            max_vertex_degree: 0,
            max_edges: 0,
        }
    }

    fn from_spec(spec: &StepSpec) -> Self {
        fn conv(v: Option<u64>) -> i64 {
            v.map_or(DISABLED, |n| n.min(DISABLED as u64) as i64)
        }
        InternalSpec {
            max_events: conv(spec.max_events),
            max_generations: conv(spec.max_generations),
            max_vertices: conv(spec.max_vertices),
            max_vertex_degree: conv(spec.max_vertex_degree),
            max_edges: conv(spec.max_edges),
        }
    }
}

/// A single-history hypergraph substitution system: the core of
/// `WolframModel` / `SetReplace`.
pub struct HypergraphSystem {
    rules: Vec<Rule>,
    tokens: Vec<Token>,
    events: Vec<Event>,
    largest_generation: Generation,
    atoms_index: AtomsIndex,
    matcher: Matcher,
    /// Tokens created but not yet visible to the matcher. Indexing is lazy so
    /// that tokens at the generation cap are matched only if the cap is later
    /// raised (libSetReplace's `unindexedTokens_`).
    unindexed: Vec<TokenId>,
    next_atom: Atom,
    destroyed_token_count: usize,
    /// Number of current-state tokens each atom appears in. Drives the
    /// `MaxVertices` / `MaxVertexDegree` limits.
    atom_degrees: HashMap<Atom, i64>,
    spec: InternalSpec,
    termination_reason: TerminationReason,
}

impl HypergraphSystem {
    /// Creates a system with the default event ordering and seed 0.
    pub fn new(rules: Vec<Rule>, initial_state: Vec<Vec<Atom>>) -> Result<Self, Error> {
        Self::with_options(rules, initial_state, EvolutionOptions::default())
    }

    pub fn with_options(
        rules: Vec<Rule>,
        initial_state: Vec<Vec<Atom>>,
        options: EvolutionOptions,
    ) -> Result<Self, Error> {
        // Re-validate rules (their fields are public).
        for rule in &rules {
            if rule.inputs.is_empty() {
                return Err(Error::InvalidRule(
                    "rule must have at least one input edge".into(),
                ));
            }
            if rule
                .inputs
                .iter()
                .chain(rule.outputs.iter())
                .any(|e| e.contains(&0))
            {
                return Err(Error::InvalidRule("atom 0 is invalid".into()));
            }
        }
        let mut max_atom: Atom = 0;
        for edge in &initial_state {
            for &atom in edge {
                if atom <= 0 {
                    return Err(Error::NonPositiveAtoms);
                }
                max_atom = max_atom.max(atom);
            }
        }
        // Unlike libSetReplace, also reserve the rules' concrete atoms so a
        // fresh atom can never collide with an atom named in a rule.
        for rule in &rules {
            for edge in rule.inputs.iter().chain(rule.outputs.iter()) {
                for &atom in edge {
                    max_atom = max_atom.max(atom);
                }
            }
        }

        let tokens: Vec<Token> = initial_state
            .into_iter()
            .map(|atoms| Token {
                atoms,
                creator_event: 0,
                destroyer_event: None,
                generation: 0,
            })
            .collect();
        let initial_event = Event {
            rule: None,
            inputs: Vec::new(),
            outputs: (0..tokens.len()).collect(),
            generation: 0,
        };
        let mut atom_degrees = HashMap::new();
        for token in &tokens {
            update_atom_degrees(&mut atom_degrees, &token.atoms, 1, true);
        }

        Ok(HypergraphSystem {
            matcher: Matcher::new(&options.event_ordering, options.random_seed),
            rules,
            tokens,
            events: vec![initial_event],
            largest_generation: 0,
            atoms_index: AtomsIndex::default(),
            // Initial tokens are queued for indexing by the first
            // update_step_spec call, exactly as in libSetReplace.
            unindexed: Vec::new(),
            next_atom: max_atom + 1,
            destroyed_token_count: 0,
            atom_degrees,
            spec: InternalSpec::zero(),
            termination_reason: TerminationReason::NotTerminated,
        })
    }

    /// Runs events until a limit in `spec` is hit, a fixed point is reached,
    /// or an error occurs. Returns the number of events applied; consult
    /// [`HypergraphSystem::termination_reason`] for why it stopped.
    ///
    /// Can be called repeatedly; raising `max_generations` across calls
    /// resumes matching of previously generation-capped tokens.
    pub fn evolve(&mut self, spec: &StepSpec) -> Result<u64, Error> {
        self.update_step_spec(spec);
        let mut count = 0;
        while self.replace_once_internal()? {
            count += 1;
        }
        Ok(count)
    }

    /// Applies a single event with no limits (other than fixed point).
    /// Returns whether an event was applied.
    pub fn replace_once(&mut self) -> Result<bool, Error> {
        self.update_step_spec(&StepSpec::default());
        self.replace_once_internal()
    }

    fn update_step_spec(&mut self, spec: &StepSpec) {
        let previous_max_generation = self.spec.max_generations;
        self.spec = InternalSpec::from_spec(spec);
        if self.spec.max_generations > previous_max_generation {
            // Tokens *at* the previous cap were never indexed; they become
            // matchable now. (Tokens beyond it cannot exist.)
            for id in 0..self.tokens.len() {
                if self.tokens[id].generation == previous_max_generation {
                    debug_assert!(self.tokens[id].destroyer_event.is_none());
                    self.unindexed.push(id);
                }
            }
        }
    }

    fn index_new_tokens(&mut self) -> Result<(), Error> {
        if self.unindexed.is_empty() {
            return Ok(());
        }
        let tokens = &self.tokens;
        self.atoms_index
            .add_tokens(&self.unindexed, |id| tokens[id].atoms.as_slice());
        self.matcher.add_matches_involving_tokens(
            &self.unindexed,
            &self.rules,
            &self.tokens,
            &self.atoms_index,
        )?;
        self.unindexed.clear();
        Ok(())
    }

    /// One step of the evolution loop, mirroring libSetReplace's
    /// `replaceOnce`. Returns `Ok(true)` if an event was applied.
    fn replace_once_internal(&mut self) -> Result<bool, Error> {
        self.termination_reason = TerminationReason::NotTerminated;

        if self.events_count() as i64 >= self.spec.max_events {
            self.termination_reason = TerminationReason::MaxEvents;
            return Ok(false);
        }

        self.index_new_tokens()?;
        let m = match self.matcher.next_match() {
            None => {
                self.termination_reason =
                    if self.largest_generation == self.spec.max_generations {
                        TerminationReason::MaxGenerations
                    } else {
                        TerminationReason::FixedPoint
                    };
                return Ok(false);
            }
            Some(m) => m,
        };

        let rule = &self.rules[m.rule];
        let explicit_inputs: Vec<Vec<Atom>> = m
            .inputs
            .iter()
            .map(|&t| self.tokens[t].atoms.clone())
            .collect();
        // Outputs with bound variables substituted; output-only variables
        // stay negative ("anonymous") until named below.
        let explicit_outputs = output_atoms_vectors(rule, &explicit_inputs);

        // Final-state limits are checked before committing; hitting one
        // leaves the system untouched.
        for check in [
            Self::will_exceed_atom_limits,
            Self::will_exceed_token_limit,
        ] {
            let status = check(self, &explicit_inputs, &explicit_outputs);
            if status != TerminationReason::NotTerminated {
                self.termination_reason = status;
                return Ok(false);
            }
        }

        // Committed from here on.
        let named_outputs = self.name_anonymous_atoms(&explicit_outputs)?;

        let event_id = self.events.len();
        let generation = m
            .inputs
            .iter()
            .map(|&t| self.tokens[t].generation + 1)
            .max()
            .unwrap_or(0);

        let mut output_ids = Vec::with_capacity(named_outputs.len());
        for atoms in named_outputs {
            let id = self.tokens.len();
            update_atom_degrees(&mut self.atom_degrees, &atoms, 1, true);
            self.tokens.push(Token {
                atoms,
                creator_event: event_id,
                destroyer_event: None,
                generation,
            });
            // Tokens at the generation cap are kept out of the matcher.
            if generation < self.spec.max_generations {
                self.unindexed.push(id);
            }
            output_ids.push(id);
        }
        self.events.push(Event {
            rule: Some(m.rule),
            inputs: m.inputs.clone(),
            outputs: output_ids,
            generation,
        });
        self.largest_generation = self.largest_generation.max(generation);

        // Single-history semantics: consumed tokens leave the matchable set.
        for &t in &m.inputs {
            self.tokens[t].destroyer_event = Some(event_id);
        }
        self.matcher.remove_matches_involving_tokens(&m.inputs);
        let (atoms_index, tokens) = (&mut self.atoms_index, &self.tokens);
        atoms_index.remove_tokens(&m.inputs, |id| tokens[id].atoms.as_slice());
        self.destroyed_token_count += m.inputs.len();
        for &t in &m.inputs {
            let atoms = self.tokens[t].atoms.clone();
            update_atom_degrees(&mut self.atom_degrees, &atoms, -1, true);
        }

        Ok(true)
    }

    fn will_exceed_atom_limits(
        &self,
        explicit_inputs: &[Vec<Atom>],
        explicit_outputs: &[Vec<Atom>],
    ) -> TerminationReason {
        if self.spec.max_vertices == DISABLED && self.spec.max_vertex_degree == DISABLED {
            return TerminationReason::NotTerminated;
        }
        let mut deltas: HashMap<Atom, i64> = HashMap::new();
        for token in explicit_inputs {
            update_atom_degrees(&mut deltas, token, -1, false);
        }
        for token in explicit_outputs {
            update_atom_degrees(&mut deltas, token, 1, false);
        }
        let mut new_atoms_count = self.atom_degrees.len() as i64;
        for (&atom, &delta) in &deltas {
            let current = self.atom_degrees.get(&atom).copied().unwrap_or(0);
            if current == 0 && delta > 0 {
                new_atoms_count += 1;
            } else if current > 0 && current + delta == 0 {
                new_atoms_count -= 1;
            }
            if current + delta > self.spec.max_vertex_degree {
                return TerminationReason::MaxVertexDegree;
            }
        }
        if new_atoms_count > self.spec.max_vertices {
            TerminationReason::MaxVertices
        } else {
            TerminationReason::NotTerminated
        }
    }

    fn will_exceed_token_limit(
        &self,
        explicit_inputs: &[Vec<Atom>],
        explicit_outputs: &[Vec<Atom>],
    ) -> TerminationReason {
        if self.spec.max_edges == DISABLED {
            return TerminationReason::NotTerminated;
        }
        let current = (self.tokens.len() - self.destroyed_token_count) as i64;
        let new = current - explicit_inputs.len() as i64 + explicit_outputs.len() as i64;
        if new > self.spec.max_edges {
            TerminationReason::MaxEdges
        } else {
            TerminationReason::NotTerminated
        }
    }

    /// Gives fresh names to anonymous (negative) atoms: consecutive integers
    /// in order of first appearance, scanning output edges left to right.
    fn name_anonymous_atoms(&mut self, vectors: &[Vec<Atom>]) -> Result<Vec<Vec<Atom>>, Error> {
        let mut names: HashMap<Atom, Atom> = HashMap::new();
        let mut result = vectors.to_vec();
        for token in &mut result {
            for atom in token.iter_mut() {
                if *atom < 0 {
                    let named = match names.get(atom) {
                        Some(&n) => n,
                        None => {
                            if self.next_atom == Atom::MAX {
                                return Err(Error::AtomCountOverflow);
                            }
                            let n = self.next_atom;
                            self.next_atom += 1;
                            names.insert(*atom, n);
                            n
                        }
                    };
                    *atom = named;
                }
            }
        }
        Ok(result)
    }

    // ----- properties ------------------------------------------------------

    /// All tokens ever created, in creation order (`"AllExpressions"`).
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// All events; index 0 is the initial pseudo-event (`"AllEventsList"`
    /// with the initial boundary event included).
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Number of real events applied (`"EventsCount"`).
    pub fn events_count(&self) -> usize {
        self.events.len() - 1
    }

    pub fn termination_reason(&self) -> TerminationReason {
        self.termination_reason
    }

    /// Tokens of the current state, in creation order (`"FinalState"`).
    pub fn final_state(&self) -> Vec<Vec<Atom>> {
        self.tokens
            .iter()
            .filter(|t| t.destroyer_event.is_none())
            .map(|t| t.atoms.clone())
            .collect()
    }

    /// Number of distinct atoms in the current state.
    pub fn final_atom_count(&self) -> usize {
        self.atom_degrees.len()
    }

    /// The state after the first `k` events (`k = 0` is the initial state).
    pub fn state_after_event(&self, k: usize) -> Vec<Vec<Atom>> {
        assert!(k <= self.events_count(), "event index out of range");
        self.tokens
            .iter()
            .filter(|t| t.creator_event <= k && t.destroyer_event.is_none_or(|d| d > k))
            .map(|t| t.atoms.clone())
            .collect()
    }

    /// States after 0, 1, ..., `events_count()` events
    /// (`SetReplaceList` / `"AllEventsStatesList"`).
    pub fn states_by_event(&self) -> Vec<Vec<Vec<Atom>>> {
        (0..=self.events_count())
            .map(|k| self.state_after_event(k))
            .collect()
    }

    /// The state at generation `g`: tokens of generation ≤ g not destroyed by
    /// an event of generation ≤ g (the `WolframModel` evolution's `[g]`
    /// property).
    pub fn state_at_generation(&self, g: Generation) -> Vec<Vec<Atom>> {
        self.tokens
            .iter()
            .filter(|t| {
                t.generation <= g
                    && t.destroyer_event
                        .is_none_or(|d| self.events[d].generation > g)
            })
            .map(|t| t.atoms.clone())
            .collect()
    }

    /// Largest generation of any event (`"TotalGenerationsCount"`).
    pub fn generations_count(&self) -> Generation {
        self.largest_generation
    }

    /// Largest generation that is both reached and fully exhausted: no
    /// remaining match could produce a token at or below it
    /// (`"MaxCompleteGeneration"`).
    pub fn max_complete_generation(&mut self) -> Result<Generation, Error> {
        self.index_new_tokens()?;
        let smallest_match_generation = self
            .matcher
            .iter()
            .map(|m| {
                m.inputs
                    .iter()
                    .map(|&t| self.tokens[t].generation)
                    .max()
                    .unwrap_or(0)
            })
            .min()
            .unwrap_or(Generation::MAX);
        Ok(smallest_match_generation.min(self.largest_generation))
    }

    /// Causal-graph edges: one `(creator event, destroyer event)` pair per
    /// consumed token (multi-edges included). With `include_initial = false`
    /// (the `"CausalGraph"` default), edges from the initial pseudo-event are
    /// omitted; vertices are events `1..=events_count()`.
    pub fn causal_graph_edges(&self, include_initial: bool) -> Vec<(EventId, EventId)> {
        self.tokens
            .iter()
            .filter_map(|t| {
                let d = t.destroyer_event?;
                if !include_initial && t.creator_event == 0 {
                    return None;
                }
                Some((t.creator_event, d))
            })
            .collect()
    }

    /// The causal graph in Graphviz DOT format. Nodes are labeled with the
    /// event id and the rule that fired.
    pub fn causal_graph_dot(&self, include_initial: bool) -> String {
        let mut out = String::from("digraph causal {\n");
        let first = if include_initial { 0 } else { 1 };
        for id in first..self.events.len() {
            let label = match self.events[id].rule {
                Some(r) => format!("e{id} (rule {r})"),
                None => format!("e{id} (init)"),
            };
            out.push_str(&format!("  e{id} [label=\"{label}\"];\n"));
        }
        for (from, to) in self.causal_graph_edges(include_initial) {
            out.push_str(&format!("  e{from} -> e{to};\n"));
        }
        out.push_str("}\n");
        out
    }
}

/// Instantiates a rule's outputs for the given explicit input tokens
/// (libSetReplace's `matchOutputAtomsVectors`): builds one variable binding
/// across all input edges, then substitutes it into the outputs. Output-only
/// variables are left negative.
fn output_atoms_vectors(rule: &Rule, explicit_inputs: &[Vec<Atom>]) -> Vec<Vec<Atom>> {
    let mut map: HashMap<Atom, Atom> = HashMap::new();
    for (pattern, token) in rule.inputs.iter().zip(explicit_inputs.iter()) {
        let ok = unify_edge(pattern, token, &mut map);
        debug_assert!(ok, "stored match no longer unifies with its rule");
    }
    rule.outputs
        .iter()
        .map(|edge| {
            edge.iter()
                .map(|a| map.get(a).copied().unwrap_or(*a))
                .collect()
        })
        .collect()
}

/// Adds `delta` to the degree of each *distinct* atom of a token.
fn update_atom_degrees(
    degrees: &mut HashMap<Atom, i64>,
    token_atoms: &[Atom],
    delta: i64,
    delete_if_zero: bool,
) {
    let distinct: BTreeSet<Atom> = token_atoms.iter().copied().collect();
    for atom in distinct {
        let entry = degrees.entry(atom).or_insert(0);
        *entry += delta;
        if delete_if_zero && *entry == 0 {
            degrees.remove(&atom);
        }
    }
}
