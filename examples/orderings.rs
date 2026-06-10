//! Demonstrates how the event-ordering function changes which event fires,
//! reproducing the scenario from SetReplace's eventOrderingFunction tests:
//! one event of {{b, c}, {a, b}} -> {} applied to seven edges.

use setreplace::*;

fn main() {
    let orderings: Vec<(&str, Vec<OrderingFunction>)> = vec![
        ("OldestEdge", vec![OrderingFunction::OldestEdge]),
        ("LeastOldEdge", vec![OrderingFunction::LeastOldEdge]),
        ("LeastRecentEdge", vec![OrderingFunction::LeastRecentEdge]),
        ("NewestEdge", vec![OrderingFunction::NewestEdge]),
        ("RuleOrdering", vec![OrderingFunction::RuleOrdering]),
        ("default", default_event_ordering()),
    ];

    for (name, ordering) in orderings {
        let mut system = HypergraphSystem::with_options(
            vec![Rule::parse("{{b, c}, {a, b}} -> {}").unwrap()],
            parse_state("{{1, 2}, {3, 4}, {4, 5}, {2, 3}, {7, 8}, {8, 9}, {5, 6}}").unwrap(),
            EvolutionOptions {
                event_ordering: ordering,
                random_seed: 0,
            },
        )
        .unwrap();
        system.evolve(&StepSpec::events(1)).unwrap();
        println!("{name:16} -> {:?}", system.final_state());
    }
}
