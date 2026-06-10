# setreplace-viz

Pure-Rust layout and rendering for `setreplace` hypergraphs, reproducing the
aesthetics of SetReplace's `HypergraphPlot` and `"LayeredCausalGraph"`.
No graphviz anywhere; the only third-party dependency is `resvg` (PNG
rasterization).

```rust
use setreplace::*;
use setreplace_viz::*;

let rule = Rule::parse("{{x, y}} -> {{x, y}, {y, z}}").unwrap();
let mut system = HypergraphSystem::new(vec![rule], parse_state("{{1, 1}}").unwrap()).unwrap();
system.evolve(&StepSpec::generations(5)).unwrap();

let svg = hypergraph_plot_svg(&system.final_state(), &HypergraphPlotOptions::default());
svg_to_png(&svg, std::path::Path::new("state.png")).unwrap();

let causal = layered_causal_graph_svg(&system, &CausalGraphOptions::default());
```

## How fidelity is achieved

Everything observable was transcribed from the SetReplace sources rather than
eyeballed:

- **Style constants** (`src/style.rs`) come from `Kernel/A0$style.m`: the
  light-theme palette (vertex fill `Hue[0.63,0.26,0.89]`, edge lines
  `Hue[0.63,0.7,0.5]` at 0.7 opacity, hyperedge polygons at 0.1 opacity,
  causal-graph orange `Hue[0.11,1,0.97]` and dark red `Hue[0,1,0.56]`), the
  vertex radius (0.06), the arrowhead-length formula
  `clamp(0.066 + 0.017·plotRange, 0.1, 0.185)`, and the **exact arrowhead
  polygon** (15 points).
- **Drawing semantics** (`src/plot.rs`, `src/geometry.rs`) mirror
  `Kernel/HypergraphPlot.m` and `Kernel/arrow.m`: hyperedges are laid out as
  *cyclic* spring paths but drawn as *ordered* consecutive arrows; arity ≥ 3
  edges get the convex hull of their segment points as a borderless
  translucent polygon; lines are trimmed by the vertex radius at both ends
  and by the arrowhead length at the head; unary edges render as growing
  circles; parallel segments separate into symmetric Bézier arcs.
- **Layout** (`src/layout.rs`) is Yifan Hu's adaptive spring-electrical
  algorithm (the method behind Mathematica's `SpringElectricalEmbedding`),
  run per connected component, principal-axis aligned (Mathematica plots come
  out wide), shelf-packed across components, and rescaled so the **mean drawn
  edge length is 1** — the same normalization `rescaleEmbedding` performs,
  which is what makes vertex/arrowhead proportions consistent at every scale.
  Layouts are deterministic per seed.
- **Causal graphs** (`src/causal.rs`) use the exact layer assignment
  SetReplace passes to `"LayeredDigraphEmbedding"` (layer = event
  generation), with barycenter ordering and neighbor-mean coordinate
  relaxation, orange event disks, dark-red arrows, and curved multi-edges.

## Verification

`examples/readme_figures.rs` regenerates the SetReplace README's canonical
figure sequence (the initial hypergraph and the 1 / 10 / 100-event states of
the README's signature rule, plus a layered causal graph) from the Rust
engine. `examples/compose_comparison.rs` builds side-by-side images against
renders of the *identical* states produced by the real SetReplace paclet via
wolframscript (`out/comparison/*_wolfram_vs_rust.png`).

Known intentional differences: embeddings are different random minima of the
same energy (vertex positions differ, quality matches); label text follows
the README's proportions (Wolfram keeps label point size fixed when exporting
larger rasters); created vertices are labeled `v8, v9, ...` in figure code
(Wolfram's `v11, v12, ...` names come from a session-dependent `Unique`
counter).
