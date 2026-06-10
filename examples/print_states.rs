use setreplace::*;
fn wl(state: &[Vec<Atom>]) -> String {
    let edges: Vec<String> = state.iter()
        .map(|e| format!("{{{}}}", e.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(",")))
        .collect();
    format!("{{{}}}", edges.join(","))
}
fn main() {
    let rule = Rule::parse("{{v1, v2, v3}, {v2, v4, v5}} -> {{v5, v6, v1}, {v6, v4, v2}, {v4, v5, v3}}").unwrap();
    let init = parse_state("{{1, 2, 3}, {2, 4, 5}, {4, 6, 7}}").unwrap();
    for n in [1u64, 10, 100] {
        let mut sys = HypergraphSystem::new(vec![rule.clone()], init.clone()).unwrap();
        sys.evolve(&StepSpec::events(n)).unwrap();
        println!("state{}={}", n, wl(&sys.final_state()));
    }
}
