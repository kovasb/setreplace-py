//! Verifies rule enumeration against `EnumerateWolframModelRules` ground
//! truth captured via wolframscript (full output in
//! tests/fixtures/enumerate_rules_ground_truth.txt), and against the counts
//! published in the Wolfram Physics Project technical introduction
//! ("The Number of Possible Rules").

use setreplace::{canonical_integer_form, enumerate_rules, EnumerationOptions, RuleSignature};

type IntRule = (Vec<Vec<i64>>, Vec<Vec<i64>>);

fn enumerate(inputs: &[(usize, usize)], outputs: &[(usize, usize)]) -> Vec<IntRule> {
    enumerate_rules(
        &RuleSignature {
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
        },
        &EnumerationOptions::default(),
    )
    .unwrap()
    .iter()
    .map(canonical_integer_form)
    .collect()
}

fn rule(lhs: &[&[i64]], rhs: &[&[i64]]) -> IntRule {
    (
        lhs.iter().map(|e| e.to_vec()).collect(),
        rhs.iter().map(|e| e.to_vec()).collect(),
    )
}

/// EnumerateWolframModelRules[{{1, 2}} -> {{1, 2}}]: the full 11-rule list.
#[test]
fn signature_1_2_to_1_2_exact() {
    let expected = vec![
        rule(&[&[1, 1]], &[&[1, 1]]),
        rule(&[&[1, 1]], &[&[1, 2]]),
        rule(&[&[1, 1]], &[&[2, 1]]),
        rule(&[&[1, 2]], &[&[1, 1]]),
        rule(&[&[1, 2]], &[&[1, 2]]),
        rule(&[&[1, 2]], &[&[2, 1]]),
        rule(&[&[1, 2]], &[&[2, 2]]),
        rule(&[&[1, 2]], &[&[1, 3]]),
        rule(&[&[1, 2]], &[&[2, 3]]),
        rule(&[&[1, 2]], &[&[3, 1]]),
        rule(&[&[1, 2]], &[&[3, 2]]),
    ];
    assert_eq!(enumerate(&[(1, 2)], &[(1, 2)]), expected);
}

/// EnumerateWolframModelRules[{{1, 2}} -> {{2, 2}}]: 73 rules; first five
/// checked exactly.
#[test]
fn signature_1_2_to_2_2() {
    let rules = enumerate(&[(1, 2)], &[(2, 2)]);
    assert_eq!(rules.len(), 73);
    let expected_head = [
        rule(&[&[1, 1]], &[&[1, 1], &[1, 1]]),
        rule(&[&[1, 1]], &[&[1, 1], &[1, 2]]),
        rule(&[&[1, 1]], &[&[1, 1], &[2, 1]]),
        rule(&[&[1, 1]], &[&[1, 2], &[1, 2]]),
        rule(&[&[1, 1]], &[&[1, 2], &[2, 1]]),
    ];
    assert_eq!(&rules[..5], &expected_head[..]);
}

/// EnumerateWolframModelRules[{{1, 3}} -> {{1, 3}}]: 178 rules; the first
/// ten (which pin down the novelty-pattern ordering) checked exactly.
#[test]
fn signature_1_3_to_1_3() {
    let rules = enumerate(&[(1, 3)], &[(1, 3)]);
    assert_eq!(rules.len(), 178);
    let expected_head = vec![
        rule(&[&[1, 1, 1]], &[&[1, 1, 1]]),
        rule(&[&[1, 1, 1]], &[&[1, 1, 2]]),
        rule(&[&[1, 1, 1]], &[&[1, 2, 1]]),
        rule(&[&[1, 1, 1]], &[&[1, 2, 2]]),
        rule(&[&[1, 1, 1]], &[&[1, 2, 3]]),
        rule(&[&[1, 1, 1]], &[&[2, 1, 1]]),
        rule(&[&[1, 1, 1]], &[&[2, 1, 2]]),
        rule(&[&[1, 1, 1]], &[&[2, 2, 1]]),
        rule(&[&[1, 1, 1]], &[&[2, 1, 3]]),
        rule(&[&[1, 1, 1]], &[&[2, 3, 1]]),
    ];
    assert_eq!(&rules[..10], &expected_head[..]);
}

/// EnumerateWolframModelRules[{{2, 2}} -> {{2, 2}}]: the complete 562-rule
/// list, in order, against the captured ground truth (matching the published
/// count of inequivalent left-connected 2_2 -> 2_2 rules).
#[test]
fn signature_2_2_to_2_2_full_list() {
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/wl_2_2_to_2_2.txt"
    ))
    .unwrap();
    let expected: Vec<IntRule> = fixture
        .lines()
        .map(|line| {
            let (lhs, rhs) = line.split_once('|').unwrap();
            let side = |s: &str| -> Vec<Vec<i64>> {
                s.split(';')
                    .map(|e| e.split(' ').map(|v| v.parse().unwrap()).collect())
                    .collect()
            };
            (side(lhs), side(rhs))
        })
        .collect();
    assert_eq!(expected.len(), 562);
    let rules = enumerate(&[(2, 2)], &[(2, 2)]);
    assert_eq!(rules.len(), expected.len());
    for (i, (actual, wanted)) in rules.iter().zip(expected.iter()).enumerate() {
        assert_eq!(actual, wanted, "first divergence at position {}", i + 1);
    }
}

/// The published count for 2_2 -> 4_2 is 40,405 inequivalent left-connected
/// rules ("Rules with Signature 2_2 -> 4_2", wolframphysics.org). Slow;
/// run with `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn signature_2_2_to_4_2_published_count() {
    let rules = enumerate(&[(2, 2)], &[(4, 2)]);
    assert_eq!(rules.len(), 40405);
}

/// Enumerated rules are directly usable by the engine.
#[test]
fn enumerated_rules_evolve() {
    let rules = enumerate_rules(
        &RuleSignature {
            inputs: vec![(1, 2)],
            outputs: vec![(2, 2)],
        },
        &EnumerationOptions::default(),
    )
    .unwrap();
    let mut grew = 0;
    for rule in &rules {
        let mut system =
            setreplace::HypergraphSystem::new(vec![rule.clone()], vec![vec![1, 1]]).unwrap();
        system.evolve(&setreplace::StepSpec::events(10)).unwrap();
        if system.final_state().len() > 1 {
            grew += 1;
        }
    }
    // Most 1_2 -> 2_2 rules grow from a self-loop.
    assert!(grew > 30, "only {grew} rules grew");
}
