//! Rough performance smoke test across models with different connectivity.

use setreplace::*;
use std::collections::HashMap;
use std::time::Instant;

fn max_degree(state: &[Vec<Atom>]) -> usize {
    let mut deg: HashMap<Atom, usize> = HashMap::new();
    for edge in state {
        let mut atoms = edge.clone();
        atoms.dedup();
        atoms.sort_unstable();
        atoms.dedup();
        for a in atoms {
            *deg.entry(a).or_insert(0) += 1;
        }
    }
    deg.values().copied().max().unwrap_or(0)
}

fn run(name: &str, rule_str: &str, init_str: &str, events: u64) {
    let rule = Rule::parse(rule_str).unwrap();
    let init = parse_state(init_str).unwrap();
    let mut system = HypergraphSystem::new(vec![rule], init).unwrap();
    let start = Instant::now();
    system.evolve(&StepSpec::events(events)).unwrap();
    let elapsed = start.elapsed();
    let state = system.final_state();
    println!(
        "{name:<12} {events:>7} events: {elapsed:>8.1?}  ({:>7.0} events/s, {} state edges, max degree {})",
        events as f64 / elapsed.as_secs_f64(),
        state.len(),
        max_degree(&state)
    );
}

fn main() {
    // Sparse: single-edge input, bounded degree.
    run("growth", "{{x, y}} -> {{x, y}, {y, z}}", "{{1, 1}}", 100_000);
    run("subdivision", "{{x, y}} -> {{x, z}, {z, y}}", "{{1, 2}}", 100_000);
    // Two-edge input, sparse outputs.
    run(
        "chain",
        "{{x, y}, {y, z}} -> {{x, z}, {z, w}, {w, y}}",
        "{{1, 2}, {2, 3}, {3, 1}}",
        20_000,
    );
    // Dense: hub vertices accumulate unbounded degree (hard for any engine).
    run(
        "dense-hub",
        "{{x, y}, {y, z}} -> {{x, z}, {z, w}, {x, w}}",
        "{{1, 2}, {2, 3}, {3, 1}}",
        20_000,
    );
}
