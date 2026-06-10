//! Reproduces the SetReplace README's canonical figures from the Rust
//! engine + renderer:
//!
//! - `HypergraphPlot[{{1,2,3},{2,4,5},{4,6,7}}, VertexLabels -> Automatic]`
//! - The same after 1 / 10 / 100 events of
//!   `{{v1,v2,v3},{v2,v4,v5}} :> {{v5,v6,v1},{v6,v4,v2},{v4,v5,v3}}`
//! - The layered causal graph of the 10-event evolution.

use std::fs;
use std::path::Path;

use setreplace::*;
use setreplace_viz::*;

fn plot_state(
    state: &[Vec<Atom>],
    labels: bool,
    seed: u64,
    out_dir: &Path,
    name: &str,
) {
    let opts = HypergraphPlotOptions {
        seed,
        labels: labels.then(|| readme_style_labels(state, 8)),
        ..Default::default()
    };
    let svg = hypergraph_plot_svg(state, &opts);
    fs::write(out_dir.join(format!("{name}.svg")), &svg).unwrap();
    svg_to_png(&svg, &out_dir.join(format!("{name}.png"))).unwrap();
    println!("wrote {name} ({} edges)", state.len());
}

fn main() {
    let out_dir = Path::new("out");
    fs::create_dir_all(out_dir).unwrap();

    let rule = Rule::parse(
        "{{v1, v2, v3}, {v2, v4, v5}} -> {{v5, v6, v1}, {v6, v4, v2}, {v4, v5, v3}}",
    )
    .unwrap();
    let init = parse_state("{{1, 2, 3}, {2, 4, 5}, {4, 6, 7}}").unwrap();

    // Figure 1: the initial hypergraph.
    plot_state(&init, true, 1, out_dir, "basic_hypergraph_plot");

    // Figures 2-4: states after 1, 10, 100 events.
    for (events, labels, seed, name) in [
        (1u64, true, 1, "evolution_result_1_step"),
        (10, true, 5, "evolution_result_10_steps"),
        (100, false, 1, "evolution_result_100_steps"),
    ] {
        let mut system = HypergraphSystem::new(vec![rule.clone()], init.clone()).unwrap();
        system.evolve(&StepSpec::events(events)).unwrap();
        let state = system.final_state();
        plot_state(&state, labels, seed, out_dir, name);
    }

    // Figure 5: layered causal graph of the 10-event evolution.
    let mut system = HypergraphSystem::new(vec![rule.clone()], init).unwrap();
    system.evolve(&StepSpec::events(10)).unwrap();
    let svg = layered_causal_graph_svg(&system, &CausalGraphOptions::default());
    fs::write(out_dir.join("layered_causal_graph.svg"), &svg).unwrap();
    svg_to_png(&svg, &out_dir.join("layered_causal_graph.png")).unwrap();
    println!(
        "wrote layered_causal_graph ({} events, {} causal edges)",
        system.events_count(),
        system.causal_graph_edges(false).len()
    );
}
