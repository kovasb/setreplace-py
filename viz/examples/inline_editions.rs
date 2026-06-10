//! Compact editions of the showcase systems for inline chat/notebook
//! embedding: fewer events, no arrowheads, small canvas.

use std::fs;
use std::path::Path;

use setreplace::*;
use setreplace_viz::*;

fn main() {
    let out = Path::new("/tmp/inline");
    fs::create_dir_all(out).unwrap();
    type System = (
        &'static str,
        &'static str,
        &'static str,
        (Option<u64>, Option<u64>),
    );
    let systems: &[System] = &[
        (
            "sierpinski",
            "{{x, y, z}} -> {{x, d, f}, {y, e, d}, {z, f, e}}",
            "{{1, 1, 1}}",
            (None, Some(5)),
        ),
        (
            "net",
            "{{a, b, b}, {c, a, d}} -> {{b, e, b}, {b, c, e}, {d, e, e}}",
            "{{1, 1, 1}, {1, 1, 1}}",
            (Some(220), None),
        ),
        (
            "lens",
            "{{a, b, c}, {d, b, e}} -> {{f, c, a}, {c, f, d}, {a, b, f}}",
            "{{1, 1, 1}, {1, 1, 1}}",
            (Some(300), None),
        ),
    ];
    for (name, rule, init, (events, generations)) in systems {
        let mut system =
            HypergraphSystem::new(vec![Rule::parse(rule).unwrap()], parse_state(init).unwrap())
                .unwrap();
        system
            .evolve(&StepSpec {
                max_events: *events,
                max_generations: *generations,
                ..StepSpec::default()
            })
            .unwrap();
        let svg = hypergraph_plot_svg(
            &system.final_state(),
            &HypergraphPlotOptions {
                seed: 1,
                labels: None,
                target_width_pt: 330.0,
                arrowhead_length: Some(0.0),
                ..Default::default()
            },
        );
        fs::write(out.join(format!("{name}.svg")), &svg).unwrap();
        println!(
            "{name}: {} edges, {} bytes",
            system.final_state().len(),
            fs::metadata(out.join(format!("{name}.svg"))).unwrap().len()
        );
    }
}
