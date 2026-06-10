//! Geometric helpers mirroring SetReplace's arrow.m (polyline trimming and
//! arrowhead placement) plus convex hulls and Bézier sampling.

use crate::style::ARROWHEAD_SHAPE;
use crate::vec2::{v2, V2};

/// Drops `length` of arclength from the front of a polyline
/// (arrow.m `lineDrop`).
fn line_drop(pts: &[V2], length: f64) -> Vec<V2> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    if pts.len() > 2 {
        let first_segment = pts[0].dist(pts[1]);
        if first_segment <= length {
            return line_drop(&pts[1..], length - first_segment);
        }
        let mut head = line_drop(&pts[..2], length);
        head.extend_from_slice(&pts[2..]);
        return head;
    }
    let dir = (pts[1] - pts[0]).normalized();
    vec![pts[0] + dir * length, pts[1]]
}

/// arrow.m `lineTake[pts, start ;; -end]`: trims `start` arclength from the
/// beginning and `end` from the end.
pub fn line_take(pts: &[V2], start: f64, end: f64) -> Vec<V2> {
    let dropped = line_drop(pts, start);
    let mut reversed: Vec<V2> = dropped.into_iter().rev().collect();
    reversed = line_drop(&reversed, end);
    reversed.into_iter().rev().collect()
}

/// The exact $edgeArrowheadShape polygon, scaled by `length`, rotated to
/// `direction`, translated to `tip`.
pub fn arrowhead_polygon(tip: V2, direction: V2, length: f64) -> Vec<V2> {
    let dir = direction.normalized();
    if dir.norm() < 0.5 {
        return Vec::new();
    }
    ARROWHEAD_SHAPE
        .iter()
        .map(|&(x, y)| v2(x * length, y * length).rotate_to(dir) + tip)
        .collect()
}

/// An arrow as drawn by arrow.m: the polyline trimmed by `vertex_size` at
/// both ends, with the line further shortened by `arrowhead_length` and the
/// arrowhead at the trimmed end. Returns (line, arrowhead).
pub fn arrow(pts: &[V2], vertex_size: f64, arrowhead_length: f64) -> (Vec<V2>, Vec<V2>) {
    let to_arrow_end = line_take(pts, vertex_size, vertex_size);
    let to_line_end = line_take(&to_arrow_end, 0.0, arrowhead_length);
    let head = if to_arrow_end.len() > 1 && arrowhead_length > 0.0 {
        let tip = *to_arrow_end.last().unwrap();
        let back = *to_line_end.last().unwrap();
        arrowhead_polygon(tip, tip - back, arrowhead_length)
    } else {
        Vec::new()
    };
    (to_line_end, head)
}

/// Convex hull (Andrew's monotone chain, lower + upper pass). Returns
/// vertices in counterclockwise order; fewer than 3 points if degenerate.
pub fn convex_hull(points: &[V2]) -> Vec<V2> {
    let mut pts: Vec<V2> = points.to_vec();
    pts.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.y.partial_cmp(&b.y).unwrap()));
    pts.dedup_by(|a, b| a.dist(*b) < 1e-9);
    let n = pts.len();
    if n < 3 {
        return pts;
    }
    let turns_right = |o: V2, a: V2, b: V2| (a - o).cross(b - o) <= 1e-12;
    let mut lower: Vec<V2> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && turns_right(lower[lower.len() - 2], lower[lower.len() - 1], p) {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<V2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && turns_right(upper[upper.len() - 2], upper[upper.len() - 1], p) {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Samples a quadratic Bézier (start, control, end) as a polyline.
pub fn sample_qbezier(start: V2, control: V2, end: V2, samples: usize) -> Vec<V2> {
    (0..=samples)
        .map(|i| {
            let t = i as f64 / samples as f64;
            let a = start.lerp(control, t);
            let b = control.lerp(end, t);
            a.lerp(b, t)
        })
        .collect()
}

/// Samples a circular loop anchored at `at`, bulging in direction `out`
/// (for self-loop edges). Starts and ends at `at`.
pub fn sample_loop(at: V2, out: V2, radius: f64, samples: usize) -> Vec<V2> {
    let dir = out.normalized();
    let center = at + dir * radius;
    let start_angle = (at - center).y.atan2((at - center).x);
    (0..=samples)
        .map(|i| {
            let t = i as f64 / samples as f64;
            let angle = start_angle + std::f64::consts::TAU * t;
            center + v2(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_take_trims_both_ends() {
        let pts = vec![v2(0.0, 0.0), v2(10.0, 0.0)];
        let trimmed = line_take(&pts, 1.0, 2.0);
        assert!((trimmed[0].x - 1.0).abs() < 1e-9);
        assert!((trimmed[1].x - 8.0).abs() < 1e-9);
    }

    #[test]
    fn hull_of_triangle() {
        let hull = convex_hull(&[v2(0.0, 0.0), v2(1.0, 0.0), v2(0.0, 1.0), v2(0.2, 0.2)]);
        assert_eq!(hull.len(), 3);
    }
}
