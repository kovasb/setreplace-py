# Python API design

Design for Python bindings over the two existing crates (`setreplace`,
`setreplace-viz`). **Implemented** in [`python/`](../python/); the test suite
at `python/tests/test_api.py` exercises this contract end to end. One
refinement made during implementation: `labels=` defaults to `None`
(equivalent to `False`).

## Goals and philosophy

The primary consumer is a person in a Jupyter notebook exploring Wolfram
models. That drives every choice:

1. **Plain data at the boundary.** Hypergraphs are `list[list[int]]` in both
   directions. No `Edge`/`Hypergraph` wrapper classes to learn; states
   compose with ordinary Python (slicing, `len`, comprehensions,
   serialization).
2. **Strings where Wolfram uses strings.** Rules parse from the same textual
   form the SetReplace docs use; event orderings and termination reasons are
   the WL names (`"LeastRecentEdge"`, `"FixedPoint"`), not enum imports.
3. **Keyword arguments instead of spec objects.** `evolve(max_events=100)`
   rather than a `StepSpec` class; Python has kwargs, so use them.
4. **Figures render themselves.** Plot calls return a `Plot` object with
   `_repr_svg_`, so a bare expression in a notebook cell displays inline —
   the same affordance as a `WolframModel[...]["FinalStatePlot"]` output
   cell.
5. **Mirror the Rust/WL semantics exactly.** Same defaults (event ordering,
   seeds), same determinism guarantees, same 0-based ids with event 0 as the
   initial pseudo-event. Anything documented for the Rust crate holds.

## Package

- **Distribution / import name**: `setreplace` (check PyPI availability at
  publish time; fallback `setreplace-rs` as the distribution name with
  `import setreplace` unchanged).
- One PyO3 `cdylib` extension module built with maturin (`abi3-py39`, so one
  wheel per platform). No pure-Python shim layer; PyO3 carries docstrings,
  properties, and `__repr__`s. Ship `py.typed` + a generated `.pyi` stub.
- New workspace member `python/` depending on `setreplace` + `setreplace-viz`.

## The surface (stub form)

```python
# --- rules ------------------------------------------------------------

class Rule:
    """A substitution rule on ordered hypergraphs."""

    def __init__(self, inputs: list[list[int | str]],
                 outputs: list[list[int | str]]) -> None:
        """Structured form: strings are pattern variables, ints are
        concrete atoms. Rule([["x","y"]], [["x","y"],["y","z"]])."""

    @staticmethod
    def parse(s: str) -> Rule:
        """Wolfram-ish text: Rule.parse('{{x, y}} -> {{x, y}, {y, z}}'),
        also accepts {{a_, b_}} :> ... blank syntax."""

    @property
    def inputs(self) -> list[list[int]]: ...   # variables as negative ints
    @property
    def outputs(self) -> list[list[int]]: ...
    def __str__(self) -> str: ...              # round-trips through parse

# --- the system -------------------------------------------------------

class HypergraphSystem:
    def __init__(
        self,
        rules: Rule | list[Rule],
        initial_state: list[list[int]],
        *,
        event_ordering: list[str] | None = None,  # default: ["LeastRecentEdge", "RuleOrdering", "RuleIndex"]
        random_seed: int = 0,
    ) -> None: ...

    def evolve(
        self,
        *,
        max_events: int | None = None,
        max_generations: int | None = None,
        max_vertices: int | None = None,
        max_vertex_degree: int | None = None,
        max_edges: int | None = None,
    ) -> int:
        """Runs events until a limit or fixed point; returns events applied.
        No limits = run to fixed point (may not terminate, as in WL).
        Callable repeatedly; raising max_generations resumes capped tokens.
        Releases the GIL while running."""

    def replace_once(self) -> bool: ...

    # -- state ----------------------------------------------------------
    @property
    def final_state(self) -> list[list[int]]: ...
    @property
    def termination_reason(self) -> str:
        """'NotTerminated' | 'MaxEvents' | 'MaxGenerations' | 'MaxVertices'
        | 'MaxVertexDegree' | 'MaxEdges' | 'FixedPoint'"""
    @property
    def events_count(self) -> int: ...
    @property
    def generations_count(self) -> int: ...
    @property
    def final_atom_count(self) -> int: ...

    def state_after_event(self, k: int) -> list[list[int]]: ...
    def states_by_event(self) -> list[list[list[int]]]: ...   # SetReplaceList
    def state_at_generation(self, g: int) -> list[list[int]]: ...
    def max_complete_generation(self) -> int: ...

    # -- history --------------------------------------------------------
    def tokens(self) -> list[Token]: ...     # every hyperedge ever
    def events(self) -> list[Event]: ...     # [0] is the initial pseudo-event
    def causal_graph_edges(self, include_initial: bool = False) -> list[tuple[int, int]]: ...
    def causal_graph_dot(self, include_initial: bool = False) -> str: ...

    # -- figures ----------------------------------------------------------
    def plot(self, *, labels: bool | dict[int, str] = False,
             seed: int = 0, width: float = 478.0) -> Plot:
        """HypergraphPlot of the current state."""
    def causal_graph_plot(self, *, include_initial: bool = False,
                          width: float = 478.0) -> Plot: ...

    def __repr__(self) -> str:
        # <HypergraphSystem: 31 events, 5 generations, 32 edges, MaxGenerations>
        ...

class Token:
    atoms: list[int]
    creator_event: int
    destroyer_event: int | None
    generation: int

class Event:
    rule: int | None            # None for the initial pseudo-event
    inputs: list[int]           # token ids
    outputs: list[int]
    generation: int

# --- plotting (state-independent) --------------------------------------

class Plot:
    @property
    def svg(self) -> str: ...
    def save(self, path: str) -> None:
        """Writes .svg or .png by extension (PNG via resvg, no external tools)."""
    def _repr_svg_(self) -> str: ...   # inline display in Jupyter

def plot(edges: list[list[int]], *, labels: bool | dict[int, str] = False,
         seed: int = 0, width: float = 478.0) -> Plot:
    """HypergraphPlot of any hypergraph (binary edges = ordinary digraph)."""

def layout(edges: list[list[int]], *, seed: int = 0) -> dict[int, tuple[float, float]]:
    """Just the spring-electrical vertex positions (mean edge length 1),
    for custom drawing with matplotlib & friends."""

# --- Wolfram Language flavored conveniences -----------------------------

def evolve(rules, initial_state, *, generations=None, events=None, **limits) -> HypergraphSystem:
    """One-liner standing in for WolframModel[rules, init, steps]:
    constructs a system, evolves it, returns it."""

def set_replace(state, rules, events: int = 1) -> list[list[int]]: ...
def set_replace_list(state, rules, events: int) -> list[list[list[int]]]: ...
def set_replace_all(state, rules, generations: int = 1) -> list[list[int]]: ...
def set_replace_fixed_point(state, rules) -> list[list[int]]: ...

class SetReplaceError(Exception):
    """Evolution-time failures (e.g. disconnected rule inputs)."""
```

Anywhere a `rules` argument appears, a single `Rule` or a `str` (parsed) is
accepted as well as lists of either — `set_replace(state, "{{a_,b_},{b_,c_}} :> {{a,c}}")`
works.

## A notebook session

```python
import setreplace as sr

rule = sr.Rule.parse(
    "{{v1, v2, v3}, {v2, v4, v5}} -> {{v5, v6, v1}, {v6, v4, v2}, {v4, v5, v3}}")

system = sr.evolve(rule, [[1, 2, 3], [2, 4, 5], [4, 6, 7]], events=10)
system                      # <HypergraphSystem: 10 events, 4 generations, 13 edges, MaxEvents>
system.final_state          # [[7, 2, 9], [7, 14, 6], ...]
system.plot(labels=True)    # renders inline
system.causal_graph_plot()  # renders inline
system.plot().save("state.png")

# Continue evolving the same system, then inspect history
system.evolve(max_events=100)
[t for t in system.tokens() if t.destroyer_event is None]   # current state, with metadata
system.causal_graph_edges()                                  # [(1, 3), (2, 3), ...]

# WL-style one-liners
sr.set_replace([[1, 2], [2, 3]], "{{a_, b_}, {b_, c_}} :> {{a, c}}")   # [[1, 3]]

# Custom drawing via raw positions
pos = sr.layout(system.final_state, seed=1)
```

## Design decisions and rationale

- **`evolve()` kwargs return the count, reason is a property** — matches the
  Rust split and lets `evolve()` be called repeatedly in a loop with
  different limits, which is a real workflow (anneal generations upward).
- **Termination reasons / orderings as strings, not enums.** WL names are
  the lingua franca of the SetReplace docs; strings keep notebook code free
  of imports. Validated eagerly with helpful errors listing valid names.
- **`labels=True | dict`.** `True` labels every vertex with `str(atom)`;
  a dict gives full control (e.g. README-style `v8` names). Default off,
  matching `HypergraphPlot`.
- **`tokens()`/`events()` are methods, state summaries are properties.**
  Properties for O(1)-ish summaries you'd type in a REPL without parens;
  methods for calls that materialize history lists.
- **`layout()` exposed.** Cheap to provide and makes the library a useful
  *layout engine* for the Python ecosystem even when our renderer isn't
  wanted (matplotlib, plotly, manim...).
- **Errors**: invalid rules/states/parse → `ValueError` at construction;
  evolution-time failures (disconnected rule inputs, atom overflow) →
  `SetReplaceError`. Both carry the Rust error message verbatim.
- **Threading**: `evolve`, `plot`, and `layout` release the GIL; a
  `HypergraphSystem` is not itself thread-safe (mutable, like the Rust type)
  and PyO3 enforces exclusive access.
- **No numpy dependency.** States are ragged (mixed arities); `list[list[int]]`
  is the honest representation. `layout()` returns plain tuples for the same
  reason. Numpy interop is a one-liner on the user side when wanted.

## Open items

1. ~~PyPI name availability for `setreplace`~~ — confirmed free; the
   distribution and import name are both `setreplace` (repo:
   [kovasb/setreplace-py](https://github.com/kovasb/setreplace-py)).
2. Wheels: start with macOS arm64 + manylinux x86_64/aarch64 via maturin;
   abi3 keeps it to one wheel per platform.
3. Later, when multiway lands in the engine, `evolve()` gains kwargs rather
   than new entry points — worth keeping in mind when naming things now.
