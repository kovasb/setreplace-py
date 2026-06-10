//! # setreplace
//!
//! A Rust reimplementation of the core of Wolfram's
//! [SetReplace](https://github.com/maxitg/SetReplace): single-history
//! hypergraph substitution systems (Wolfram model evolution).
//!
//! A *state* is a multiset of hyperedges — ordered lists of positive integer
//! atoms. A *rule* consumes a sub-multiset matching its input patterns and
//! produces new hyperedges, possibly creating fresh atoms. The engine tracks
//! every token (hyperedge instance) and event, which yields generations,
//! per-event/per-generation states, and the causal graph.
//!
//! ```
//! use setreplace::*;
//!
//! // SetReplace[{{1, 2}, {2, 3}}, {{a_, b_}, {b_, c_}} :> {{a, c}}]
//! let rule = Rule::parse("{{a_, b_}, {b_, c_}} :> {{a, c}}").unwrap();
//! let state = parse_state("{{1, 2}, {2, 3}}").unwrap();
//! assert_eq!(set_replace(&state, &[rule], 1).unwrap(), vec![vec![1, 3]]);
//!
//! // WolframModel[{{x, y}} -> {{x, y}, {y, z}}, {{1, 1}}, 5]
//! let rule = Rule::parse("{{x, y}} -> {{x, y}, {y, z}}").unwrap();
//! let mut system = HypergraphSystem::new(vec![rule], vec![vec![1, 1]]).unwrap();
//! system.evolve(&StepSpec::generations(5)).unwrap();
//! assert_eq!(system.final_state().len(), 32);
//! assert_eq!(system.events_count(), 31);
//! ```
//!
//! Semantics follow libSetReplace (the C++ engine inside SetReplace), with
//! the Wolfram Language layer's defaults: event ordering
//! `{LeastRecentEdge, RuleOrdering, RuleIndex}`, remaining ties broken
//! uniformly at random (seeded). See `docs/engine.md` in the repository for
//! the exact scope, conventions, and the few deliberate deviations.

mod atoms_index;
mod error;
mod matcher;
mod pcg;
mod rule;
mod system;

pub use error::Error;
pub use matcher::{default_event_ordering, OrderingFunction};
pub use rule::{parse_state, Atom, Rule};
pub use system::{
    Event, EventId, EvolutionOptions, Generation, HypergraphSystem, StepSpec,
    TerminationReason, Token, TokenId,
};

fn evolved(
    state: &[Vec<Atom>],
    rules: &[Rule],
    spec: &StepSpec,
) -> Result<HypergraphSystem, Error> {
    let mut system = HypergraphSystem::new(rules.to_vec(), state.to_vec())?;
    system.evolve(spec)?;
    Ok(system)
}

/// `SetReplace[state, rules, events]`: applies up to `events` substitution
/// events (in the default ordering) and returns the resulting state.
pub fn set_replace(
    state: &[Vec<Atom>],
    rules: &[Rule],
    events: u64,
) -> Result<Vec<Vec<Atom>>, Error> {
    Ok(evolved(state, rules, &StepSpec::events(events))?.final_state())
}

/// `SetReplaceList[state, rules, events]`: the list of states after 0, 1, ...
/// substitution events (stops early at a fixed point).
pub fn set_replace_list(
    state: &[Vec<Atom>],
    rules: &[Rule],
    events: u64,
) -> Result<Vec<Vec<Vec<Atom>>>, Error> {
    Ok(evolved(state, rules, &StepSpec::events(events))?.states_by_event())
}

/// `SetReplaceAll[state, rules, generations]`: evolves for the given number
/// of generations (each original token is replaced at most `generations`
/// times) and returns the resulting state.
pub fn set_replace_all(
    state: &[Vec<Atom>],
    rules: &[Rule],
    generations: u64,
) -> Result<Vec<Vec<Atom>>, Error> {
    Ok(evolved(state, rules, &StepSpec::generations(generations))?.final_state())
}

/// `SetReplaceFixedPoint[state, rules]`: evolves until no rule matches.
/// Beware: like its Wolfram Language counterpart, this does not terminate if
/// the system never reaches a fixed point.
pub fn set_replace_fixed_point(
    state: &[Vec<Atom>],
    rules: &[Rule],
) -> Result<Vec<Vec<Atom>>, Error> {
    Ok(evolved(state, rules, &StepSpec::default())?.final_state())
}
