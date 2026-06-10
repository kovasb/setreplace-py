# Using setreplace from Rust

The Python package is a thin layer over two Rust crates, both usable
directly:

| crate | what it is | dependencies |
|---|---|---|
| `setreplace` (repo root) | the substitution engine (port of `libSetReplace`, single-history) | none |
| `setreplace-viz` ([viz/](../viz/)) | multilevel spring-electrical layout + SVG/PNG rendering in SetReplace's style | `resvg` |

```rust
use setreplace::*;
use setreplace_viz::*;

// WolframModel[{{x, y}} -> {{x, y}, {y, z}}, {{1, 1}}, 5]
let rule = Rule::parse("{{x, y}} -> {{x, y}, {y, z}}")?;
let mut system = HypergraphSystem::new(vec![rule], parse_state("{{1, 1}}")?)?;
system.evolve(&StepSpec::generations(5))?;

assert_eq!(system.final_state().len(), 32);          // "FinalState"
assert_eq!(system.events_count(), 31);               // "EventsCount"
assert_eq!(system.generations_count(), 5);           // "TotalGenerationsCount"

let svg = hypergraph_plot_svg(&system.final_state(), &HypergraphPlotOptions::default());
svg_to_png(&svg, std::path::Path::new("state.png"))?;
let causal = layered_causal_graph_svg(&system, &CausalGraphOptions::default());
```

Evolution under explicit options and limits:

```rust
let mut system = HypergraphSystem::with_options(rules, init, EvolutionOptions {
    event_ordering: default_event_ordering(), // {LeastRecentEdge, RuleOrdering, RuleIndex}
    random_seed: 42,                          // seeded tie-breaking
})?;
system.evolve(&StepSpec { max_events: Some(100), ..Default::default() })?;
system.termination_reason();                  // MaxEvents / MaxGenerations / FixedPoint / ...
system.tokens();                              // every hyperedge ever, with creator/destroyer/generation
system.events();                              // every event; [0] is the initial pseudo-event
system.states_by_event();                     // SetReplaceList
system.state_at_generation(2);                // evolution[2]
system.causal_graph_edges(false);             // (creator event, destroyer event) pairs
```

Rough Wolfram Language ↔ Rust dictionary:

| Wolfram Language | here |
|---|---|
| `SetReplace[set, rules, n]` | `set_replace(&set, &rules, n)` |
| `SetReplaceList[set, rules, n]` | `set_replace_list(&set, &rules, n)` |
| `SetReplaceAll[set, rules, g]` | `set_replace_all(&set, &rules, g)` |
| `SetReplaceFixedPoint[set, rules]` | `set_replace_fixed_point(&set, &rules)` |
| `WolframModel[rules, init, g]` | `system.evolve(&StepSpec::generations(g))` |
| `<\|"MaxEvents" -> n, "MaxVertices" -> v\|>` | `StepSpec { max_events, max_vertices, ... }` |
| `"EventOrderingFunction" -> {...}` | `EvolutionOptions::event_ordering` |
| `"FinalState"` / `"EventsCount"` / `"TerminationReason"` | `final_state()` / `events_count()` / `termination_reason()` |
| `"CausalGraph"` / `"LayeredCausalGraph"` | `causal_graph_edges()` / `layered_causal_graph_svg()` |
| `HypergraphPlot[state]` | `hypergraph_plot_svg(&state, &opts)` |

Deeper documentation:

- [docs/engine.md](engine.md) — engine scope/triage, exact conventions
  (0-based ids, event 0 = initial pseudo-event, fresh-atom naming),
  verification story, performance notes (~200k events/s on sparse models).
- [viz/README.md](../viz/README.md) — how the renderer achieves fidelity
  (style constants and arrowhead geometry transcribed from SetReplace's
  sources; Hu's multilevel spring-electrical embedding; mean-edge-length
  normalization).

## Building

```bash
cargo test --workspace                                     # engine + viz tests
cargo run --release -p setreplace-viz --example readme_figures   # README figure set
cargo run --release -p setreplace-viz --example showcase   # gallery renders
cargo run --release --example bench                        # engine throughput
```
