//! HypergraphPlot: assembles the figure exactly as SetReplace's
//! drawEmbedding does — hyperedge convex-hull polygons underneath, then
//! arrows (trimmed lines + exact arrowheads), then vertex disks, then unary
//! edge circles, then labels.

use std::collections::HashMap;

use setreplace::Atom;

use crate::geometry::{arrow, convex_hull, sample_loop, sample_qbezier};
use crate::layout::{layout_hypergraph, vertex_list};
use crate::style;
use crate::svg::{Frame, Svg};
use crate::vec2::{v2, BBox, V2};

pub struct HypergraphPlotOptions {
    /// Layout seed (deterministic per seed).
    pub seed: u64,
    /// Optional vertex labels.
    pub labels: Option<HashMap<Atom, String>>,
    /// Target plot width in printer's points; the README figures use 478.
    /// Rendered at 2x for retina-quality rasters.
    pub target_width_pt: f64,
}

impl Default for HypergraphPlotOptions {
    fn default() -> Self {
        HypergraphPlotOptions {
            seed: 0,
            labels: None,
            target_width_pt: 478.0,
        }
    }
}

/// Convenience: labels every vertex; atoms above `fresh_from` are rendered
/// as `v<atom>` (mirroring the generated-vertex names in the SetReplace
/// README), the rest as plain integers.
pub fn readme_style_labels(edges: &[Vec<Atom>], fresh_from: Atom) -> HashMap<Atom, String> {
    vertex_list(edges)
        .into_iter()
        .map(|a| {
            let label = if a >= fresh_from {
                format!("v{a}")
            } else {
                format!("{a}")
            };
            (a, label)
        })
        .collect()
}

struct DrawnEdge {
    /// Sampled polyline for each consecutive vertex pair.
    segments: Vec<Vec<V2>>,
    /// Convex-hull polygon (arity >= 3 only).
    polygon: Vec<V2>,
    /// Unary edge anchor (arity 1).
    unary_at: Option<V2>,
}

pub fn hypergraph_plot_svg(edges: &[Vec<Atom>], opts: &HypergraphPlotOptions) -> String {
    let layout = layout_hypergraph(edges, opts.seed);
    let pos = &layout.positions;

    // Plot range (larger coordinate extent) drives the arrowhead length.
    let mut vertex_bbox = BBox::empty();
    for p in pos.values() {
        vertex_bbox.include(*p);
    }
    let plot_range = if pos.is_empty() {
        0.0
    } else {
        vertex_bbox.width().max(vertex_bbox.height())
    };
    let arrowhead_len = style::arrowhead_length(plot_range);
    let vertex_size = style::VERTEX_SIZE;

    // Count parallel drawn segments (same unordered vertex pair) so they can
    // curve apart, as Mathematica's multigraph embedding does.
    let mut pair_counts: HashMap<(Atom, Atom), usize> = HashMap::new();
    for edge in edges {
        for w in edge.windows(2) {
            if w[0] != w[1] {
                let key = (w[0].min(w[1]), w[0].max(w[1]));
                *pair_counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    let mut pair_seen: HashMap<(Atom, Atom), usize> = HashMap::new();

    let mut drawn: Vec<DrawnEdge> = Vec::with_capacity(edges.len());
    for edge in edges {
        let mut segments: Vec<Vec<V2>> = Vec::new();
        if edge.len() == 1 {
            drawn.push(DrawnEdge {
                segments,
                polygon: Vec::new(),
                unary_at: Some(pos[&edge[0]]),
            });
            continue;
        }
        for w in edge.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (pa, pb) = (pos[&a], pos[&b]);
            if a == b {
                // Self-loop within a hyperedge.
                let out = loop_direction(a, pos, edges);
                segments.push(sample_loop(pa, out, 0.18, 32));
                continue;
            }
            let key = (a.min(b), a.max(b));
            let total = pair_counts[&key];
            let index = *pair_seen
                .entry(key)
                .and_modify(|i| *i += 1)
                .or_insert(0);
            // Parallel segments spread across symmetric bulge offsets along
            // a canonical perpendicular (atom order), so antiparallel pairs
            // separate too; a lone segment stays straight.
            let offset = 0.35 * (index as f64 - (total as f64 - 1.0) / 2.0);
            if offset.abs() < 1e-9 {
                segments.push(vec![pa, pb]);
            } else {
                let (lo, hi) = (pos[&a.min(b)], pos[&a.max(b)]);
                let axis = (hi - lo).perp().normalized();
                let control = pa.lerp(pb, 0.5) + axis * offset;
                segments.push(sample_qbezier(pa, control, pb, 24));
            }
        }
        // Hyperedge polygon: convex hull of all segment points (arity >= 3).
        let polygon = if edge.len() > 2 {
            let points: Vec<V2> = segments.iter().flatten().copied().collect();
            let hull = convex_hull(&points);
            if hull.len() >= 3 {
                hull
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        drawn.push(DrawnEdge {
            segments,
            polygon,
            unary_at: None,
        });
    }

    // Content bounding box: disks, segments, polygons, labels.
    let mut bbox = BBox::empty();
    for p in pos.values() {
        bbox.include(*p + v2(vertex_size, vertex_size));
        bbox.include(*p - v2(vertex_size, vertex_size));
    }
    for e in &drawn {
        for seg in &e.segments {
            for p in seg {
                bbox.include(*p);
            }
        }
        for p in &e.polygon {
            bbox.include(*p);
        }
    }

    // Label placement: radially away from the mean direction of neighbors.
    let font_size = 0.19;
    let mut label_items: Vec<(V2, String)> = Vec::new();
    if let Some(labels) = &opts.labels {
        let neighbor_dirs = neighbor_directions(edges, pos);
        for &a in &layout.vertices {
            let Some(text) = labels.get(&a) else { continue };
            let p = pos[&a];
            let dir = neighbor_dirs
                .get(&a)
                .copied()
                .filter(|d| d.norm() > 0.15)
                .map(|d| (-d).normalized())
                .unwrap_or_else(|| v2(0.30, -0.95).normalized());
            let width_allowance = 0.5 * font_size * text.chars().count() as f64;
            let anchor = p
                + dir * (vertex_size + 0.5 * font_size + 0.03)
                + dir * (width_allowance * dir.x.abs() * 0.8);
            label_items.push((anchor, text.clone()));
            // Reserve space in the bounding box.
            let half = v2(width_allowance + 0.02, font_size * 0.62);
            bbox.include(anchor + half);
            bbox.include(anchor - half);
        }
    }

    // Unary circle radii contribute to the bounds too.
    let mut unary_counts: HashMap<(i64, i64), f64> = HashMap::new();
    let mut unary_circles: Vec<(V2, f64)> = Vec::new();
    for e in &drawn {
        if let Some(p) = e.unary_at {
            let key = ((p.x * 1e6) as i64, (p.y * 1e6) as i64);
            let radius = unary_counts
                .entry(key)
                .and_modify(|r| *r += vertex_size)
                .or_insert(2.0 * vertex_size);
            let r = *radius;
            unary_circles.push((p, r));
            bbox.include(p + v2(r, r));
            bbox.include(p - v2(r, r));
        }
    }

    let bbox = bbox.pad(0.02 * plot_range.max(1.0) + 0.02);

    // Image sizing: fits in MAX_IMAGE_SIZE scaled by Min[1, 0.7 range] per
    // dimension (HypergraphPlot.m), then scaled up to the requested width.
    let sf_x = 1.0f64.min(0.7 * vertex_bbox.width());
    let sf_y = 1.0f64.min(0.7 * vertex_bbox.height());
    let box_w_pt = style::MAX_IMAGE_SIZE.0 * sf_x.max(0.2);
    let box_h_pt = style::MAX_IMAGE_SIZE.1 * sf_y.max(0.2);
    let pt_per_unit = (box_w_pt / bbox.width()).min(box_h_pt / bbox.height());
    let upscale = opts.target_width_pt / style::MAX_IMAGE_SIZE.0;
    let px_per_unit = pt_per_unit * upscale * 2.0; // 2x raster

    let frame = Frame {
        scale: px_per_unit,
        world_min: bbox.min,
        world_max_y: bbox.max.y,
    };
    let mut svg = Svg::new(
        bbox.width() * px_per_unit,
        bbox.height() * px_per_unit,
    );
    let stroke_px = 2.0; // 1 printer's point at 2x

    // 1. Hyperedge polygons (under everything).
    for e in &drawn {
        if !e.polygon.is_empty() {
            let pts: Vec<V2> = e.polygon.iter().map(|p| frame.to_px(*p)).collect();
            svg.polygon(&pts, style::EDGE_POLYGON);
        }
    }
    // 2. Edge lines with arrowheads.
    for e in &drawn {
        for seg in &e.segments {
            let (line, head) = arrow(seg, vertex_size, arrowhead_len);
            let line_px: Vec<V2> = line.iter().map(|p| frame.to_px(*p)).collect();
            svg.polyline(&line_px, style::EDGE_LINE, stroke_px);
            if head.len() >= 3 {
                let head_px: Vec<V2> = head.iter().map(|p| frame.to_px(*p)).collect();
                svg.polygon(&head_px, style::EDGE_LINE);
            }
        }
    }
    // 3. Vertex disks.
    for &a in &layout.vertices {
        svg.circle(
            frame.to_px(pos[&a]),
            frame.len_px(vertex_size),
            Some(style::VERTEX_FILL),
            Some((style::VERTEX_BORDER, stroke_px)),
        );
    }
    // 4. Unary edge circles.
    for (p, r) in &unary_circles {
        svg.circle(
            frame.to_px(*p),
            frame.len_px(*r),
            None,
            Some((style::EDGE_LINE, stroke_px)),
        );
    }
    // 5. Labels.
    for (p, text) in &label_items {
        svg.text(
            frame.to_px(*p),
            frame.len_px(font_size),
            text,
            style::LABEL_COLOR,
            style::LABEL_FONT,
        );
    }

    svg.finish()
}

/// Mean unit direction from each vertex towards its hyperedge neighbors;
/// labels go on the opposite side.
fn neighbor_directions(
    edges: &[Vec<Atom>],
    pos: &HashMap<Atom, V2>,
) -> HashMap<Atom, V2> {
    let mut sums: HashMap<Atom, V2> = HashMap::new();
    for edge in edges {
        for w in edge.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a == b {
                continue;
            }
            let dir = (pos[&b] - pos[&a]).normalized();
            *sums.entry(a).or_insert_with(|| v2(0.0, 0.0)) += dir;
            *sums.entry(b).or_insert_with(|| v2(0.0, 0.0)) += -dir;
        }
    }
    for sum in sums.values_mut() {
        *sum = sum.normalized();
    }
    sums
}

/// Outward direction for a self-loop: away from the centroid of all other
/// vertices.
fn loop_direction(at: Atom, pos: &HashMap<Atom, V2>, edges: &[Vec<Atom>]) -> V2 {
    let p = pos[&at];
    let mut centroid = v2(0.0, 0.0);
    let mut count = 0;
    for edge in edges {
        for &b in edge {
            if b != at {
                centroid += pos[&b];
                count += 1;
            }
        }
    }
    if count == 0 {
        return v2(0.0, 1.0);
    }
    let away = p - centroid * (1.0 / count as f64);
    if away.norm() < 1e-9 {
        v2(0.0, 1.0)
    } else {
        away.normalized()
    }
}
