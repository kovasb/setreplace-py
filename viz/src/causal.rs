//! LayeredCausalGraph: events as orange disks, causal edges as dark-red
//! arrows, layered top-to-bottom by event generation — exactly the layer
//! assignment SetReplace passes to "LayeredDigraphEmbedding"
//! (layer = generations_count - event generation).

use std::collections::HashMap;

use setreplace::HypergraphSystem;

use crate::geometry::arrow;
use crate::style;
use crate::svg::{Frame, Svg};
use crate::vec2::{v2, BBox, V2};

pub struct CausalGraphOptions {
    /// Include the initial pseudo-event (drawn in blue), as
    /// "IncludeBoundaryEvents" -> "Initial".
    pub include_initial: bool,
    /// Target plot width in printer's points.
    pub target_width_pt: f64,
}

impl Default for CausalGraphOptions {
    fn default() -> Self {
        CausalGraphOptions {
            include_initial: false,
            target_width_pt: 478.0,
        }
    }
}

pub fn layered_causal_graph_svg(system: &HypergraphSystem, opts: &CausalGraphOptions) -> String {
    let events = system.events();
    let causal_edges = system.causal_graph_edges(opts.include_initial);
    let first_event = if opts.include_initial { 0 } else { 1 };
    let ids: Vec<usize> = (first_event..events.len()).collect();
    if ids.is_empty() {
        return Svg::new(100.0, 100.0).finish();
    }

    // Layer = event generation (generation 1 on top; the initial event, if
    // shown, sits above at generation 0).
    let min_gen = events[ids[0]].generation;
    let max_gen = ids.iter().map(|&id| events[id].generation).max().unwrap();
    let layer_count = (max_gen - min_gen + 1) as usize;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); layer_count];
    for &id in &ids {
        layers[(events[id].generation - min_gen) as usize].push(id);
    }

    let adjacency = causal_edges;
    let positions = layered_positions(&layers, &adjacency);

    // Geometry in layout units: rows 1.0 apart, siblings >= 1.0 apart.
    let vertex_r = 0.032;
    let arrowhead_len = 0.12;
    let row_gap = 1.0;
    let pos: HashMap<usize, V2> = positions
        .iter()
        .map(|(&id, &x)| {
            let layer = (events[id].generation - min_gen) as f64;
            (id, v2(x, -layer * row_gap))
        })
        .collect();

    let mut bbox = BBox::empty();
    for p in pos.values() {
        bbox.include(*p + v2(vertex_r, vertex_r));
        bbox.include(*p - v2(vertex_r, vertex_r));
    }
    let bbox = bbox.pad(0.15);

    let pt_per_unit =
        (style::MAX_IMAGE_SIZE.0 / bbox.width()).min(style::MAX_IMAGE_SIZE.1 / bbox.height());
    let upscale = opts.target_width_pt / style::MAX_IMAGE_SIZE.0;
    let px_per_unit = pt_per_unit * upscale * 2.0;

    let frame = Frame {
        scale: px_per_unit,
        world_min: bbox.min,
        world_max_y: bbox.max.y,
    };
    let mut svg = Svg::new(bbox.width() * px_per_unit, bbox.height() * px_per_unit);
    let stroke_px = 1.7;

    // Multi-edges between the same event pair separate into symmetric arcs,
    // as in Mathematica's Graph rendering.
    let mut pair_counts: HashMap<(usize, usize), usize> = HashMap::new();
    for &(from, to) in &adjacency {
        *pair_counts.entry((from, to)).or_insert(0) += 1;
    }
    let mut pair_seen: HashMap<(usize, usize), usize> = HashMap::new();
    for &(from, to) in &adjacency {
        let total = pair_counts[&(from, to)];
        let index = *pair_seen
            .entry((from, to))
            .and_modify(|i| *i += 1)
            .or_insert(0);
        let (pa, pb) = (pos[&from], pos[&to]);
        let offset = 0.22 * (index as f64 - (total as f64 - 1.0) / 2.0);
        let seg = if offset.abs() < 1e-9 {
            vec![pa, pb]
        } else {
            let control = pa.lerp(pb, 0.5) + (pb - pa).perp().normalized() * offset;
            crate::geometry::sample_qbezier(pa, control, pb, 24)
        };
        let (line, head) = arrow(&seg, vertex_r, arrowhead_len);
        let line_px: Vec<V2> = line.iter().map(|p| frame.to_px(*p)).collect();
        svg.polyline(&line_px, style::CAUSAL_EDGE, stroke_px);
        if head.len() >= 3 {
            let head_px: Vec<V2> = head.iter().map(|p| frame.to_px(*p)).collect();
            svg.polygon(&head_px, style::CAUSAL_EDGE);
        }
    }
    for &id in &ids {
        let fill = if id == 0 {
            style::INITIAL_EVENT_VERTEX
        } else {
            style::EVENT_VERTEX
        };
        svg.circle(
            frame.to_px(pos[&id]),
            frame.len_px(vertex_r),
            Some(fill),
            Some((fill, stroke_px * 0.5)),
        );
    }

    svg.finish()
}

/// Barycenter crossing reduction plus iterative coordinate relaxation:
/// the small/standard recipe for layered DAG drawing.
fn layered_positions(layers: &[Vec<usize>], edges: &[(usize, usize)]) -> HashMap<usize, f64> {
    let mut parents: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(a, b) in edges {
        children.entry(a).or_default().push(b);
        parents.entry(b).or_default().push(a);
    }

    let mut order: Vec<Vec<usize>> = layers.to_vec();
    let mut x: HashMap<usize, f64> = HashMap::new();
    for row in &order {
        for (i, &id) in row.iter().enumerate() {
            x.insert(id, i as f64);
        }
    }

    let mean_x = |ids: Option<&Vec<usize>>, x: &HashMap<usize, f64>, fallback: f64| {
        ids.filter(|v| !v.is_empty())
            .map(|v| v.iter().map(|n| x[n]).sum::<f64>() / v.len() as f64)
            .unwrap_or(fallback)
    };

    // Median/barycenter ordering sweeps.
    for sweep in 0..8 {
        let top_down = sweep % 2 == 0;
        let rows: Vec<usize> = if top_down {
            (0..order.len()).collect()
        } else {
            (0..order.len()).rev().collect()
        };
        for r in rows {
            let neighbor_map = if top_down { &parents } else { &children };
            let mut keyed: Vec<(f64, usize)> = order[r]
                .iter()
                .map(|&id| (mean_x(neighbor_map.get(&id), &x, x[&id]), id))
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
            order[r] = keyed.iter().map(|&(_, id)| id).collect();
            for (i, &id) in order[r].iter().enumerate() {
                x.insert(id, i as f64);
            }
        }
    }

    // Coordinate relaxation: pull towards neighbor means, enforce minimum
    // separation of 1.0 within each row.
    for round in 0..30 {
        let top_down = round % 2 == 0;
        let rows: Vec<usize> = if top_down {
            (0..order.len()).collect()
        } else {
            (0..order.len()).rev().collect()
        };
        for r in rows {
            for &id in &order[r] {
                let from_parents = mean_x(parents.get(&id), &x, x[&id]);
                let from_children = mean_x(children.get(&id), &x, x[&id]);
                let target = match (parents.get(&id), children.get(&id)) {
                    (Some(p), Some(c)) if !p.is_empty() && !c.is_empty() => {
                        0.5 * (from_parents + from_children)
                    }
                    (Some(p), _) if !p.is_empty() => from_parents,
                    (_, Some(c)) if !c.is_empty() => from_children,
                    _ => x[&id],
                };
                x.insert(id, target);
            }
            // Enforce min separation, keeping the row roughly centered.
            let mut row_sorted: Vec<usize> = order[r].clone();
            row_sorted.sort_by(|a, b| x[a].partial_cmp(&x[b]).unwrap().then(a.cmp(b)));
            for i in 1..row_sorted.len() {
                let (prev, cur) = (row_sorted[i - 1], row_sorted[i]);
                if x[&cur] < x[&prev] + 1.0 {
                    x.insert(cur, x[&prev] + 1.0);
                }
            }
            for i in (0..row_sorted.len().saturating_sub(1)).rev() {
                let (cur, next) = (row_sorted[i], row_sorted[i + 1]);
                if x[&cur] > x[&next] - 1.0 {
                    x.insert(cur, x[&next] - 1.0);
                }
            }
            order[r] = row_sorted;
        }
    }

    // Center each connected row block around the global mean.
    let global_mean: f64 = x.values().sum::<f64>() / x.len() as f64;
    for v in x.values_mut() {
        *v -= global_mean;
    }
    x
}
