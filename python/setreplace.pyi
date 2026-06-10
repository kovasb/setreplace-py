"""Wolfram model (SetReplace) hypergraph substitution and visualization."""

from typing import Union

__version__: str

Rules = Union["Rule", str, list[Union["Rule", str]]]

class SetReplaceError(Exception):
    """Evolution-time failure (e.g. disconnected rule inputs)."""

class Rule:
    def __init__(
        self,
        inputs: list[list[int | str]],
        outputs: list[list[int | str]],
    ) -> None:
        """Structured form: strings are pattern variables, ints are concrete atoms."""

    @staticmethod
    def parse(s: str) -> Rule:
        """Wolfram-ish text: '{{x, y}} -> {{x, y}, {y, z}}' or '{{a_, b_}} :> ...'."""

    @property
    def inputs(self) -> list[list[int]]: ...
    @property
    def outputs(self) -> list[list[int]]: ...

class Token:
    atoms: list[int]
    creator_event: int
    destroyer_event: int | None
    generation: int

class Event:
    rule: int | None
    inputs: list[int]
    outputs: list[int]
    generation: int

class Plot:
    @property
    def svg(self) -> str: ...
    def save(self, path: str) -> None:
        """Writes .svg or .png by extension (PNG rasterized in-process)."""
    def _repr_svg_(self) -> str: ...

class HypergraphSystem:
    def __init__(
        self,
        rules: Rules,
        initial_state: list[list[int]],
        *,
        event_ordering: list[str] | None = None,
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
    ) -> int: ...
    def replace_once(self) -> bool: ...
    @property
    def final_state(self) -> list[list[int]]: ...
    @property
    def termination_reason(self) -> str: ...
    @property
    def events_count(self) -> int: ...
    @property
    def generations_count(self) -> int: ...
    @property
    def final_atom_count(self) -> int: ...
    def state_after_event(self, k: int) -> list[list[int]]: ...
    def states_by_event(self) -> list[list[list[int]]]: ...
    def state_at_generation(self, g: int) -> list[list[int]]: ...
    def max_complete_generation(self) -> int: ...
    def tokens(self) -> list[Token]: ...
    def events(self) -> list[Event]: ...
    def causal_graph_edges(self, include_initial: bool = False) -> list[tuple[int, int]]: ...
    def causal_graph_dot(self, include_initial: bool = False) -> str: ...
    def plot(
        self,
        *,
        labels: bool | dict[int, str] | None = None,
        seed: int = 0,
        width: float = 478.0,
    ) -> Plot: ...
    def causal_graph_plot(
        self, *, include_initial: bool = False, width: float = 478.0
    ) -> Plot: ...

def plot(
    edges: list[list[int]],
    *,
    labels: bool | dict[int, str] | None = None,
    seed: int = 0,
    width: float = 478.0,
) -> Plot: ...
def layout(edges: list[list[int]], *, seed: int = 0) -> dict[int, tuple[float, float]]: ...
def evolve(
    rules: Rules,
    initial_state: list[list[int]],
    *,
    generations: int | None = None,
    events: int | None = None,
    max_vertices: int | None = None,
    max_vertex_degree: int | None = None,
    max_edges: int | None = None,
    event_ordering: list[str] | None = None,
    random_seed: int = 0,
) -> HypergraphSystem: ...
def enumerate_rules(
    inputs: list[tuple[int, int]] | list[list[int]],
    outputs: list[tuple[int, int]] | list[list[int]],
    *,
    connectivity: str = "Automatic",
    max_elements: int | None = None,
) -> list[Rule]:
    """All inequivalent rules of a signature ((count, arity) pairs per side),
    replicating EnumerateWolframModelRules: same canonical forms, same order.
    connectivity: "Automatic" (LHS connected, rule connected), "All", "None"."""

def set_replace(state: list[list[int]], rules: Rules, events: int = 1) -> list[list[int]]: ...
def set_replace_list(
    state: list[list[int]], rules: Rules, events: int
) -> list[list[list[int]]]: ...
def set_replace_all(
    state: list[list[int]], rules: Rules, generations: int = 1
) -> list[list[int]]: ...
def set_replace_fixed_point(state: list[list[int]], rules: Rules) -> list[list[int]]: ...
