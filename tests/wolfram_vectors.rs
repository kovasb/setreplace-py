//! Test vectors lifted from SetReplace's own Wolfram Language test suite
//! (Tests/*.wlt in the SetReplace repository), translated to this crate's
//! conventions: token/event IDs are 0-based (event 0 = initial pseudo-event),
//! and symbolic atoms are replaced by the integers the WL layer would assign
//! them. File/line references point at the original .wlt sources.

use setreplace::*;

fn rule(s: &str) -> Rule {
    Rule::parse(s).unwrap()
}

fn state(s: &str) -> Vec<Vec<Atom>> {
    parse_state(s).unwrap()
}

// ---------------------------------------------------------------- SetReplace

/// SetReplace.wlt:197 — SetReplace[{{1, 2}, {2, 3}},
/// {{a_, b_}, {b_, c_}} :> {{a, c}}, ∞] == {{1, 3}}
#[test]
fn chain_contraction() {
    let rules = [rule("{{a_, b_}, {b_, c_}} :> {{a, c}}")];
    let result = set_replace(&state("{{1, 2}, {2, 3}}"), &rules, 1).unwrap();
    assert_eq!(result, vec![vec![1, 3]]);
    // Same result at the fixed point.
    let result = set_replace_fixed_point(&state("{{1, 2}, {2, 3}}"), &rules).unwrap();
    assert_eq!(result, vec![vec![1, 3]]);
}

/// SetReplace.wlt:161 — SetReplace[{{1}, {2}}, {{1}, {2}} :> {{3}}] == {{3}}
/// (integer literals in pattern rules are concrete atoms).
#[test]
fn concrete_atom_rule() {
    let rules = [rule("{{1}, {2}} -> {{3}}")];
    let result = set_replace(&state("{{1}, {2}}"), &rules, 1).unwrap();
    assert_eq!(result, vec![vec![3]]);
}

/// SetReplaceList.wlt:85 — SetReplaceList[{{1, 2}, {2, 3}, {3, 1}},
/// {{a_, b_}, {b_, c_}} :> {{a, c}}, 2]
/// == {{{1, 2}, {2, 3}, {3, 1}}, {{3, 1}, {1, 3}}, {{3, 3}}}
#[test]
fn set_replace_list_cycle() {
    let rules = [rule("{{a_, b_}, {b_, c_}} :> {{a, c}}")];
    let states = set_replace_list(&state("{{1, 2}, {2, 3}, {3, 1}}"), &rules, 2).unwrap();
    assert_eq!(
        states,
        vec![
            vec![vec![1, 2], vec![2, 3], vec![3, 1]],
            vec![vec![3, 1], vec![1, 3]],
            vec![vec![3, 3]],
        ]
    );
}

// ------------------------------------------------------ event ordering tests

/// eventOrderingFunction.wlt:69-81 — one event of
/// WolframModel[{{b, c}, {a, b}} -> {}] on
/// {{1, 2}, {3, 4}, {4, 5}, {2, 3}, {a, b}, {b, c}, {5, 6}}
/// under each ordering. The symbolic atoms a, b, c become 7, 8, 9 under the
/// WL layer's canonical (Union) numbering.
fn ordering_final_state(ordering: Vec<OrderingFunction>) -> Vec<Vec<Atom>> {
    let rules = vec![rule("{{b, c}, {a, b}} -> {}")];
    let init = state("{{1, 2}, {3, 4}, {4, 5}, {2, 3}, {7, 8}, {8, 9}, {5, 6}}");
    let options = EvolutionOptions {
        event_ordering: ordering,
        random_seed: 0,
    };
    let mut system = HypergraphSystem::with_options(rules, init, options).unwrap();
    system.evolve(&StepSpec::events(1)).unwrap();
    assert_eq!(system.events_count(), 1);
    system.final_state()
}

#[test]
fn ordering_oldest_edge() {
    assert_eq!(
        ordering_final_state(vec![OrderingFunction::OldestEdge]),
        state("{{3, 4}, {4, 5}, {7, 8}, {8, 9}, {5, 6}}")
    );
}

#[test]
fn ordering_least_old_edge() {
    assert_eq!(
        ordering_final_state(vec![OrderingFunction::LeastOldEdge]),
        state("{{1, 2}, {3, 4}, {4, 5}, {2, 3}, {5, 6}}")
    );
}

#[test]
fn ordering_newest_edge() {
    assert_eq!(
        ordering_final_state(vec![OrderingFunction::NewestEdge]),
        state("{{1, 2}, {3, 4}, {2, 3}, {7, 8}, {8, 9}}")
    );
}

#[test]
fn ordering_rule_ordering() {
    assert_eq!(
        ordering_final_state(vec![OrderingFunction::RuleOrdering]),
        state("{{1, 2}, {4, 5}, {7, 8}, {8, 9}, {5, 6}}")
    );
}

/// Derived by hand from the LeastRecentEdge definition (the combination
/// ReverseSortedIds/forward is also exercised by the default-ordering vectors
/// below): the match whose newest edge is oldest is {{4,5},{3,4}}.
#[test]
fn ordering_least_recent_edge() {
    assert_eq!(
        ordering_final_state(vec![OrderingFunction::LeastRecentEdge]),
        state("{{1, 2}, {2, 3}, {7, 8}, {8, 9}, {5, 6}}")
    );
}

/// RuleIndex / ReverseRuleIndex select between two rules that both match.
#[test]
fn ordering_rule_index() {
    let rules = vec![rule("{{x, y}} -> {}"), rule("{{x, y}} -> {{x}}")];
    let init = state("{{1, 2}}");
    for (ordering, expected) in [
        (OrderingFunction::RuleIndex, state("{}")),
        (OrderingFunction::ReverseRuleIndex, state("{{1}}")),
    ] {
        let options = EvolutionOptions {
            event_ordering: vec![ordering],
            random_seed: 0,
        };
        let mut system =
            HypergraphSystem::with_options(rules.clone(), init.clone(), options).unwrap();
        system.evolve(&StepSpec::events(1)).unwrap();
        assert_eq!(system.final_state(), expected);
    }
}

// -------------------------------------------------------------- WolframModel

/// WolframModel.wlt:308-311 — WolframModel[{{0, 1}} -> {{0, 2}, {2, 1}},
/// {{0, 1}}, <|"MaxGenerations" -> 3|>] has 3 generations, 7 events, and
/// terminates with "MaxGenerations". (Initial condition renamed to {{1, 2}};
/// in non-pattern WolframModel rules every atom is a variable.)
#[test]
fn max_generations_termination() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, z}, {z, y}}")],
        state("{{1, 2}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::generations(3)).unwrap();
    assert_eq!(system.generations_count(), 3);
    assert_eq!(system.events_count(), 7);
    assert_eq!(
        system.termination_reason(),
        TerminationReason::MaxGenerations
    );
}

/// WolframModel.wlt:314-317 — same system with <|"MaxEvents" -> 6|>:
/// 3 generations, 6 events, "MaxEvents".
#[test]
fn max_events_termination() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, z}, {z, y}}")],
        state("{{1, 2}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::events(6)).unwrap();
    assert_eq!(system.generations_count(), 3);
    assert_eq!(system.events_count(), 6);
    assert_eq!(system.termination_reason(), TerminationReason::MaxEvents);
}

/// WolframModel.wlt:333-336 — WolframModel[{{0, 1}, {1, 2}} -> {{0, 2}},
/// {{0, 1}, {1, 2}, {2, 3}, {3, 4}}] reaches a fixed point with
/// 2 generations and 3 events. WolframModel.wlt:1193/1182-1184 — the final
/// state is {{1, 5}} and generation 1 is {{1, 3}, {3, 5}} (renamed 0..4 ->
/// 1..5).
#[test]
fn path_contraction_full() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}, {y, z}} -> {{x, z}}")],
        state("{{1, 2}, {2, 3}, {3, 4}, {4, 5}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::default()).unwrap();
    assert_eq!(system.generations_count(), 2);
    assert_eq!(system.events_count(), 3);
    assert_eq!(system.termination_reason(), TerminationReason::FixedPoint);
    assert_eq!(system.final_state(), state("{{1, 5}}"));
    assert_eq!(system.state_at_generation(1), state("{{1, 3}, {3, 5}}"));
    assert_eq!(
        system.state_at_generation(0),
        state("{{1, 2}, {2, 3}, {3, 4}, {4, 5}}")
    );

    // Full event trace under the default ordering, derived by hand.
    let events = system.events();
    assert_eq!(events[1].inputs, vec![0, 1]);
    assert_eq!(events[1].outputs, vec![4]);
    assert_eq!(events[1].generation, 1);
    assert_eq!(events[2].inputs, vec![2, 3]);
    assert_eq!(events[2].outputs, vec![5]);
    assert_eq!(events[2].generation, 1);
    assert_eq!(events[3].inputs, vec![4, 5]);
    assert_eq!(events[3].outputs, vec![6]);
    assert_eq!(events[3].generation, 2);

    assert_eq!(system.causal_graph_edges(false), vec![(1, 3), (2, 3)]);
    assert_eq!(
        system.causal_graph_edges(true),
        vec![(0, 1), (0, 1), (0, 2), (0, 2), (1, 3), (2, 3)]
    );
}

/// WolframModel.wlt:1218 — WolframModel[{{1}} -> {}, {{1}, ..., {5}}, ∞]
/// destroys everything.
#[test]
fn annihilation_to_empty() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x}} -> {}")],
        state("{{1}, {2}, {3}, {4}, {5}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::default()).unwrap();
    assert_eq!(system.final_state(), Vec::<Vec<Atom>>::new());
    assert_eq!(system.events_count(), 5);
    assert_eq!(system.termination_reason(), TerminationReason::FixedPoint);
}

/// WolframModel.wlt:1198 — 10 generations of {{1, 2}} -> {{1, 3}, {3, 2}}
/// give "TotalGenerationsCount" 10.
#[test]
fn ten_generations() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, z}, {z, y}}")],
        state("{{1, 2}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::generations(10)).unwrap();
    assert_eq!(system.generations_count(), 10);
}

/// The classic growth rule {{x, y}} -> {{x, y}, {y, z}} from {{1, 1}}:
/// 5 generations form a complete binary tree of events. Fresh atoms are
/// consecutive integers starting after the largest named atom.
#[test]
fn growth_rule_census() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, y}, {y, z}}")],
        state("{{1, 1}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::generations(5)).unwrap();
    assert_eq!(system.events_count(), 31);
    assert_eq!(system.final_state().len(), 32);
    assert_eq!(
        system.termination_reason(),
        TerminationReason::MaxGenerations
    );
    assert_eq!(system.tokens().len(), 63);
    // 31 fresh atoms named 2..=32, one per event.
    assert_eq!(system.final_atom_count(), 32);
    let max_atom = system
        .tokens()
        .iter()
        .flat_map(|t| t.atoms.iter().copied())
        .max()
        .unwrap();
    assert_eq!(max_atom, 32);
    // Each non-root event consumes a token made by an earlier real event.
    assert_eq!(system.causal_graph_edges(false).len(), 30);
    assert_eq!(system.state_after_event(0), state("{{1, 1}}"));
}

/// Verified live against SetReplace 0.3.196:
/// WolframModel[{{1, 2}} -> {{1, 2}, {2, 3}}, {{1, 1}}, 2] gives
/// "FinalState" {{1, 1}, {1, 3}, {1, 2}, {2, 4}} and "AllEventsList"
/// {{1, {1} -> {2, 3}}, {1, {2} -> {4, 5}}, {1, {3} -> {6, 7}}} (1-based).
/// Fresh vertices continue from the largest named integer in creation order.
#[test]
fn fresh_vertex_naming_matches_wolfram() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, y}, {y, z}}")],
        state("{{1, 1}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::generations(2)).unwrap();
    assert_eq!(
        system.final_state(),
        state("{{1, 1}, {1, 3}, {1, 2}, {2, 4}}")
    );
    let events = system.events();
    assert_eq!(
        (events[1].inputs.clone(), events[1].outputs.clone()),
        (vec![0], vec![1, 2])
    );
    assert_eq!(
        (events[2].inputs.clone(), events[2].outputs.clone()),
        (vec![1], vec![3, 4])
    );
    assert_eq!(
        (events[3].inputs.clone(), events[3].outputs.clone()),
        (vec![2], vec![5, 6])
    );
}

// ------------------------------------------------------- matching semantics

/// A repeated variable within one edge requires equal atoms.
#[test]
fn repeated_variable_in_edge() {
    let rules = [rule("{{x, x}} -> {{x}}")];
    let result = set_replace_fixed_point(&state("{{1, 2}, {3, 3}}"), &rules).unwrap();
    assert_eq!(result, state("{{1, 2}, {3}}"));

    let mut system = HypergraphSystem::new(rules.to_vec(), state("{{1, 2}}")).unwrap();
    system.evolve(&StepSpec::default()).unwrap();
    assert_eq!(system.events_count(), 0);
    assert_eq!(system.termination_reason(), TerminationReason::FixedPoint);
}

/// Two input edges can never match the same token, even if duplicates of the
/// pattern; two identical tokens are required.
#[test]
fn distinct_tokens_required() {
    let rules = [rule("{{x, y}, {x, y}} -> {{x}}")];
    let mut system = HypergraphSystem::new(rules.to_vec(), state("{{1, 2}}")).unwrap();
    system.evolve(&StepSpec::default()).unwrap();
    assert_eq!(system.events_count(), 0);

    let result = set_replace_fixed_point(&state("{{1, 2}, {1, 2}}"), &rules).unwrap();
    assert_eq!(result, state("{{1}}"));
}

/// Distinct variables may bind the same atom.
#[test]
fn non_injective_binding() {
    let rules = [rule("{{x, y}} -> {{y, x}}")];
    let result = set_replace(&state("{{1, 1}}"), &rules, 1).unwrap();
    assert_eq!(result, state("{{1, 1}}"));
}

/// Rules whose inputs do not form a connected hypergraph are unsupported,
/// matching libSetReplace's DisconnectedInputs error.
#[test]
fn disconnected_inputs_error() {
    let rules = vec![rule("{{x, y}, {z, w}} -> {{x, w}}")];
    let mut system = HypergraphSystem::new(rules, state("{{1, 2}, {3, 4}}")).unwrap();
    assert_eq!(
        system.evolve(&StepSpec::events(1)),
        Err(Error::DisconnectedInputs)
    );
}

/// Initial states must use positive atoms (libSetReplace NonPositiveAtoms).
#[test]
fn non_positive_atoms_rejected() {
    let rules = vec![rule("{{x, y}} -> {}")];
    assert_eq!(
        HypergraphSystem::new(rules, vec![vec![0, 1]]).err(),
        Some(Error::NonPositiveAtoms)
    );
}

// ------------------------------------------------------------- SetReplaceAll

/// SetReplaceAll == one generation: on the 4-edge path, generation 1 is
/// {{1, 3}, {3, 5}}.
#[test]
fn set_replace_all_one_generation() {
    let rules = [rule("{{x, y}, {y, z}} -> {{x, z}}")];
    let result = set_replace_all(&state("{{1, 2}, {2, 3}, {3, 4}, {4, 5}}"), &rules, 1).unwrap();
    assert_eq!(result, state("{{1, 3}, {3, 5}}"));
}

// --------------------------------------------------------- final-state caps

/// MaxEdges: the growth rule adds one edge per event; the event that would
/// make 11 edges is not applied.
#[test]
fn max_edges_cap() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, y}, {y, z}}")],
        state("{{1, 1}}"),
    )
    .unwrap();
    system
        .evolve(&StepSpec {
            max_edges: Some(10),
            ..StepSpec::default()
        })
        .unwrap();
    assert_eq!(system.termination_reason(), TerminationReason::MaxEdges);
    assert_eq!(system.events_count(), 9);
    assert_eq!(system.final_state().len(), 10);
}

/// MaxVertices: the growth rule adds one fresh atom per event.
#[test]
fn max_vertices_cap() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, y}, {y, z}}")],
        state("{{1, 1}}"),
    )
    .unwrap();
    system
        .evolve(&StepSpec {
            max_vertices: Some(5),
            ..StepSpec::default()
        })
        .unwrap();
    assert_eq!(system.termination_reason(), TerminationReason::MaxVertices);
    assert_eq!(system.events_count(), 4);
    assert_eq!(system.final_atom_count(), 5);
}

/// MaxVertexDegree: {{x, y}} -> {{x, y}, {x, z}} raises the degree of x by
/// one per event.
#[test]
fn max_vertex_degree_cap() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}} -> {{x, y}, {x, z}}")],
        state("{{1, 2}}"),
    )
    .unwrap();
    system
        .evolve(&StepSpec {
            max_vertex_degree: Some(4),
            ..StepSpec::default()
        })
        .unwrap();
    assert_eq!(
        system.termination_reason(),
        TerminationReason::MaxVertexDegree
    );
    assert_eq!(system.events_count(), 3);
}

// ----------------------------------------------------- evolution continuance

/// Raising MaxGenerations across evolve calls resumes matching of tokens that
/// sat at the previous cap.
#[test]
fn resume_with_higher_generation_cap() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}, {y, z}} -> {{x, z}}")],
        state("{{1, 2}, {2, 3}, {3, 4}, {4, 5}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::generations(1)).unwrap();
    assert_eq!(system.events_count(), 2);
    assert_eq!(
        system.termination_reason(),
        TerminationReason::MaxGenerations
    );
    assert_eq!(system.final_state(), state("{{1, 3}, {3, 5}}"));

    system.evolve(&StepSpec::generations(2)).unwrap();
    assert_eq!(system.events_count(), 3);
    assert_eq!(system.final_state(), state("{{1, 5}}"));
}

/// MaxCompleteGeneration: after the two generation-1 events of the path
/// system, generation 1 is complete; after the fixed point it is 2.
#[test]
fn max_complete_generation_progression() {
    let mut system = HypergraphSystem::new(
        vec![rule("{{x, y}, {y, z}} -> {{x, z}}")],
        state("{{1, 2}, {2, 3}, {3, 4}, {4, 5}}"),
    )
    .unwrap();
    system.evolve(&StepSpec::events(2)).unwrap();
    assert_eq!(system.max_complete_generation().unwrap(), 1);
    system.evolve(&StepSpec::default()).unwrap();
    assert_eq!(system.max_complete_generation().unwrap(), 2);
}

// ----------------------------------------------------------------- ordering

/// Pure random ordering is deterministic for a fixed seed.
#[test]
fn random_ordering_seeded_determinism() {
    let run = |seed: u64| {
        let options = EvolutionOptions {
            event_ordering: vec![OrderingFunction::Random],
            random_seed: seed,
        };
        let mut system = HypergraphSystem::with_options(
            vec![rule("{{x, y}} -> {{x, y}, {y, z}}")],
            state("{{1, 1}}"),
            options,
        )
        .unwrap();
        system.evolve(&StepSpec::generations(4)).unwrap();
        (system.events().to_vec(), system.final_state())
    };
    let (events_a, state_a) = run(5);
    let (events_b, state_b) = run(5);
    assert_eq!(events_a, events_b);
    assert_eq!(state_a, state_b);
    // Counts are order-independent for this rule.
    assert_eq!(events_a.len() - 1, 15);
    assert_eq!(state_a.len(), 16);
}

/// `Any` as the final ordering function picks deterministically without
/// consulting the RNG: identical across seeds.
#[test]
fn any_ordering_is_seed_independent() {
    let run = |seed: u64| {
        let options = EvolutionOptions {
            event_ordering: vec![OrderingFunction::Any],
            random_seed: seed,
        };
        let mut system = HypergraphSystem::with_options(
            vec![rule("{{x, y}} -> {{x, y}, {y, z}}")],
            state("{{1, 1}}"),
            options,
        )
        .unwrap();
        system.evolve(&StepSpec::generations(3)).unwrap();
        (system.events().to_vec(), system.final_state())
    };
    assert_eq!(run(1), run(99));
}
