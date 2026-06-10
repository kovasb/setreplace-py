//! Spring-electrical hypergraph embedding implementing Yifan Hu,
//! "Efficient, High-Quality Force-Directed Graph Drawing" (the algorithm
//! behind Mathematica's SpringElectricalEmbedding), specialized to
//! SetReplace's HypergraphPlot conventions:
//!
//! - The *layout* graph treats every hyperedge as a **cyclic** path: a
//!   hyperedge {a, b, c} contributes springs a–b, b–c, c–a (a binary edge
//!   {a, b} contributes the spring twice). Only the *ordered* consecutive
//!   pairs are drawn later.
//! - Disconnected components are laid out independently and packed.
//! - The final embedding is rescaled so the mean drawn-segment length is 1
//!   (`rescaleEmbedding`/`edgeScale` in the original), which puts vertex
//!   radii and arrowhead lengths in natural units.
//!
//! The reference algorithm, per the paper:
//! - forces `f_a = d²/K` along springs and `f_r = −C·K^{1+p}/dᵖ` between all
//!   pairs (eq. 1/3), with `C = 0.2` and `p = 1` by default;
//! - repulsion approximated by a Barnes–Hut quadtree (§4) with opening
//!   criterion `d_S/d ≤ θ`, `θ = 1.2`, center-of-gravity supernodes, dense
//!   leaves at an adaptively tuned `max_tree_level` (valley search on the
//!   cost estimate `counts + 1.7·ns`);
//! - Gauss-Seidel sweeps (each vertex moves as soon as its force is known),
//!   the adaptive step-length scheme (§3.2, t = 0.9) from random initial
//!   layouts, simple `step := t·step` cooling during refinement (§5.3);
//! - termination when the per-sweep movement `‖x − x⁰‖ < K·tol`;
//! - multilevel coarsening (§5.1) by smallest-vertex-weight maximal
//!   matching, falling back to maximal-independent-vertex-set coarsening
//!   (the HYBRID scheme) when the ratio exceeds ρ = 0.75, down to two
//!   vertices;
//! - prolongation with coincident-pair jitter, scaled by the
//!   pseudo-diameter ratio of the levels (eq. 10).

use std::collections::{BTreeMap, HashMap, VecDeque};

use rayon::prelude::*;

use setreplace::Atom;

use crate::pcg::Pcg32;
use crate::vec2::{v2, BBox, V2};

const K: f64 = 1.0; // natural spring length
const C: f64 = 0.2; // relative repulsive strength
const T: f64 = 0.9; // step adaptivity
const THETA: f64 = 1.2; // Barnes-Hut opening criterion
const TOL: f64 = 0.01; // movement termination tolerance (× K)
const RHO: f64 = 0.75; // coarsening stall ratio
/// Below this size exact all-pairs repulsion beats the tree overhead.
const EXACT_REPULSION_LIMIT: usize = 100;
/// Above this size, force sweeps switch from Gauss-Seidel to parallel
/// Jacobi (forces computed from the sweep-start snapshot across threads).
/// Jacobi reads only the snapshot, so results are deterministic for any
/// thread count.
const PARALLEL_FORCE_THRESHOLD: usize = 4000;

#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Layout seed (deterministic per seed).
    pub seed: u64,
    /// `p` in the general repulsive force `f_r = −C·K^{1+p}/dᵖ` (paper
    /// eq. 3). `1.0` is the classic spring-electrical model and
    /// Mathematica's default; `2.0` reduces the "peripheral effect" on
    /// tree-like graphs.
    pub repulsive_exponent: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            seed: 0,
            repulsive_exponent: 1.0,
        }
    }
}

pub struct Layout {
    /// Position of every vertex, keyed by atom.
    pub positions: HashMap<Atom, V2>,
    /// Vertices in order of first appearance (matches WL's vertexList).
    pub vertices: Vec<Atom>,
}

/// Vertices in order of first appearance.
pub fn vertex_list(edges: &[Vec<Atom>]) -> Vec<Atom> {
    let mut seen = HashMap::new();
    let mut out = Vec::new();
    for edge in edges {
        for &a in edge {
            if seen.insert(a, ()).is_none() {
                out.push(a);
            }
        }
    }
    out
}

/// Cyclic springs for the layout graph: consecutive pairs plus the
/// wrap-around pair (Partition[edge, 2, 1, 1] in the original). Self-pairs
/// are dropped (they exert no force).
fn layout_springs(edges: &[Vec<Atom>], index: &HashMap<Atom, usize>) -> Vec<(usize, usize)> {
    let mut springs = Vec::new();
    for edge in edges {
        let k = edge.len();
        if k < 2 {
            continue;
        }
        for i in 0..k {
            let a = edge[i];
            let b = edge[(i + 1) % k];
            if a != b {
                springs.push((index[&a], index[&b]));
            }
        }
    }
    springs
}

/// The segments that will actually be drawn (ordered consecutive pairs),
/// used for the mean-edge-length rescale.
fn drawn_segments(edges: &[Vec<Atom>]) -> Vec<(Atom, Atom)> {
    let mut out = Vec::new();
    for edge in edges {
        for w in edge.windows(2) {
            out.push((w[0], w[1]));
        }
    }
    out
}

pub fn layout_hypergraph(edges: &[Vec<Atom>], seed: u64) -> Layout {
    layout_hypergraph_with(
        edges,
        &LayoutOptions {
            seed,
            ..Default::default()
        },
    )
}

pub fn layout_hypergraph_with(edges: &[Vec<Atom>], options: &LayoutOptions) -> Layout {
    let vertices = vertex_list(edges);
    let index: HashMap<Atom, usize> = vertices.iter().enumerate().map(|(i, &a)| (a, i)).collect();
    let n = vertices.len();
    if n == 0 {
        return Layout {
            positions: HashMap::new(),
            vertices,
        };
    }

    let springs = layout_springs(edges, &index);

    // Connected components (isolated vertices form their own).
    let component = components(n, &springs);
    let component_count = component.iter().copied().max().map_or(0, |m| m + 1);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); component_count];
    for (v, &c) in component.iter().enumerate() {
        members[c].push(v);
    }
    let mut springs_by_component: Vec<Vec<(usize, usize)>> = vec![Vec::new(); component_count];
    for &(a, b) in &springs {
        springs_by_component[component[a]].push((a, b));
    }

    let mut rng = Pcg32::new(options.seed);
    let mut positions = vec![v2(0.0, 0.0); n];
    let mut boxes: Vec<BBox> = Vec::with_capacity(component_count);
    for (c, verts) in members.iter().enumerate() {
        let local = force_layout(
            verts,
            &springs_by_component[c],
            &mut rng,
            options.repulsive_exponent,
        );
        let mut bbox = BBox::empty();
        for (&v, &p) in verts.iter().zip(local.iter()) {
            positions[v] = p;
            bbox.include(p);
        }
        if verts.len() == 1 {
            // A lone vertex still occupies space when packing.
            bbox = bbox.pad(0.5);
        }
        boxes.push(bbox);
    }

    if component_count > 1 {
        pack_components(&members, &boxes, &mut positions);
    }

    // Rescale so the mean drawn-segment length is 1.
    let segments = drawn_segments(edges);
    let mut total = 0.0;
    let mut count = 0usize;
    for &(a, b) in &segments {
        if a != b {
            total += positions[index[&a]].dist(positions[index[&b]]);
            count += 1;
        }
    }
    if count > 0 && total > 1e-9 {
        let factor = (count as f64) / total;
        for p in &mut positions {
            *p = *p * factor;
        }
    }

    Layout {
        positions: vertices
            .iter()
            .map(|&a| (a, positions[index[&a]]))
            .collect(),
        vertices,
    }
}

fn components(n: usize, springs: &[(usize, usize)]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in springs {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    // Renumber roots densely in order of first appearance.
    let mut root_ids: HashMap<usize, usize> = HashMap::new();
    (0..n)
        .map(|v| {
            let r = find(&mut parent, v);
            let next = root_ids.len();
            *root_ids.entry(r).or_insert(next)
        })
        .collect()
}

/// Lays out one connected component with the full multilevel algorithm,
/// then centers it and aligns the principal axis horizontally (Mathematica
/// embeddings come out wide; mirror orientation fixed deterministically).
fn force_layout(verts: &[usize], springs: &[(usize, usize)], rng: &mut Pcg32, p: f64) -> Vec<V2> {
    let n = verts.len();
    let local: HashMap<usize, usize> = verts.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    // Aggregate parallel springs into weights.
    let mut weights: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for &(a, b) in springs {
        let (a, b) = (local[&a], local[&b]);
        let key = (a.min(b), a.max(b));
        *weights.entry(key).or_insert(0.0) += 1.0;
    }
    let springs: Vec<(usize, usize, f64)> =
        weights.into_iter().map(|((a, b), w)| (a, b, w)).collect();
    let vertex_weights = vec![1.0; n];

    let mut pos = multilevel(n, &springs, &vertex_weights, rng, p);

    let mut center = v2(0.0, 0.0);
    for q in &pos {
        center += *q;
    }
    center = center * (1.0 / n as f64);
    for q in &mut pos {
        *q = *q - center;
    }
    pca_align(&mut pos);
    pos
}

// ---------------------------------------------------------------- multilevel

fn multilevel(
    n: usize,
    springs: &[(usize, usize, f64)],
    vertex_weights: &[f64],
    rng: &mut Pcg32,
    p: f64,
) -> Vec<V2> {
    if n <= 2 {
        return random_layout_refined(n, springs, rng, p);
    }
    match coarsen(n, springs, vertex_weights) {
        None => random_layout_refined(n, springs, rng, p),
        Some(coarse) => {
            let coarse_pos = multilevel(coarse.n, &coarse.springs, &coarse.weights, rng, p);
            // Eq. (10): expand the inherited layout by the pseudo-diameter
            // ratio of the two levels.
            let gamma = (pseudo_diameter(n, springs) / pseudo_diameter(coarse.n, &coarse.springs))
                .clamp(1.0, 8.0);
            let mut pos = prolong(n, springs, &coarse, &coarse_pos, gamma, rng);
            refine(&mut pos, springs, p, Cooling::Simple, 300);
            pos
        }
    }
}

/// Coarsest-level (or stalled-coarsening) layout: random initial placement
/// normalized so the mean spring length is K (the paper sets the coarsest
/// natural spring length to the random layout's average edge length), then
/// the adaptive scheme of §3.2.
fn random_layout_refined(
    n: usize,
    springs: &[(usize, usize, f64)],
    rng: &mut Pcg32,
    p: f64,
) -> Vec<V2> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![v2(0.0, 0.0)];
    }
    let radius = (n as f64).sqrt();
    let mut pos: Vec<V2> = (0..n)
        .map(|_| {
            let angle = rng.next_f64() * std::f64::consts::TAU;
            let r = radius * rng.next_f64().sqrt();
            v2(r * angle.cos(), r * angle.sin())
        })
        .collect();
    let mean = mean_spring_length(&pos, springs);
    if mean > 1e-9 {
        let factor = K / mean;
        for q in &mut pos {
            *q = *q * factor;
        }
    }
    refine(&mut pos, springs, p, Cooling::Adaptive, 1000);
    pos
}

fn mean_spring_length(pos: &[V2], springs: &[(usize, usize, f64)]) -> f64 {
    if springs.is_empty() {
        return 0.0;
    }
    springs
        .iter()
        .map(|&(a, b, _)| pos[a].dist(pos[b]))
        .sum::<f64>()
        / springs.len() as f64
}

/// Pseudo-diameter in hops: BFS from vertex 0 to its farthest vertex, then
/// the eccentricity of that vertex (double sweep).
fn pseudo_diameter(n: usize, springs: &[(usize, usize, f64)]) -> f64 {
    if n <= 1 {
        return 1.0;
    }
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b, _) in springs {
        adjacency[a].push(b);
        adjacency[b].push(a);
    }
    let bfs_farthest = |start: usize| -> (usize, usize) {
        let mut dist = vec![usize::MAX; n];
        let mut queue = VecDeque::new();
        dist[start] = 0;
        queue.push_back(start);
        let (mut far, mut far_d) = (start, 0);
        while let Some(u) = queue.pop_front() {
            for &w in &adjacency[u] {
                if dist[w] == usize::MAX {
                    dist[w] = dist[u] + 1;
                    if dist[w] > far_d {
                        far_d = dist[w];
                        far = w;
                    }
                    queue.push_back(w);
                }
            }
        }
        (far, far_d)
    };
    let (u, _) = bfs_farthest(0);
    let (_, d) = bfs_farthest(u);
    d.max(1) as f64
}

// ---------------------------------------------------------------- coarsening

enum CoarsePlan {
    /// Edge collapsing: fine vertex -> coarse vertex (matched pairs share).
    Matching { coarse_of: Vec<usize> },
    /// Maximal independent vertex set: only IS members map to coarse
    /// vertices; the rest interpolate from IS neighbors at prolongation.
    IndependentSet { coarse_of: Vec<Option<usize>> },
}

struct Coarse {
    n: usize,
    springs: Vec<(usize, usize, f64)>,
    weights: Vec<f64>,
    plan: CoarsePlan,
}

/// §5.1: edge collapsing by maximal matching, preferring the unmatched
/// neighbor with the smallest vertex weight (the paper adopts Walshaw's
/// choice; ties broken by heavier edge, then index). Edge weights sum when
/// parallel edges merge; vertex weights accumulate. If matching shrinks the
/// graph by less than ρ, fall back to MIVS coarsening (HYBRID): the coarse
/// vertices are a maximal independent set, joined when their fine-graph
/// distance is at most three. Returns None if both schemes stall.
fn coarsen(n: usize, springs: &[(usize, usize, f64)], vertex_weights: &[f64]) -> Option<Coarse> {
    let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(a, b, w) in springs {
        adjacency[a].push((b, w));
        adjacency[b].push((a, w));
    }

    // --- edge collapsing ---
    let mut mate: Vec<Option<usize>> = vec![None; n];
    for v in 0..n {
        if mate[v].is_some() {
            continue;
        }
        let mut best: Option<(f64, f64, usize)> = None; // (vertex weight, -edge weight, index)
        for &(u, ew) in &adjacency[v] {
            if u == v || mate[u].is_some() {
                continue;
            }
            let key = (vertex_weights[u], -ew, u);
            if best.is_none_or(|b| key < b) {
                best = Some(key);
            }
        }
        if let Some((_, _, u)) = best {
            mate[v] = Some(u);
            mate[u] = Some(v);
        }
    }
    let mut coarse_of = vec![usize::MAX; n];
    let mut next = 0usize;
    for v in 0..n {
        if coarse_of[v] != usize::MAX {
            continue;
        }
        coarse_of[v] = next;
        if let Some(u) = mate[v] {
            coarse_of[u] = next;
        }
        next += 1;
    }
    if (next as f64) <= RHO * n as f64 {
        let mut weights = vec![0.0; next];
        for v in 0..n {
            weights[coarse_of[v]] += vertex_weights[v];
        }
        let mut merged: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for &(a, b, w) in springs {
            let (ca, cb) = (coarse_of[a], coarse_of[b]);
            if ca != cb {
                *merged.entry((ca.min(cb), ca.max(cb))).or_insert(0.0) += w;
            }
        }
        return Some(Coarse {
            n: next,
            springs: merged.into_iter().map(|((a, b), w)| (a, b, w)).collect(),
            weights,
            plan: CoarsePlan::Matching { coarse_of },
        });
    }

    // --- MIVS fallback ---
    let mut in_set = vec![false; n];
    let mut blocked = vec![false; n];
    for v in 0..n {
        if !blocked[v] {
            in_set[v] = true;
            for &(u, _) in &adjacency[v] {
                blocked[u] = true;
            }
        }
    }
    let mut coarse_of: Vec<Option<usize>> = vec![None; n];
    let mut next = 0usize;
    for (v, &in_s) in in_set.iter().enumerate() {
        if in_s {
            coarse_of[v] = Some(next);
            next += 1;
        }
    }
    if (next as f64) > RHO * n as f64 || next < 2 {
        return None;
    }
    // Coarse edges link IS vertices at fine-graph distance <= 3.
    let mut merged: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut dist = vec![usize::MAX; n];
    let mut touched: Vec<usize> = Vec::new();
    for v in 0..n {
        let Some(cv) = coarse_of[v] else { continue };
        dist[v] = 0;
        touched.push(v);
        let mut frontier = vec![v];
        for d in 1..=3usize {
            let mut next_frontier = Vec::new();
            for &u in &frontier {
                for &(w, _) in &adjacency[u] {
                    if dist[w] == usize::MAX {
                        dist[w] = d;
                        touched.push(w);
                        next_frontier.push(w);
                        if let Some(cw) = coarse_of[w] {
                            if cw > cv {
                                merged.entry((cv, cw)).or_insert(1.0);
                            }
                        }
                    }
                }
            }
            frontier = next_frontier;
        }
        for &u in &touched {
            dist[u] = usize::MAX;
        }
        touched.clear();
    }
    Some(Coarse {
        n: next,
        springs: merged.into_iter().map(|((a, b), w)| (a, b, w)).collect(),
        weights: vec![1.0; next],
        plan: CoarsePlan::IndependentSet { coarse_of },
    })
}

/// §5.3 prolongation: scale the coarse layout by γ (eq. 10), then place
/// fine vertices — matched pairs both at the parent (jittered apart), MIVS
/// non-members at the average of their independent-set neighbors.
fn prolong(
    n: usize,
    springs: &[(usize, usize, f64)],
    coarse: &Coarse,
    coarse_pos: &[V2],
    gamma: f64,
    rng: &mut Pcg32,
) -> Vec<V2> {
    let scaled: Vec<V2> = coarse_pos.iter().map(|&q| q * gamma).collect();
    let jitter = |rng: &mut Pcg32| {
        let angle = rng.next_f64() * std::f64::consts::TAU;
        v2(angle.cos(), angle.sin()) * (0.05 * K)
    };
    match &coarse.plan {
        CoarsePlan::Matching { coarse_of } => {
            (0..n).map(|v| scaled[coarse_of[v]] + jitter(rng)).collect()
        }
        CoarsePlan::IndependentSet { coarse_of } => {
            let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
            for &(a, b, _) in springs {
                adjacency[a].push(b);
                adjacency[b].push(a);
            }
            (0..n)
                .map(|v| match coarse_of[v] {
                    Some(cv) => scaled[cv],
                    None => {
                        let mut sum = v2(0.0, 0.0);
                        let mut count = 0usize;
                        for &u in &adjacency[v] {
                            if let Some(cu) = coarse_of[u] {
                                sum += scaled[cu];
                                count += 1;
                            }
                        }
                        if count == 0 {
                            jitter(rng)
                        } else {
                            sum * (1.0 / count as f64) + jitter(rng)
                        }
                    }
                })
                .collect()
        }
    }
}

// --------------------------------------------------------------- refinement

enum Cooling {
    /// §3.2 adaptive scheme (trust-region style), for random initial layouts.
    Adaptive,
    /// Simple `step := t·step` (Walshaw), preferred during refinement (§5.3).
    Simple,
}

/// Force iteration per Algorithm 1: Gauss-Seidel sweeps (each vertex moves
/// as soon as its force is computed), repulsion via the Barnes–Hut quadtree
/// above `EXACT_REPULSION_LIMIT` vertices, terminating when the per-sweep
/// movement drops below K·tol.
fn refine(
    pos: &mut [V2],
    springs: &[(usize, usize, f64)],
    p: f64,
    cooling: Cooling,
    max_sweeps: usize,
) {
    let n = pos.len();
    if n <= 1 {
        return;
    }
    let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(a, b, w) in springs {
        adjacency[a].push((b, w));
        adjacency[b].push((a, w));
    }

    let mut step = match cooling {
        Cooling::Adaptive => K,
        Cooling::Simple => 0.35 * K,
    };
    let mut progress = 0u32;
    let mut previous_energy = f64::INFINITY;
    let use_tree = n > EXACT_REPULSION_LIMIT;
    let mut tree = QuadTree::default();
    let mut tuner = DepthTuner::new();
    let mut snapshot: Vec<V2> = Vec::with_capacity(n);

    for _ in 0..max_sweeps {
        snapshot.clear();
        snapshot.extend_from_slice(pos);
        if use_tree {
            tree.build(pos, tuner.depth());
        }
        let mut energy = 0.0;
        let mut traversed = 0u64;
        let mut supernodes = 0u64;
        if n > PARALLEL_FORCE_THRESHOLD {
            // Parallel Jacobi: all forces from the snapshot, then apply.
            let forces: Vec<(V2, u64, u64)> = (0..n)
                .into_par_iter()
                .map(|i| {
                    let mut force = v2(0.0, 0.0);
                    for &(j, w) in &adjacency[i] {
                        let delta = snapshot[j] - snapshot[i];
                        let d = delta.norm().max(1e-9);
                        force += delta * (w * d / K);
                    }
                    let (mut tr, mut sn) = (0u64, 0u64);
                    force += tree.repulsion(&snapshot, i, p, &mut tr, &mut sn);
                    (force, tr, sn)
                })
                .collect();
            for (i, &(force, tr, sn)) in forces.iter().enumerate() {
                let magnitude = force.norm();
                if magnitude > 1e-12 {
                    pos[i] += force * (step / magnitude);
                }
                energy += magnitude * magnitude;
                traversed += tr;
                supernodes += sn;
            }
        } else {
            // Gauss-Seidel: each vertex moves as soon as its force is known.
            for i in 0..n {
                let mut force = v2(0.0, 0.0);
                for &(j, w) in &adjacency[i] {
                    // f_a = w·d²/K along the spring.
                    let delta = pos[j] - pos[i];
                    let d = delta.norm().max(1e-9);
                    force += delta * (w * d / K);
                }
                if use_tree {
                    force += tree.repulsion(pos, i, p, &mut traversed, &mut supernodes);
                } else {
                    for j in 0..n {
                        if j != i {
                            force += pair_repulsion(pos[i], pos[j], p, pair_salt(i, j));
                        }
                    }
                }
                let magnitude = force.norm();
                if magnitude > 1e-12 {
                    pos[i] += force * (step / magnitude);
                }
                energy += magnitude * magnitude;
            }
        }
        if use_tree {
            tuner.observe(traversed as f64 + 1.7 * supernodes as f64);
        }
        match cooling {
            Cooling::Adaptive => {
                if energy < previous_energy {
                    progress += 1;
                    if progress >= 5 {
                        progress = 0;
                        step /= T;
                    }
                } else {
                    progress = 0;
                    step *= T;
                }
                previous_energy = energy;
            }
            Cooling::Simple => step *= T,
        }
        // Convergence: movement of the whole layout below K·tol.
        let movement: f64 = pos
            .iter()
            .zip(snapshot.iter())
            .map(|(a, b)| {
                let d = *a - *b;
                d.x * d.x + d.y * d.y
            })
            .sum::<f64>()
            .sqrt();
        if movement < K * TOL {
            break;
        }
    }
}

/// Repulsive force from j on i: magnitude C·K^{1+p}/dᵖ away from j.
/// Coincident points separate along a stateless hashed direction (keyed by
/// the pair), keeping the hot loop free of shared RNG state so it can run
/// in parallel deterministically.
fn pair_repulsion(xi: V2, xj: V2, p: f64, salt: u64) -> V2 {
    let mut delta = xi - xj;
    let mut d = delta.norm();
    if d < 1e-6 {
        delta = hashed_direction(salt) * 0.5;
        d = 0.5;
    }
    delta * (C * K.powf(1.0 + p) / d.powf(p + 1.0))
}

fn pair_salt(i: usize, j: usize) -> u64 {
    ((i as u64) << 32) ^ j as u64
}

/// splitmix64-derived unit vector.
fn hashed_direction(salt: u64) -> V2 {
    let mut z = salt.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let angle = (z as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    v2(angle.cos(), angle.sin())
}

// ------------------------------------------------------- Barnes-Hut quadtree

/// Arena quadtree (§4). Leaves hold their member vertices as a linked chain
/// through `chain`; internal nodes carry mass and center-of-gravity sums.
/// Squares with more than one vertex split until `max_level`, below which
/// they become dense leaves.
#[derive(Default)]
struct QuadTree {
    nodes: Vec<QuadNode>,
    /// Per-vertex next pointer of its leaf's member chain (u32::MAX ends).
    chain: Vec<u32>,
}

struct QuadNode {
    center: V2,
    half: f64,
    com_sum: V2,
    mass: f64,
    children: [u32; 4],
    /// Head of the member chain (leaves only).
    head: u32,
    leaf: bool,
}

impl QuadNode {
    fn new(center: V2, half: f64) -> QuadNode {
        QuadNode {
            center,
            half,
            com_sum: v2(0.0, 0.0),
            mass: 0.0,
            children: [u32::MAX; 4],
            head: u32::MAX,
            leaf: true,
        }
    }
}

impl QuadTree {
    fn build(&mut self, pos: &[V2], max_level: usize) {
        self.nodes.clear();
        self.chain.clear();
        self.chain.resize(pos.len(), u32::MAX);
        let mut bbox = BBox::empty();
        for q in pos {
            bbox.include(*q);
        }
        let half = (0.5 * bbox.width().max(bbox.height())).max(1e-9) * 1.0001;
        let center = v2(
            0.5 * (bbox.min.x + bbox.max.x),
            0.5 * (bbox.min.y + bbox.max.y),
        );
        self.nodes.push(QuadNode::new(center, half));
        for i in 0..pos.len() {
            self.insert(0, i as u32, pos, 0, max_level);
        }
    }

    fn insert(&mut self, node: usize, vertex: u32, pos: &[V2], level: usize, max_level: usize) {
        let q = pos[vertex as usize];
        self.nodes[node].com_sum += q;
        self.nodes[node].mass += 1.0;
        if self.nodes[node].leaf {
            if self.nodes[node].mass <= 1.0 || level >= max_level {
                // Single occupant, or a dense leaf at the depth cap.
                self.chain[vertex as usize] = self.nodes[node].head;
                self.nodes[node].head = vertex;
                return;
            }
            // Split: re-insert the existing chain into children.
            let mut at = self.nodes[node].head;
            self.nodes[node].head = u32::MAX;
            self.nodes[node].leaf = false;
            while at != u32::MAX {
                let next = self.chain[at as usize];
                self.chain[at as usize] = u32::MAX;
                self.insert_into_child(node, at, pos, level, max_level);
                at = next;
            }
        }
        self.insert_into_child(node, vertex, pos, level, max_level);
    }

    fn insert_into_child(
        &mut self,
        node: usize,
        vertex: u32,
        pos: &[V2],
        level: usize,
        max_level: usize,
    ) {
        let q = pos[vertex as usize];
        let center = self.nodes[node].center;
        let half = self.nodes[node].half;
        let quadrant = usize::from(q.x >= center.x) | (usize::from(q.y >= center.y) << 1);
        let child = self.nodes[node].children[quadrant];
        let child = if child == u32::MAX {
            let child_center = v2(
                center.x
                    + if quadrant & 1 == 1 {
                        half / 2.0
                    } else {
                        -half / 2.0
                    },
                center.y
                    + if quadrant & 2 == 2 {
                        half / 2.0
                    } else {
                        -half / 2.0
                    },
            );
            let idx = self.nodes.len() as u32;
            self.nodes.push(QuadNode::new(child_center, half / 2.0));
            self.nodes[node].children[quadrant] = idx;
            idx
        } else {
            child
        };
        self.insert(child as usize, vertex, pos, level + 1, max_level);
    }

    /// Total repulsive force on vertex `i`: supernode approximation
    /// (center of gravity, force × |S|) where `d_S/d ≤ θ`, exact pairs in
    /// leaves. Counts traversed nodes and supernode uses for depth tuning.
    fn repulsion(
        &self,
        pos: &[V2],
        i: usize,
        p: f64,
        traversed: &mut u64,
        supernodes: &mut u64,
    ) -> V2 {
        let xi = pos[i];
        let mut force = v2(0.0, 0.0);
        let mut stack: Vec<u32> = vec![0];
        while let Some(idx) = stack.pop() {
            let node = &self.nodes[idx as usize];
            *traversed += 1;
            if node.mass <= 0.0 {
                continue;
            }
            if node.leaf {
                let mut at = node.head;
                while at != u32::MAX {
                    if at as usize != i {
                        force += pair_repulsion(xi, pos[at as usize], p, pair_salt(i, at as usize));
                    }
                    at = self.chain[at as usize];
                }
                continue;
            }
            let com = node.com_sum * (1.0 / node.mass);
            let d = xi.dist(com);
            let width = node.half * 2.0;
            if d > 1e-9 && width / d <= THETA {
                // f_r(i, S) = −|S|·C·K^{1+p}/dᵖ towards i.
                let delta = xi - com;
                force += delta * (node.mass * C * K.powf(1.0 + p) / d.powf(p + 1.0));
                *supernodes += 1;
            } else {
                for &child in &node.children {
                    if child != u32::MAX {
                        stack.push(child);
                    }
                }
            }
        }
        force
    }
}

/// §4's adaptive `max_tree_level`: a valley search on the per-sweep cost
/// estimate `h = counts + 1.7·ns`, starting from depth 8, settling on the
/// best depth once a previously tried depth is reached again.
struct DepthTuner {
    current: usize,
    direction: i64,
    tried: Vec<(usize, f64)>,
    settled: bool,
}

impl DepthTuner {
    fn new() -> DepthTuner {
        DepthTuner {
            current: 8,
            direction: 1,
            tried: Vec::new(),
            settled: false,
        }
    }

    fn depth(&self) -> usize {
        self.current
    }

    fn observe(&mut self, h: f64) {
        if self.settled {
            return;
        }
        self.tried.push((self.current, h));
        if self.tried.len() >= 2 {
            let last = self.tried[self.tried.len() - 1].1;
            let before = self.tried[self.tried.len() - 2].1;
            if last > before {
                self.direction = -self.direction;
            }
        }
        let next = (self.current as i64 + self.direction).clamp(3, 20) as usize;
        if self.tried.iter().any(|&(d, _)| d == next) {
            self.current = self
                .tried
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap()
                .0;
            self.settled = true;
        } else {
            self.current = next;
        }
    }
}

/// Rotates points so the principal component lies along x, with mirror
/// orientation fixed by third moments (deterministic).
fn pca_align(pos: &mut [V2]) {
    let n = pos.len() as f64;
    if pos.len() < 2 {
        return;
    }
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in pos.iter() {
        sxx += p.x * p.x;
        sxy += p.x * p.y;
        syy += p.y * p.y;
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (sin, cos) = (-theta).sin_cos();
    for p in pos.iter_mut() {
        *p = v2(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
    }
    let skew_x: f64 = pos.iter().map(|p| p.x * p.x * p.x).sum::<f64>() / n;
    let skew_y: f64 = pos.iter().map(|p| p.y * p.y * p.y).sum::<f64>() / n;
    for p in pos.iter_mut() {
        if skew_x < 0.0 {
            p.x = -p.x;
        }
        if skew_y < 0.0 {
            p.y = -p.y;
        }
    }
}

/// Shelf-packs component bounding boxes into rows, aiming for a roughly
/// 4:3 overall aspect ratio (stand-in for GraphPlot's component packing).
fn pack_components(members: &[Vec<usize>], boxes: &[BBox], positions: &mut [V2]) {
    let padding = 0.8;
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| {
        let (wa, ha) = (boxes[a].width(), boxes[a].height());
        let (wb, hb) = (boxes[b].width(), boxes[b].height());
        (wb * hb).partial_cmp(&(wa * ha)).unwrap().then(a.cmp(&b))
    });
    let total_area: f64 = boxes
        .iter()
        .map(|b| (b.width() + padding) * (b.height() + padding))
        .sum();
    let target_width = (total_area * 4.0 / 3.0).sqrt();

    let mut cursor = v2(0.0, 0.0);
    let mut row_height: f64 = 0.0;
    for &c in &order {
        let w = boxes[c].width() + padding;
        let h = boxes[c].height() + padding;
        if cursor.x > 1e-9 && cursor.x + w > target_width {
            cursor = v2(0.0, cursor.y - row_height);
            row_height = 0.0;
        }
        let offset = cursor - boxes[c].min;
        for &v in &members[c] {
            positions[v] += offset;
        }
        cursor.x += w;
        row_height = row_height.max(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The quadtree force approximation must stay close to the exact sum.
    #[test]
    fn quadtree_matches_exact_repulsion() {
        let mut rng = Pcg32::new(11);
        let pos: Vec<V2> = (0..600)
            .map(|_| v2(rng.next_f64() * 20.0, rng.next_f64() * 20.0))
            .collect();
        let mut tree = QuadTree::default();
        tree.build(&pos, 12);
        let (mut t, mut s) = (0, 0);
        // Net forces on interior vertices nearly cancel, so plain relative
        // error is ill-conditioned. Normalize by the total interaction
        // magnitude instead, which bounds the per-term approximation error
        // of Hu's deliberately coarse theta = 1.2.
        let mut total_err = 0.0;
        for i in 0..pos.len() {
            let approx = tree.repulsion(&pos, i, 1.0, &mut t, &mut s);
            let mut exact = v2(0.0, 0.0);
            let mut magnitude_sum = 0.0;
            for j in 0..pos.len() {
                if j != i {
                    let f = pair_repulsion(pos[i], pos[j], 1.0, pair_salt(i, j));
                    magnitude_sum += f.norm();
                    exact += f;
                }
            }
            let err = (approx - exact).norm() / magnitude_sum.max(1e-12);
            assert!(
                err < 0.05,
                "normalized error {err} too large for vertex {i}"
            );
            total_err += err;
        }
        let mean_err = total_err / pos.len() as f64;
        assert!(
            mean_err < 0.02,
            "mean normalized error {mean_err} too large"
        );
        assert!(s > 0, "no supernode approximations used");
    }

    /// The parallel Jacobi path (n > PARALLEL_FORCE_THRESHOLD) must give
    /// identical results regardless of thread count: forces read only the
    /// sweep-start snapshot. Slow in debug; run with --release --ignored.
    #[test]
    #[ignore]
    fn parallel_layout_thread_count_independent() {
        let edges: Vec<Vec<Atom>> = (1..6000)
            .map(|i| vec![i as Atom, (i + 1) as Atom])
            .collect();
        let a = layout_hypergraph(&edges, 5);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let b = pool.install(|| layout_hypergraph(&edges, 5));
        for (atom, p) in &a.positions {
            let q = b.positions[atom];
            assert!((p.x - q.x).abs() < 1e-12 && (p.y - q.y).abs() < 1e-12);
        }
    }

    /// Layouts remain deterministic per seed with the tree path engaged.
    #[test]
    fn large_layout_deterministic() {
        let edges: Vec<Vec<Atom>> = (1..400).map(|i| vec![i as Atom, (i + 1) as Atom]).collect();
        let a = layout_hypergraph(&edges, 3);
        let b = layout_hypergraph(&edges, 3);
        assert_eq!(a.positions.len(), b.positions.len());
        for (atom, p) in &a.positions {
            let q = b.positions[atom];
            assert!((p.x - q.x).abs() < 1e-12 && (p.y - q.y).abs() < 1e-12);
        }
    }
}
