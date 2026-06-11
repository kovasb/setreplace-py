# setreplace

**Wolfram models in Python** — hypergraph substitution systems with the exact
semantics of Max Piskunov's [SetReplace](https://github.com/maxitg/SetReplace),
plus `HypergraphPlot`-style rendering, powered by a Rust engine. No Wolfram
Language license, no graphviz, no native dependencies to install.

<img src="https://raw.githubusercontent.com/kovasb/setreplace-py/main/docs/images/showcase/announcement_web.png" width="500"
     alt="1500 events of the Wolfram Physics Project announcement's rule">

```python
import setreplace as sr

# The rule from the Wolfram Physics Project announcement
system = sr.evolve("{{x, y}, {x, z}} -> {{x, z}, {x, w}, {y, w}, {z, w}}",
                   [[1, 2], [2, 3], [3, 4], [2, 4]], events=1500)
system.plot()        # ↑ renders inline in Jupyter
```

States are plain `list[list[int]]`; rules are the same strings the
SetReplace docs use. Evolution is incremental and fully deterministic
(seeded), and every token and event keeps its causal history:

```python
rule = "{{v1, v2, v3}, {v2, v4, v5}} -> {{v5, v6, v1}, {v6, v4, v2}, {v4, v5, v3}}"
system = sr.evolve(rule, [[1, 2, 3], [2, 4, 5], [4, 6, 7]], events=10)

system                       # <HypergraphSystem: 10 events, 5 generations, 13 edges, MaxEvents>
system.final_state           # [[7, 2, 9], [7, 14, 6], ...]
system.evolve(max_events=100)
system.termination_reason    # "MaxEvents" | "FixedPoint" | "MaxGenerations" | ...
system.tokens()[0]           # Token(atoms=[1, 2, 3], creator_event=0, ...)
system.causal_graph_plot()   # layered causal graph, inline
system.plot().save("state.png")

sr.set_replace([[1, 2], [2, 3]], "{{a_, b_}, {b_, c_}} :> {{a, c}}")   # [[1, 3]]
pos = sr.layout(system.final_state)   # {atom: (x, y)} for matplotlib & friends
```

| Wolfram Language | Python |
|---|---|
| `WolframModel[rules, init, g]` | `sr.evolve(rules, init, generations=g)` |
| `SetReplace[set, rules, n]` | `sr.set_replace(set, rules, n)` |
| `<\|"MaxEvents" -> n, "MaxVertices" -> v\|>` | `system.evolve(max_events=n, max_vertices=v)` |
| `"EventOrderingFunction" -> {"OldestEdge", ...}` | `event_ordering=["OldestEdge", ...]` |
| `"CausalGraph"` / `"LayeredCausalGraph"` | `system.causal_graph_edges()` / `.causal_graph_plot()` |
| `HypergraphPlot[state, VertexLabels -> Automatic]` | `sr.plot(state, labels=True)` |

The engine is a verified port of SetReplace's C++ core: test vectors lifted
from SetReplace's own suite, plus live cross-checks against SetReplace under
wolframscript — final states, event traces, causal graphs, and every event
ordering agree exactly. The renderer's palette and arrowhead geometry are
transcribed from SetReplace's style sources, and large states lay out in
seconds via a multilevel spring-electrical embedding.

Docs, gallery, and the Rust crates:
[github.com/kovasb/setreplace-py](https://github.com/kovasb/setreplace-py).

Independent reimplementation of [SetReplace](https://github.com/maxitg/SetReplace)
(MIT) by Max Piskunov and contributors; not affiliated with the SetReplace
project, the Wolfram Physics Project, or Wolfram Research. MIT licensed.
