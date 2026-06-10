//! Renders long evolutions of notable Wolfram model rules from the Wolfram
//! Physics Project announcement post ("Finally We May Have a Path to the
//! Fundamental Theory of Physics... and It's Beautiful", April 2020).
//!
//! The post writes rules in non-pattern notation where every integer is a
//! variable; here they are transcribed with letters (this crate's parser
//! treats integers as concrete atoms). Initial conditions {{0,0},{0,0}} etc.
//! become {{1,1},{1,1}} (atoms must be positive); the structure is identical.

use std::fs;
use std::path::Path;
use std::time::Instant;

use setreplace::*;
use setreplace_viz::*;

struct Showcase {
    name: &'static str,
    description: &'static str,
    rule: &'static str,
    init: &'static str,
    /// (max_events, max_generations)
    steps: (Option<u64>, Option<u64>),
    seed: u64,
    width: f64,
}

const SYSTEMS: &[Showcase] = &[
    Showcase {
        name: "announcement_web",
        description: "the post's recurring binary rule {{x,y},{x,z}} -> {{x,z},{x,w},{y,w},{z,w}}",
        rule: "{{x, y}, {x, z}} -> {{x, z}, {x, w}, {y, w}, {z, w}}",
        init: "{{1, 2}, {2, 3}, {3, 4}, {2, 4}}",
        steps: (Some(1500), None),
        seed: 1,
        width: 620.0,
    },
    Showcase {
        name: "sierpinski_fractal",
        description: "{{1,2,3}} -> {{1,4,6},{2,5,4},{3,6,5}}: self-similar triangles",
        rule: "{{x, y, z}} -> {{x, d, f}, {y, e, d}, {z, f, e}}",
        init: "{{1, 1, 1}}",
        steps: (None, Some(7)),
        seed: 1,
        width: 620.0,
    },
    Showcase {
        name: "triangular_net",
        description: "{{1,2,2},{3,1,4}} -> {{2,5,2},{2,3,5},{4,5,5}}: a regular triangulated net emerges",
        rule: "{{a, b, b}, {c, a, d}} -> {{b, e, b}, {b, c, e}, {d, e, e}}",
        init: "{{1, 1, 1}, {1, 1, 1}}",
        steps: (Some(1000), None),
        seed: 1,
        width: 620.0,
    },
    Showcase {
        name: "lens_mesh",
        description: "{{1,2,3},{4,2,5}} -> {{6,3,1},{3,6,4},{1,2,6}}: a curved lens-shaped mesh",
        rule: "{{a, b, c}, {d, b, e}} -> {{f, c, a}, {c, f, d}, {a, b, f}}",
        init: "{{1, 1, 1}, {1, 1, 1}}",
        steps: (Some(2000), None),
        seed: 1,
        width: 620.0,
    },
    Showcase {
        name: "crumpled_ball",
        description: "{{1,1,2},{3,4,1}} -> {{4,4,3},{5,4,5},{5,2,1}}: a densely crumpled ball of space",
        rule: "{{a, a, b}, {c, d, a}} -> {{d, d, c}, {e, d, e}, {e, b, a}}",
        init: "{{1, 1, 1}, {1, 1, 1}}",
        steps: (Some(2000), None),
        seed: 1,
        width: 620.0,
    },
];

fn main() {
    let out_dir = Path::new("out/showcase");
    fs::create_dir_all(out_dir).unwrap();

    for s in SYSTEMS {
        let total = Instant::now();
        let rule = Rule::parse(s.rule).unwrap();
        let init = parse_state(s.init).unwrap();
        let mut system = HypergraphSystem::new(vec![rule], init).unwrap();
        let spec = StepSpec {
            max_events: s.steps.0,
            max_generations: s.steps.1,
            ..StepSpec::default()
        };
        let evolve_t = Instant::now();
        system.evolve(&spec).unwrap();
        let evolve_time = evolve_t.elapsed();

        let state = system.final_state();
        let layout_t = Instant::now();
        let svg = hypergraph_plot_svg(
            &state,
            &HypergraphPlotOptions {
                seed: s.seed,
                labels: None,
                target_width_pt: s.width,
                ..Default::default()
            },
        );
        let layout_time = layout_t.elapsed();
        svg_to_png(&svg, &out_dir.join(format!("{}.png", s.name))).unwrap();
        fs::write(out_dir.join(format!("{}.svg", s.name)), &svg).unwrap();

        println!(
            "{:<22} {:>5} events, {:>5} edges, {:>5} atoms | evolve {:>6.1?}, layout+render {:>6.1?}, total {:>6.1?}",
            s.name,
            system.events_count(),
            state.len(),
            system.final_atom_count(),
            evolve_time,
            layout_time,
            total.elapsed()
        );
        println!("  ({})", s.description);
    }
}
