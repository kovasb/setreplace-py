//! The classic growth model:
//! WolframModel[{{x, y}} -> {{x, y}, {y, z}}, {{1, 1}}, 5]

use setreplace::*;

fn main() {
    let rule = Rule::parse("{{x, y}} -> {{x, y}, {y, z}}").unwrap();
    let mut system = HypergraphSystem::new(vec![rule], parse_state("{{1, 1}}").unwrap()).unwrap();
    system.evolve(&StepSpec::generations(5)).unwrap();

    println!("termination: {:?}", system.termination_reason());
    println!("events:      {}", system.events_count());
    println!("generations: {}", system.generations_count());
    println!(
        "final state: {} edges, {} atoms",
        system.final_state().len(),
        system.final_atom_count()
    );
    println!(
        "first few edges: {:?}",
        &system.final_state()[..5.min(system.final_state().len())]
    );

    // The causal graph of the first three generations is a binary tree.
    let mut small = HypergraphSystem::new(
        vec![Rule::parse("{{x, y}} -> {{x, y}, {y, z}}").unwrap()],
        parse_state("{{1, 1}}").unwrap(),
    )
    .unwrap();
    small.evolve(&StepSpec::generations(3)).unwrap();
    println!(
        "\ncausal graph (3 generations):\n{}",
        small.causal_graph_dot(false)
    );
}
