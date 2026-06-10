//! Spring-electrical hypergraph embedding, following SetReplace's
//! HypergraphPlot.m:
//!
//! - The *layout* graph treats every hyperedge as a **cyclic** path: a
//!   hyperedge {a, b, c} contributes springs a–b, b–c, c–a (a binary edge
//!   {a, b} contributes the spring twice). Only the *ordered* consecutive
//!   pairs are drawn later.
//! - Disconnected components are laid out independently and packed.
//! - The final embedding is rescaled so the mean drawn-segment length is 1
//!   (`rescaleEmbedding`/`edgeScale` in the original), which puts vertex
//!   radii and arrowhead lengths in natural units.

use std::collections::{BTreeMap, HashMap};

use setreplace::Atom;

use crate::pcg::Pcg32;
use crate::vec2::{v2, BBox, V2};

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
    let vertices = vertex_list(edges);
    let index: HashMap<Atom, usize> = vertices
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, i))
        .collect();
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

    let mut rng = Pcg32::new(seed);
    let mut positions = vec![v2(0.0, 0.0); n];
    let mut boxes: Vec<BBox> = Vec::with_capacity(component_count);
    for (c, verts) in members.iter().enumerate() {
        let local = force_layout(verts, &springs_by_component[c], &mut rng);
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

/// Spring-electrical layout of one component using Yifan Hu's multilevel
/// adaptive scheme (the algorithm behind Mathematica's
/// SpringElectricalEmbedding): the graph is recursively coarsened by
/// heavy-edge matching, laid out at the coarsest level, and the positions
/// are interpolated back up with force refinement at each level. This is
/// what lets large structured graphs (meshes, fractals) unfold globally
/// instead of freezing in a tangled local minimum. Deterministic given the
/// RNG state.
fn force_layout(verts: &[usize], springs: &[(usize, usize)], rng: &mut Pcg32) -> Vec<V2> {
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

    let mut pos = multilevel(n, &springs, rng);

    // Center on the origin and align the principal axis horizontally, as
    // Mathematica's embeddings come out (deterministic mirror choice).
    let mut center = v2(0.0, 0.0);
    for p in &pos {
        center += *p;
    }
    center = center * (1.0 / n as f64);
    for p in &mut pos {
        *p = *p - center;
    }
    pca_align(&mut pos);
    pos
}

/// Below this size, lay out directly from random initial positions.
const COARSEST: usize = 40;

fn multilevel(n: usize, springs: &[(usize, usize, f64)], rng: &mut Pcg32) -> Vec<V2> {
    if n == 1 {
        return vec![v2(0.0, 0.0)];
    }
    if n <= COARSEST {
        let radius = (n as f64).sqrt();
        let mut pos: Vec<V2> = (0..n)
            .map(|_| {
                let angle = rng.next_f64() * std::f64::consts::TAU;
                let r = radius * rng.next_f64().sqrt();
                v2(r * angle.cos(), r * angle.sin())
            })
            .collect();
        refine(&mut pos, springs, 1.0, 1000, rng);
        return pos;
    }

    let (coarse_of, coarse_n) = heavy_edge_matching(n, springs);
    if coarse_n as f64 > 0.95 * n as f64 {
        // Matching failed to shrink the graph (e.g. a star); lay out here.
        let radius = (n as f64).sqrt();
        let mut pos: Vec<V2> = (0..n)
            .map(|_| {
                let angle = rng.next_f64() * std::f64::consts::TAU;
                let r = radius * rng.next_f64().sqrt();
                v2(r * angle.cos(), r * angle.sin())
            })
            .collect();
        refine(&mut pos, springs, 1.0, 1000, rng);
        return pos;
    }

    // Coarse springs: project and merge.
    let mut coarse_weights: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for &(a, b, w) in springs {
        let (ca, cb) = (coarse_of[a], coarse_of[b]);
        if ca != cb {
            let key = (ca.min(cb), ca.max(cb));
            *coarse_weights.entry(key).or_insert(0.0) += w;
        }
    }
    let coarse_springs: Vec<(usize, usize, f64)> = coarse_weights
        .into_iter()
        .map(|((a, b), w)| (a, b, w))
        .collect();

    let coarse_pos = multilevel(coarse_n, &coarse_springs, rng);

    // Prolong: each vertex starts at its coarse representative, slightly
    // displaced so matched pairs can separate.
    let mut pos: Vec<V2> = (0..n)
        .map(|v| {
            let angle = rng.next_f64() * std::f64::consts::TAU;
            coarse_pos[coarse_of[v]] + v2(angle.cos(), angle.sin()) * 0.05
        })
        .collect();
    // The coarse layout's scale roughly doubles in vertex count terms;
    // expand so densities stay comparable before refining.
    let growth = (n as f64 / coarse_n as f64).sqrt();
    for p in &mut pos {
        *p = *p * growth;
    }
    refine(&mut pos, springs, 0.35, 250, rng);
    pos
}

/// Greedy heavy-edge matching: visits vertices in index order, pairing each
/// unmatched vertex with its heaviest unmatched neighbor. Returns the
/// fine→coarse index map and the coarse vertex count. Deterministic.
fn heavy_edge_matching(n: usize, springs: &[(usize, usize, f64)]) -> (Vec<usize>, usize) {
    let mut neighbors: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(a, b, w) in springs {
        neighbors[a].push((b, w));
        neighbors[b].push((a, w));
    }
    let mut mate: Vec<Option<usize>> = vec![None; n];
    for v in 0..n {
        if mate[v].is_some() {
            continue;
        }
        let mut best: Option<(usize, f64)> = None;
        for &(u, w) in &neighbors[v] {
            if u != v && mate[u].is_none() {
                let better = match best {
                    None => true,
                    Some((bu, bw)) => w > bw || (w == bw && u < bu),
                };
                if better {
                    best = Some((u, w));
                }
            }
        }
        if let Some((u, _)) = best {
            mate[v] = Some(u);
            mate[u] = Some(v);
        }
    }
    let mut coarse_of = vec![usize::MAX; n];
    let mut next = 0;
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
    (coarse_of, next)
}

/// Hu's adaptive force iteration: attractive force w·d²/K along springs,
/// repulsive force C·K²/d between all pairs, every vertex moving a fixed
/// adaptive step along its force direction.
fn refine(
    pos: &mut [V2],
    springs: &[(usize, usize, f64)],
    initial_step: f64,
    max_iterations: usize,
    rng: &mut Pcg32,
) {
    let n = pos.len();
    let k = 1.0; // natural spring length
    let c = 0.2; // relative repulsive strength (Hu's default)
    let t = 0.9; // step adaptivity
    let mut step = initial_step * k;
    let mut progress = 0u32;
    let mut previous_energy = f64::INFINITY;

    let mut force = vec![v2(0.0, 0.0); n];
    for _ in 0..max_iterations {
        for f in &mut force {
            *f = v2(0.0, 0.0);
        }
        for i in 0..n {
            for j in (i + 1)..n {
                let mut delta = pos[i] - pos[j];
                let mut d = delta.norm();
                if d < 1e-6 {
                    // Deterministic jitter for coincident points.
                    delta = v2(rng.next_f64() - 0.5, rng.next_f64() - 0.5);
                    d = delta.norm().max(1e-6);
                }
                let push = delta * (c * k * k / (d * d));
                force[i] += push;
                force[j] += -push;
            }
        }
        for &(a, b, w) in springs {
            let delta = pos[a] - pos[b];
            let d = delta.norm().max(1e-6);
            let pull = delta * (w * d / k); // magnitude w·d²/k along the unit vector
            force[a] += -pull;
            force[b] += pull;
        }

        let energy: f64 = force.iter().map(|f| f.norm() * f.norm()).sum();
        for i in 0..n {
            let m = force[i].norm();
            if m > 1e-12 {
                pos[i] += force[i] * (step / m);
            }
        }
        // Adaptive step update (Hu 2005).
        if energy < previous_energy {
            progress += 1;
            if progress >= 5 {
                progress = 0;
                step /= t;
            }
        } else {
            progress = 0;
            step *= t;
        }
        previous_energy = energy;
        if step < 1e-4 * k {
            break;
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
        (wb * hb)
            .partial_cmp(&(wa * ha))
            .unwrap()
            .then(a.cmp(&b))
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
