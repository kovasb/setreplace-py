//! Python bindings for `setreplace` + `setreplace-viz`, implementing the
//! contract in docs/python-api.md.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use pyo3::create_exception;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;

use ::setreplace as engine;
use ::setreplace_viz as viz;

create_exception!(
    setreplace,
    SetReplaceError,
    pyo3::exceptions::PyException,
    "Evolution-time failure (e.g. disconnected rule inputs, atom overflow)."
);

/// Invalid input → ValueError; evolution-time failures → SetReplaceError.
fn engine_err(e: engine::Error) -> PyErr {
    match e {
        engine::Error::DisconnectedInputs | engine::Error::AtomCountOverflow => {
            SetReplaceError::new_err(e.to_string())
        }
        _ => PyValueError::new_err(e.to_string()),
    }
}

fn reason_name(reason: engine::TerminationReason) -> &'static str {
    use engine::TerminationReason::*;
    match reason {
        NotTerminated => "NotTerminated",
        MaxEvents => "MaxEvents",
        MaxGenerations => "MaxGenerations",
        MaxVertices => "MaxVertices",
        MaxVertexDegree => "MaxVertexDegree",
        MaxEdges => "MaxEdges",
        FixedPoint => "FixedPoint",
    }
}

const ORDERING_NAMES: &str = "OldestEdge, LeastOldEdge, LeastRecentEdge, NewestEdge, \
     RuleOrdering, ReverseRuleOrdering, RuleIndex, ReverseRuleIndex, Random, Any";

fn parse_ordering(names: &[String]) -> PyResult<Vec<engine::OrderingFunction>> {
    use engine::OrderingFunction::*;
    names
        .iter()
        .map(|n| match n.as_str() {
            "OldestEdge" => Ok(OldestEdge),
            "LeastOldEdge" => Ok(LeastOldEdge),
            "LeastRecentEdge" => Ok(LeastRecentEdge),
            "NewestEdge" => Ok(NewestEdge),
            "RuleOrdering" => Ok(RuleOrdering),
            "ReverseRuleOrdering" => Ok(ReverseRuleOrdering),
            "RuleIndex" => Ok(RuleIndex),
            "ReverseRuleIndex" => Ok(ReverseRuleIndex),
            "Random" => Ok(Random),
            "Any" => Ok(Any),
            other => Err(PyValueError::new_err(format!(
                "unknown event ordering `{other}`; valid names: {ORDERING_NAMES}"
            ))),
        })
        .collect()
}

/// Accepts a Rule, a rule string, or a list of either.
fn extract_rules(obj: &Bound<'_, PyAny>) -> PyResult<Vec<engine::Rule>> {
    fn one(obj: &Bound<'_, PyAny>) -> PyResult<engine::Rule> {
        if let Ok(r) = obj.extract::<PyRef<Rule>>() {
            return Ok(r.inner.clone());
        }
        if let Ok(s) = obj.extract::<String>() {
            return engine::Rule::parse(&s).map_err(engine_err);
        }
        Err(PyValueError::new_err(
            "rules must be a Rule, a rule string like '{{x, y}} -> {{x, y}, {y, z}}', \
             or a list of those",
        ))
    }
    if let Ok(list) = obj.downcast::<PyList>() {
        return list.iter().map(|item| one(&item)).collect();
    }
    Ok(vec![one(obj)?])
}

fn rule_to_text(r: &engine::Rule) -> String {
    let atom = |a: &i64| {
        if *a < 0 {
            format!("x{}", -a)
        } else {
            a.to_string()
        }
    };
    let edge = |e: &Vec<i64>| format!("{{{}}}", e.iter().map(atom).collect::<Vec<_>>().join(", "));
    let side =
        |es: &[Vec<i64>]| format!("{{{}}}", es.iter().map(edge).collect::<Vec<_>>().join(", "));
    format!("{} -> {}", side(&r.inputs), side(&r.outputs))
}

/// In structured rules, ints are concrete atoms and strings are variables.
#[derive(FromPyObject)]
enum AtomOrVar {
    Int(i64),
    Var(String),
}

/// `labels=` accepts True/False or {atom: text}.
#[derive(FromPyObject)]
enum LabelsArg {
    Flag(bool),
    Map(HashMap<i64, String>),
}

fn build_labels(state: &[Vec<i64>], labels: Option<LabelsArg>) -> Option<HashMap<i64, String>> {
    match labels {
        None | Some(LabelsArg::Flag(false)) => None,
        Some(LabelsArg::Flag(true)) => Some(
            viz::vertex_list(state)
                .into_iter()
                .map(|a| (a, a.to_string()))
                .collect(),
        ),
        Some(LabelsArg::Map(m)) => Some(m),
    }
}

fn make_plot(
    py: Python<'_>,
    state: &[Vec<i64>],
    labels: Option<LabelsArg>,
    seed: u64,
    width: f64,
    repulsive_exponent: f64,
) -> Plot {
    let opts = viz::HypergraphPlotOptions {
        seed,
        labels: build_labels(state, labels),
        target_width_pt: width,
        repulsive_exponent,
        ..Default::default()
    };
    let svg = py.allow_threads(|| viz::hypergraph_plot_svg(state, &opts));
    Plot { svg }
}

/// A substitution rule on ordered hypergraphs.
#[pyclass(frozen, module = "setreplace")]
#[derive(Clone)]
pub struct Rule {
    inner: engine::Rule,
}

#[pymethods]
impl Rule {
    /// Structured form: strings are pattern variables, ints are concrete
    /// atoms. `Rule([["x", "y"]], [["x", "y"], ["y", "z"]])`.
    #[new]
    fn new(inputs: Vec<Vec<AtomOrVar>>, outputs: Vec<Vec<AtomOrVar>>) -> PyResult<Self> {
        let mut vars: Vec<String> = Vec::new();
        let mut resolve = |edges: Vec<Vec<AtomOrVar>>| -> Vec<Vec<i64>> {
            edges
                .into_iter()
                .map(|e| {
                    e.into_iter()
                        .map(|a| match a {
                            AtomOrVar::Int(n) => n,
                            AtomOrVar::Var(name) => {
                                let idx =
                                    vars.iter().position(|v| *v == name).unwrap_or_else(|| {
                                        vars.push(name);
                                        vars.len() - 1
                                    });
                                -((idx + 1) as i64)
                            }
                        })
                        .collect()
                })
                .collect()
        };
        let ins = resolve(inputs);
        let outs = resolve(outputs);
        engine::Rule::new(ins, outs)
            .map(|inner| Rule { inner })
            .map_err(engine_err)
    }

    /// Wolfram-ish text, e.g. `'{{x, y}} -> {{x, y}, {y, z}}'`; the
    /// `{{a_, b_}} :> ...` blank syntax is accepted too.
    #[staticmethod]
    fn parse(s: &str) -> PyResult<Self> {
        engine::Rule::parse(s)
            .map(|inner| Rule { inner })
            .map_err(engine_err)
    }

    /// Input pattern edges; variables appear as negative ints.
    #[getter]
    fn inputs(&self) -> Vec<Vec<i64>> {
        self.inner.inputs.clone()
    }

    #[getter]
    fn outputs(&self) -> Vec<Vec<i64>> {
        self.inner.outputs.clone()
    }

    fn __str__(&self) -> String {
        rule_to_text(&self.inner)
    }

    fn __repr__(&self) -> String {
        format!("Rule.parse(\"{}\")", rule_to_text(&self.inner))
    }
}

/// One hyperedge instance in the evolution's history.
#[pyclass(frozen, get_all, module = "setreplace")]
#[derive(Clone)]
pub struct Token {
    atoms: Vec<i64>,
    creator_event: usize,
    destroyer_event: Option<usize>,
    generation: i64,
}

#[pymethods]
impl Token {
    fn __repr__(&self) -> String {
        format!(
            "Token(atoms={:?}, creator_event={}, destroyer_event={}, generation={})",
            self.atoms,
            self.creator_event,
            self.destroyer_event
                .map_or("None".to_string(), |d| d.to_string()),
            self.generation
        )
    }
}

/// An applied substitution event; event 0 is the initial pseudo-event.
#[pyclass(frozen, get_all, module = "setreplace")]
#[derive(Clone)]
pub struct Event {
    rule: Option<usize>,
    inputs: Vec<usize>,
    outputs: Vec<usize>,
    generation: i64,
}

#[pymethods]
impl Event {
    fn __repr__(&self) -> String {
        format!(
            "Event(rule={}, inputs={:?}, outputs={:?}, generation={})",
            self.rule.map_or("None".to_string(), |r| r.to_string()),
            self.inputs,
            self.outputs,
            self.generation
        )
    }
}

/// A rendered figure. Displays inline in Jupyter; `.save()` writes
/// .svg or .png (PNG rasterized in-process, no external tools).
#[pyclass(frozen, module = "setreplace")]
pub struct Plot {
    svg: String,
}

#[pymethods]
impl Plot {
    #[getter]
    fn svg(&self) -> &str {
        &self.svg
    }

    fn save(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let p = Path::new(path);
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        // GIL released so ThreadPoolExecutor batches rasterize in parallel.
        match ext.as_deref() {
            Some("svg") => py
                .allow_threads(|| fs::write(p, &self.svg))
                .map_err(PyErr::from),
            Some("png") => py
                .allow_threads(|| viz::svg_to_png(&self.svg, p))
                .map_err(PyValueError::new_err),
            _ => Err(PyValueError::new_err("path must end in .svg or .png")),
        }
    }

    fn _repr_svg_(&self) -> &str {
        &self.svg
    }

    fn __repr__(&self) -> String {
        format!("<Plot: {} bytes of SVG>", self.svg.len())
    }
}

/// A single-history hypergraph substitution system (Wolfram model).
#[pyclass(module = "setreplace")]
pub struct HypergraphSystem {
    inner: engine::HypergraphSystem,
}

#[pymethods]
impl HypergraphSystem {
    #[new]
    #[pyo3(signature = (rules, initial_state, *, event_ordering=None, random_seed=0))]
    fn new(
        rules: &Bound<'_, PyAny>,
        initial_state: Vec<Vec<i64>>,
        event_ordering: Option<Vec<String>>,
        random_seed: u64,
    ) -> PyResult<Self> {
        let rules = extract_rules(rules)?;
        let ordering = match &event_ordering {
            Some(names) => parse_ordering(names)?,
            None => engine::default_event_ordering(),
        };
        engine::HypergraphSystem::with_options(
            rules,
            initial_state,
            engine::EvolutionOptions {
                event_ordering: ordering,
                random_seed,
            },
        )
        .map(|inner| Self { inner })
        .map_err(engine_err)
    }

    /// Runs events until a limit or fixed point; returns the number of
    /// events applied. No limits = run to fixed point (which may not
    /// terminate). May be called repeatedly; raising `max_generations`
    /// resumes matching of previously capped tokens.
    #[pyo3(signature = (*, max_events=None, max_generations=None, max_vertices=None,
                        max_vertex_degree=None, max_edges=None))]
    fn evolve(
        &mut self,
        py: Python<'_>,
        max_events: Option<u64>,
        max_generations: Option<u64>,
        max_vertices: Option<u64>,
        max_vertex_degree: Option<u64>,
        max_edges: Option<u64>,
    ) -> PyResult<u64> {
        let spec = engine::StepSpec {
            max_events,
            max_generations,
            max_vertices,
            max_vertex_degree,
            max_edges,
        };
        py.allow_threads(|| self.inner.evolve(&spec))
            .map_err(engine_err)
    }

    /// Applies a single event (no limits). Returns whether one was applied.
    fn replace_once(&mut self, py: Python<'_>) -> PyResult<bool> {
        py.allow_threads(|| self.inner.replace_once())
            .map_err(engine_err)
    }

    #[getter]
    fn final_state(&self) -> Vec<Vec<i64>> {
        self.inner.final_state()
    }

    #[getter]
    fn termination_reason(&self) -> &'static str {
        reason_name(self.inner.termination_reason())
    }

    #[getter]
    fn events_count(&self) -> usize {
        self.inner.events_count()
    }

    #[getter]
    fn generations_count(&self) -> i64 {
        self.inner.generations_count()
    }

    #[getter]
    fn final_atom_count(&self) -> usize {
        self.inner.final_atom_count()
    }

    /// The state after the first `k` events (`k = 0` is the initial state).
    fn state_after_event(&self, k: usize) -> PyResult<Vec<Vec<i64>>> {
        if k > self.inner.events_count() {
            return Err(PyIndexError::new_err(format!(
                "event index {k} out of range (0..={})",
                self.inner.events_count()
            )));
        }
        Ok(self.inner.state_after_event(k))
    }

    /// States after 0, 1, ..., events_count events (SetReplaceList).
    fn states_by_event(&self) -> Vec<Vec<Vec<i64>>> {
        self.inner.states_by_event()
    }

    /// The state at generation `g` (the WL evolution object's `[g]`).
    fn state_at_generation(&self, g: i64) -> Vec<Vec<i64>> {
        self.inner.state_at_generation(g)
    }

    fn max_complete_generation(&mut self) -> PyResult<i64> {
        self.inner.max_complete_generation().map_err(engine_err)
    }

    /// Every hyperedge ever created, with full causal metadata.
    fn tokens(&self) -> Vec<Token> {
        self.inner
            .tokens()
            .iter()
            .map(|t| Token {
                atoms: t.atoms.clone(),
                creator_event: t.creator_event,
                destroyer_event: t.destroyer_event,
                generation: t.generation,
            })
            .collect()
    }

    /// Every event; index 0 is the initial pseudo-event.
    fn events(&self) -> Vec<Event> {
        self.inner
            .events()
            .iter()
            .map(|e| Event {
                rule: e.rule,
                inputs: e.inputs.clone(),
                outputs: e.outputs.clone(),
                generation: e.generation,
            })
            .collect()
    }

    #[pyo3(signature = (include_initial=false))]
    fn causal_graph_edges(&self, include_initial: bool) -> Vec<(usize, usize)> {
        self.inner.causal_graph_edges(include_initial)
    }

    #[pyo3(signature = (include_initial=false))]
    fn causal_graph_dot(&self, include_initial: bool) -> String {
        self.inner.causal_graph_dot(include_initial)
    }

    /// HypergraphPlot of the current state.
    #[pyo3(signature = (*, labels=None, seed=0, width=478.0, repulsive_exponent=1.0))]
    fn plot(
        &self,
        py: Python<'_>,
        labels: Option<LabelsArg>,
        seed: u64,
        width: f64,
        repulsive_exponent: f64,
    ) -> Plot {
        let state = self.inner.final_state();
        make_plot(py, &state, labels, seed, width, repulsive_exponent)
    }

    /// Layered causal graph (orange events, dark-red causal edges).
    #[pyo3(signature = (*, include_initial=false, width=478.0))]
    fn causal_graph_plot(&self, py: Python<'_>, include_initial: bool, width: f64) -> Plot {
        let opts = viz::CausalGraphOptions {
            include_initial,
            target_width_pt: width,
        };
        let svg = py.allow_threads(|| viz::layered_causal_graph_svg(&self.inner, &opts));
        Plot { svg }
    }

    fn __repr__(&self) -> String {
        format!(
            "<HypergraphSystem: {} events, {} generations, {} edges, {}>",
            self.inner.events_count(),
            self.inner.generations_count(),
            self.inner.final_state().len(),
            reason_name(self.inner.termination_reason())
        )
    }
}

/// HypergraphPlot of any hypergraph (all-binary edges = ordinary digraph).
#[pyfunction]
#[pyo3(signature = (edges, *, labels=None, seed=0, width=478.0, repulsive_exponent=1.0))]
fn plot(
    py: Python<'_>,
    edges: Vec<Vec<i64>>,
    labels: Option<LabelsArg>,
    seed: u64,
    width: f64,
    repulsive_exponent: f64,
) -> Plot {
    make_plot(py, &edges, labels, seed, width, repulsive_exponent)
}

/// Spring-electrical vertex positions (mean drawn-edge length 1), for
/// custom drawing with matplotlib & friends.
#[pyfunction]
#[pyo3(signature = (edges, *, seed=0, repulsive_exponent=1.0))]
fn layout(
    py: Python<'_>,
    edges: Vec<Vec<i64>>,
    seed: u64,
    repulsive_exponent: f64,
) -> HashMap<i64, (f64, f64)> {
    let options = viz::LayoutOptions {
        seed,
        repulsive_exponent,
    };
    let result = py.allow_threads(|| viz::layout_hypergraph_with(&edges, &options));
    result
        .positions
        .into_iter()
        .map(|(a, p)| (a, (p.x, p.y)))
        .collect()
}

/// One-liner standing in for WolframModel[rules, init, steps]: constructs a
/// system, evolves it, returns it.
#[pyfunction]
#[pyo3(signature = (rules, initial_state, *, generations=None, events=None, max_vertices=None,
                    max_vertex_degree=None, max_edges=None, event_ordering=None, random_seed=0))]
#[allow(clippy::too_many_arguments)]
fn evolve(
    py: Python<'_>,
    rules: &Bound<'_, PyAny>,
    initial_state: Vec<Vec<i64>>,
    generations: Option<u64>,
    events: Option<u64>,
    max_vertices: Option<u64>,
    max_vertex_degree: Option<u64>,
    max_edges: Option<u64>,
    event_ordering: Option<Vec<String>>,
    random_seed: u64,
) -> PyResult<HypergraphSystem> {
    let mut system = HypergraphSystem::new(rules, initial_state, event_ordering, random_seed)?;
    let spec = engine::StepSpec {
        max_events: events,
        max_generations: generations,
        max_vertices,
        max_vertex_degree,
        max_edges,
    };
    py.allow_threads(|| system.inner.evolve(&spec))
        .map_err(engine_err)?;
    Ok(system)
}

/// All inequivalent rules of a signature, replicating the Wolfram Physics
/// Project's EnumerateWolframModelRules (canonical forms, same order).
/// Signature sides are (count, arity) pairs: 2 binary edges -> 3 binary
/// edges is `enumerate_rules([(2, 2)], [(3, 2)])`.
#[pyfunction]
#[pyo3(signature = (inputs, outputs, *, connectivity="Automatic", max_elements=None))]
fn enumerate_rules(
    py: Python<'_>,
    inputs: Vec<Vec<usize>>,
    outputs: Vec<Vec<usize>>,
    connectivity: &str,
    max_elements: Option<usize>,
) -> PyResult<Vec<Rule>> {
    fn groups(side: Vec<Vec<usize>>) -> PyResult<Vec<(usize, usize)>> {
        side.into_iter()
            .map(|g| {
                if g.len() == 2 {
                    Ok((g[0], g[1]))
                } else {
                    Err(PyValueError::new_err(
                        "signature sides must be lists of (count, arity) pairs, \
                         e.g. enumerate_rules([(2, 2)], [(3, 2)])",
                    ))
                }
            })
            .collect()
    }
    let connectivity = match connectivity {
        "Automatic" => engine::Connectivity::LeftConnected,
        "None" => engine::Connectivity::None,
        "All" => engine::Connectivity::All,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown connectivity `{other}`; valid: Automatic, None, All"
            )))
        }
    };
    let signature = engine::RuleSignature {
        inputs: groups(inputs)?,
        outputs: groups(outputs)?,
    };
    let options = engine::EnumerationOptions {
        connectivity,
        max_elements,
    };
    let rules = py
        .allow_threads(|| engine::enumerate_rules(&signature, &options))
        .map_err(engine_err)?;
    Ok(rules.into_iter().map(|inner| Rule { inner }).collect())
}

/// SetReplace[state, rules, events]: applies up to `events` substitution
/// events and returns the resulting state.
#[pyfunction]
#[pyo3(signature = (state, rules, events=1))]
fn set_replace(
    py: Python<'_>,
    state: Vec<Vec<i64>>,
    rules: &Bound<'_, PyAny>,
    events: u64,
) -> PyResult<Vec<Vec<i64>>> {
    let rules = extract_rules(rules)?;
    py.allow_threads(|| engine::set_replace(&state, &rules, events))
        .map_err(engine_err)
}

/// SetReplaceList[state, rules, events]: states after 0, 1, ... events.
#[pyfunction]
fn set_replace_list(
    py: Python<'_>,
    state: Vec<Vec<i64>>,
    rules: &Bound<'_, PyAny>,
    events: u64,
) -> PyResult<Vec<Vec<Vec<i64>>>> {
    let rules = extract_rules(rules)?;
    py.allow_threads(|| engine::set_replace_list(&state, &rules, events))
        .map_err(engine_err)
}

/// SetReplaceAll[state, rules, generations]: evolves the given number of
/// generations and returns the resulting state.
#[pyfunction]
#[pyo3(signature = (state, rules, generations=1))]
fn set_replace_all(
    py: Python<'_>,
    state: Vec<Vec<i64>>,
    rules: &Bound<'_, PyAny>,
    generations: u64,
) -> PyResult<Vec<Vec<i64>>> {
    let rules = extract_rules(rules)?;
    py.allow_threads(|| engine::set_replace_all(&state, &rules, generations))
        .map_err(engine_err)
}

/// SetReplaceFixedPoint[state, rules]: evolves until no rule matches.
/// Beware: does not terminate if the system never reaches a fixed point.
#[pyfunction]
fn set_replace_fixed_point(
    py: Python<'_>,
    state: Vec<Vec<i64>>,
    rules: &Bound<'_, PyAny>,
) -> PyResult<Vec<Vec<i64>>> {
    let rules = extract_rules(rules)?;
    py.allow_threads(|| engine::set_replace_fixed_point(&state, &rules))
        .map_err(engine_err)
}

#[pymodule]
fn setreplace(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Rule>()?;
    m.add_class::<HypergraphSystem>()?;
    m.add_class::<Token>()?;
    m.add_class::<Event>()?;
    m.add_class::<Plot>()?;
    m.add_function(wrap_pyfunction!(plot, m)?)?;
    m.add_function(wrap_pyfunction!(layout, m)?)?;
    m.add_function(wrap_pyfunction!(evolve, m)?)?;
    m.add_function(wrap_pyfunction!(enumerate_rules, m)?)?;
    m.add_function(wrap_pyfunction!(set_replace, m)?)?;
    m.add_function(wrap_pyfunction!(set_replace_list, m)?)?;
    m.add_function(wrap_pyfunction!(set_replace_all, m)?)?;
    m.add_function(wrap_pyfunction!(set_replace_fixed_point, m)?)?;
    m.add("SetReplaceError", m.py().get_type::<SetReplaceError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
