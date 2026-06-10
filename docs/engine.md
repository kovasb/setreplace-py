# The `setreplace` engine: scope, conventions, verification

This is the detailed companion to the repo README for the engine crate.

## Triage: what is the core?

SetReplace is a large package, but it has a clear center of mass: the C++
engine `libSetReplace` (`HypergraphSubstitutionSystem` + `HypergraphMatcher` +
`AtomsIndex` + `TokenEventGraph`, ~2600 lines). Everything else is either a
Wolfram Language front end to that engine or analysis/visualization layered on
its output. This crate reimplements that engine, plus the WL layer's
*semantic* defaults (event ordering, step counting, atom numbering), which are
part of the observable behavior.

**Included** (verified against SetReplace's own test vectors and live against
SetReplace 0.3.196 — see Verification):

- States as multisets of hyperedges over positive integer atoms; full
  token/event history with creator, destroyer, and generation bookkeeping.
- Pattern rules with variables, repeated variables, concrete atoms, and fresh
  atom creation for output-only variables (`Rule::parse` accepts
  `{{x, y}} -> {{x, y}, {y, z}}` and `{{a_, b_}} :> ...` syntax with
  `"PatternRules"` semantics: integers are concrete, identifiers are
  variables).
- The incremental matcher: anchor each new token at every rule position,
  complete the rest through an atom→tokens index picking the
  fewest-candidates position first, deduplicate by hash, reject rules whose
  inputs are not a connected hypergraph (`Error::DisconnectedInputs`),
  matching libSetReplace.
- Event selection: the full `"EventOrderingFunction"` set — `OldestEdge`,
  `LeastOldEdge`, `LeastRecentEdge`, `NewestEdge`, `RuleOrdering`,
  `ReverseRuleOrdering`, `RuleIndex`, `ReverseRuleIndex`, `Random`, `Any` —
  with the original architecture: a sorted queue of buckets of
  ordering-equivalent matches; the next event is drawn uniformly at random
  (seeded) from the first bucket, so randomness is the implicit final
  tie-breaker and `"Random"` is the empty specification. Default ordering is
  `{LeastRecentEdge, RuleOrdering, RuleIndex}`, as in `WolframModel`.
- Step limits and termination: `MaxEvents`, `MaxGenerations` (implemented as
  in the original — tokens at the cap are simply never indexed, and the cap
  may be raised across `evolve` calls), `MaxVertices`, `MaxVertexDegree`,
  `MaxEdges` (checked before the event that would violate them), plus
  `FixedPoint`.
- Evolution-object properties: `final_state`, `tokens` (all expressions),
  `events` (with the initial pseudo-event 0), `state_after_event` /
  `states_by_event` (= `SetReplaceList`), `state_at_generation` (= the
  evolution object's `[g]`), `generations_count`, `max_complete_generation`,
  `events_count`, and the causal graph as edges or Graphviz DOT *text*
  (boundary event excluded by default, as in `"CausalGraph"`).
- WL convenience equivalents: `set_replace`, `set_replace_list`,
  `set_replace_all`, `set_replace_fixed_point`.

**Excluded** (deliberately, for now):

- Multiway / local-multiway evolution (`maxDestroyerEvents > 1`), branchial
  graphs, and spacelike/branchlike/timelike separation tracking — the largest
  coherent omission; the single-history engine does not need any of it.
- `"EventDeduplication"` (same-input-set isomorphic-output merging).
- The WL symbolic fallback: non-hypergraph sets (e.g.
  `SetReplace[{1, 2, 3}, 2 -> 5]`), arbitrary WL patterns and conditions.
- Parallel matching, abort handling, `TimeConstraint`.
- All WL-side visualization (`HypergraphPlot` lives in `setreplace-viz`
  instead), the `GenerateMultihistory`/type-system layer, paclet/LibraryLink
  plumbing, and analysis utilities (isomorphism, unifications, dimension
  estimators, ...).

## Conventions

- Atoms are positive `i64` (the engine rejects others, like libSetReplace);
  in rules, negative atoms are pattern variables. Token and event IDs are
  0-based; **event 0 is the initial pseudo-event** that creates the initial
  state (WL reports the same structure 1-based).
- Fresh atoms created by an event are consecutive integers continuing after
  the largest atom named in the initial state or rules, assigned in order of
  first appearance in the event's outputs. This reproduces the vertex names
  `WolframModel` reports for integer initial conditions (verified live).
  Internally libSetReplace skips one integer (its counter is off by one from
  ours) and does not reserve rule atoms, but neither detail is observable
  through Wolfram Language.
- Determinism: with a complete ordering specification the evolution is fully
  deterministic. With ties, the choice is uniform over the tied matches from
  a seeded PCG32 (`EvolutionOptions::random_seed`); libSetReplace uses
  `std::mt19937` + `std::uniform_int_distribution`, whose stream is
  implementation-defined, so cross-implementation random *streams* differ by
  nature; the per-step distribution is the same. `Any` picks an unspecified
  but deterministic representative, as in the original.

## Verification

Three layers:

1. **Unit/property tests** for the parser, RNG, and matching semantics
   (repeated variables, token distinctness, non-injective bindings,
   disconnected-input rejection, cap behaviors, resumption with a raised
   generation cap).
2. **Test vectors lifted from SetReplace's own `.wlt` suite**
   (`tests/wolfram_vectors.rs` cites file and line for each): basic
   replacement, `SetReplaceList`, every event-ordering vector from
   `eventOrderingFunction.wlt`, step-spec/termination vectors from
   `WolframModel.wlt`.
3. **Live cross-check against SetReplace 0.3.196** via wolframscript:
   final states, state lists, event counts, generation counts, termination
   reasons, exact `"AllEventsList"` token-level traces, `"CausalGraph"` edge
   lists, all named edge orderings, generation states, and fresh-vertex
   naming all agree exactly.

## Performance

Per event the cost is O(log #matches) selection plus match discovery bounded
by the local connectivity around the consumed/created tokens — the same
architecture as libSetReplace (sorted bucket queue + atom index + most
constrained-first completion). On sparse models this sustains ~200k events/s
in release mode (`examples/bench.rs`); models that grow unbounded-degree hub
vertices are inherently slower for any matcher, since candidate sets scale
with vertex degree.
