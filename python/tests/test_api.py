"""End-to-end tests of the Python API against the contract in
docs/python-api.md. Plain asserts; run with `python tests/test_api.py`."""

import os
import tempfile

import setreplace as sr

GROWTH = "{{x, y}} -> {{x, y}, {y, z}}"
CHAIN = "{{a_, b_}, {b_, c_}} :> {{a, c}}"


def test_rule_parse_and_structured():
    parsed = sr.Rule.parse(GROWTH)
    structured = sr.Rule([["x", "y"]], [["x", "y"], ["y", "z"]])
    assert parsed.inputs == structured.inputs == [[-1, -2]]
    assert parsed.outputs == structured.outputs == [[-1, -2], [-2, -3]]
    # str() round-trips semantically.
    again = sr.Rule.parse(str(parsed))
    assert again.inputs == parsed.inputs and again.outputs == parsed.outputs
    assert "Rule.parse" in repr(parsed)
    # Concrete atoms stay concrete.
    concrete = sr.Rule([[1], [2]], [[3]])
    assert concrete.inputs == [[1], [2]]


def test_growth_evolution():
    system = sr.HypergraphSystem(sr.Rule.parse(GROWTH), [[1, 1]])
    applied = system.evolve(max_generations=5)
    assert applied == 31
    assert system.events_count == 31
    assert len(system.final_state) == 32
    assert system.generations_count == 5
    assert system.termination_reason == "MaxGenerations"
    assert system.final_atom_count == 32
    assert "31 events" in repr(system) and "MaxGenerations" in repr(system)


def test_evolve_one_liner_and_continuation():
    system = sr.evolve(GROWTH, [[1, 1]], generations=1)
    assert system.events_count == 1
    system.evolve(max_generations=2)
    assert system.events_count == 3
    assert system.termination_reason == "MaxGenerations"


def test_set_replace_family():
    assert sr.set_replace([[1, 2], [2, 3]], CHAIN) == [[1, 3]]
    states = sr.set_replace_list([[1, 2], [2, 3], [3, 1]], CHAIN, 2)
    assert states == [
        [[1, 2], [2, 3], [3, 1]],
        [[3, 1], [1, 3]],
        [[3, 3]],
    ]
    chain4 = [[1, 2], [2, 3], [3, 4], [4, 5]]
    assert sr.set_replace_all(chain4, CHAIN) == [[1, 3], [3, 5]]
    assert sr.set_replace_fixed_point(chain4, CHAIN) == [[1, 5]]


def test_event_ordering():
    # eventOrderingFunction.wlt vector: OldestEdge removes {1,2},{2,3}.
    rules = "{{b, c}, {a, b}} -> {}"
    init = [[1, 2], [3, 4], [4, 5], [2, 3], [7, 8], [8, 9], [5, 6]]
    system = sr.evolve(rules, init, events=1, event_ordering=["OldestEdge"])
    assert system.final_state == [[3, 4], [4, 5], [7, 8], [8, 9], [5, 6]]
    try:
        sr.HypergraphSystem(rules, init, event_ordering=["Bogus"])
        raise AssertionError("expected ValueError")
    except ValueError as e:
        assert "Bogus" in str(e) and "OldestEdge" in str(e)


def test_history_and_causality():
    system = sr.evolve("{{x, y}, {y, z}} -> {{x, z}}",
                       [[1, 2], [2, 3], [3, 4], [4, 5]])
    assert system.termination_reason == "FixedPoint"
    assert system.causal_graph_edges() == [(1, 3), (2, 3)]
    assert "digraph" in system.causal_graph_dot()

    events = system.events()
    assert events[0].rule is None and events[0].generation == 0
    assert events[1].inputs == [0, 1] and events[1].outputs == [4]
    assert events[3].generation == 2

    tokens = system.tokens()
    alive = [t for t in tokens if t.destroyer_event is None]
    assert [t.atoms for t in alive] == [[1, 5]]
    assert tokens[0].creator_event == 0 and tokens[0].destroyer_event == 1
    assert "Token(" in repr(tokens[0]) and "Event(" in repr(events[1])

    assert system.state_after_event(0) == [[1, 2], [2, 3], [3, 4], [4, 5]]
    assert system.state_at_generation(1) == [[1, 3], [3, 5]]
    assert len(system.states_by_event()) == 4
    assert system.max_complete_generation() == 2
    try:
        system.state_after_event(99)
        raise AssertionError("expected IndexError")
    except IndexError:
        pass


def test_error_mapping():
    try:
        sr.HypergraphSystem(GROWTH, [[0, 1]])
        raise AssertionError("expected ValueError")
    except ValueError:
        pass
    try:
        sr.Rule.parse("not a rule")
        raise AssertionError("expected ValueError")
    except ValueError:
        pass
    system = sr.HypergraphSystem("{{x, y}, {z, w}} -> {{x, w}}", [[1, 2], [3, 4]])
    try:
        system.evolve(max_events=1)
        raise AssertionError("expected SetReplaceError")
    except sr.SetReplaceError as e:
        assert "connected" in str(e)


def test_plots():
    system = sr.evolve(GROWTH, [[1, 1]], generations=3)
    p = system.plot(labels=True)
    assert p.svg.startswith("<svg") and "<text" in p.svg
    assert p._repr_svg_() == p.svg
    bare = system.plot()
    assert "<text" not in bare.svg
    custom = system.plot(labels={1: "origin"})
    assert "origin" in custom.svg

    causal = system.causal_graph_plot()
    assert causal.svg.startswith("<svg")

    with tempfile.TemporaryDirectory() as d:
        png = os.path.join(d, "state.png")
        svg = os.path.join(d, "state.svg")
        p.save(png)
        p.save(svg)
        assert open(png, "rb").read(8).startswith(b"\x89PNG")
        assert open(svg).read().startswith("<svg")
        try:
            p.save(os.path.join(d, "state.gif"))
            raise AssertionError("expected ValueError")
        except ValueError:
            pass

    # Module-level plot on a plain digraph (binary edges).
    digraph = sr.plot([[1, 2], [2, 3], [3, 1]], labels=True)
    assert digraph.svg.count("<polygon") >= 3  # arrowheads


def test_layout():
    state = [[1, 2], [2, 3], [3, 1]]
    pos = sr.layout(state)
    assert set(pos.keys()) == {1, 2, 3}
    # Mean drawn edge length is normalized to 1.
    import math
    lengths = []
    for a, b in [(1, 2), (2, 3), (3, 1)]:
        (xa, ya), (xb, yb) = pos[a], pos[b]
        lengths.append(math.hypot(xa - xb, ya - yb))
    assert abs(sum(lengths) / len(lengths) - 1.0) < 1e-6
    # Deterministic per seed.
    assert sr.layout(state, seed=7) == sr.layout(state, seed=7)


def test_determinism_and_seeds():
    a = sr.evolve(GROWTH, [[1, 1]], generations=4,
                  event_ordering=["Random"], random_seed=5)
    b = sr.evolve(GROWTH, [[1, 1]], generations=4,
                  event_ordering=["Random"], random_seed=5)
    assert a.final_state == b.final_state
    assert [e.inputs for e in a.events()] == [e.inputs for e in b.events()]


def test_enumerate_rules():
    rules = sr.enumerate_rules([(1, 2)], [(1, 2)])
    assert len(rules) == 11
    assert rules[0].inputs == [[-1, -1]] and rules[0].outputs == [[-1, -1]]
    assert len(sr.enumerate_rules([(1, 2)], [(1, 2)], max_elements=2)) == 7
    assert len(sr.enumerate_rules([(2, 2)], [(2, 2)])) == 562
    assert len(sr.enumerate_rules([(1, 2)], [(2, 2)], connectivity="None")) > 73
    # Enumerated rules plug straight into evolution.
    system = sr.evolve(rules[10], [[1, 2]], events=5)
    assert system.events_count == 5
    try:
        sr.enumerate_rules([(1, 2)], [(1, 2)], connectivity="Bogus")
        raise AssertionError("expected ValueError")
    except ValueError as e:
        assert "Bogus" in str(e)


def main():
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for t in tests:
        t()
        print(f"ok  {t.__name__}")
    print(f"\n{len(tests)} tests passed (setreplace {sr.__version__})")


if __name__ == "__main__":
    main()
