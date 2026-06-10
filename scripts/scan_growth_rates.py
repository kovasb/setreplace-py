"""Growth-rate scan over an enumerated rule signature.

Replicates the Wolfram Physics Project's triage methodology: every
inequivalent rule of a signature is evolved from the standard self-loop
initial condition under the standard event ordering; per-generation state
sizes are recorded and classified. Results go to scans/ (gitignored).

Usage: python scripts/scan_growth_rates.py [--max-gens 9] [--max-events 10000]
"""

import argparse
import json
import math
import time
from pathlib import Path

import setreplace as sr


def classify(sizes, reason):
    """Coarse growth classification from per-generation edge counts."""
    if reason == "FixedPoint":
        return "terminated", 0.0
    # The last generation is incomplete if we stopped on the event cap.
    if reason == "MaxEvents" and len(sizes) > 2:
        sizes = sizes[:-1]
    if len(sizes) < 4:
        return "indeterminate", 0.0
    tail = sizes[-5:]
    ratios = [b / a for a, b in zip(tail, tail[1:]) if a > 0]
    diffs = [b - a for a, b in zip(tail, tail[1:])]
    if all(d == 0 for d in diffs):
        return "static", 0.0
    mean_r = sum(ratios) / len(ratios)
    var_r = sum((r - mean_r) ** 2 for r in ratios) / len(ratios)
    mean_d = sum(diffs) / len(diffs)
    var_d = sum((d - mean_d) ** 2 for d in diffs) / len(diffs)
    rel_r = math.sqrt(var_r) / mean_r if mean_r else 1.0
    rel_d = math.sqrt(var_d) / abs(mean_d) if mean_d else 1.0
    if mean_r > 1.25 and rel_r < 0.08:
        return "exponential", round(mean_r, 3)
    if rel_d < 0.25:
        return "linear", round(mean_d, 3)
    # Power-law fit s ~ t^p over the recorded generations.
    points = [(g, s) for g, s in enumerate(sizes) if g >= 1 and s > 0]
    if len(points) >= 3:
        xs = [math.log(g) for g, _ in points]
        ys = [math.log(s) for _, s in points]
        n = len(xs)
        mx, my = sum(xs) / n, sum(ys) / n
        denom = sum((x - mx) ** 2 for x in xs)
        if denom > 0:
            p = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denom
            return "polynomial", round(p, 2)
    return "irregular", 0.0


def connected(state):
    """Whether the hypergraph's edges form one component."""
    if len(state) <= 1:
        return True
    parent = {}

    def find(x):
        while parent.setdefault(x, x) != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for i, edge in enumerate(state):
        for v in edge:
            parent[find(("v", v))] = find(("e", i))
    roots = {find(("e", i)) for i in range(len(state))}
    return len(roots) == 1


def hub_fraction(state):
    """Largest share of edges any single vertex appears in."""
    if not state:
        return 0.0
    counts = {}
    for edge in state:
        for v in set(edge):
            counts[v] = counts.get(v, 0) + 1
    return round(max(counts.values()) / len(state), 3)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-gens", type=int, default=9)
    ap.add_argument("--max-events", type=int, default=10_000)
    args = ap.parse_args()

    signature_inputs, signature_outputs = [(2, 2)], [(3, 2)]
    init = [[1, 1], [1, 1]]  # standard self-loop initial condition

    t0 = time.perf_counter()
    rules = sr.enumerate_rules(signature_inputs, signature_outputs)
    t_enum = time.perf_counter() - t0
    print(f"enumerated {len(rules)} rules in {t_enum:.2f}s")

    out_path = Path("scans/2_2-3_2.jsonl")
    out_path.parent.mkdir(exist_ok=True)
    counts = {}
    total_events = 0
    t1 = time.perf_counter()
    with out_path.open("w") as out:
        for i, rule in enumerate(rules):
            system = sr.evolve(
                rule, init, generations=args.max_gens, events=args.max_events
            )
            gens = system.generations_count
            sizes = [len(system.state_at_generation(g)) for g in range(gens + 1)]
            reason = system.termination_reason
            cls, rate = classify(sizes, reason)
            final = system.final_state
            record = {
                "i": i,
                "rule": str(rule),
                "reason": reason,
                "events": system.events_count,
                "gens": gens,
                "sizes": sizes,
                "atoms": system.final_atom_count,
                "connected": connected(final),
                "hub": hub_fraction(final),
                "class": cls,
                "rate": rate,
            }
            out.write(json.dumps(record) + "\n")
            counts[cls] = counts.get(cls, 0) + 1
            total_events += system.events_count
    t_scan = time.perf_counter() - t1

    print(f"scanned {len(rules)} rules in {t_scan:.2f}s "
          f"({total_events} events, {total_events / t_scan:,.0f} events/s)")
    for cls in sorted(counts, key=counts.get, reverse=True):
        print(f"  {cls:14} {counts[cls]:5}  ({100 * counts[cls] / len(rules):.1f}%)")
    print(f"results: {out_path}")


if __name__ == "__main__":
    main()
