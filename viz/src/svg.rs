//! Minimal SVG document builder (only the primitives the plots need).

use std::fmt::Write;

use crate::style::Rgba;
use crate::vec2::V2;

pub struct Svg {
    width: f64,
    height: f64,
    body: String,
}

fn fmt_coord(v: f64) -> String {
    format!("{:.1}", v)
}

impl Svg {
    pub fn new(width: f64, height: f64) -> Svg {
        let mut svg = Svg {
            width,
            height,
            body: String::new(),
        };
        // White background (notebook export default).
        let _ = writeln!(
            svg.body,
            "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>",
            fmt_coord(width),
            fmt_coord(height)
        );
        svg
    }

    pub fn circle(&mut self, center: V2, r: f64, fill: Option<Rgba>, stroke: Option<(Rgba, f64)>) {
        let _ = write!(
            self.body,
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"",
            fmt_coord(center.x),
            fmt_coord(center.y),
            fmt_coord(r)
        );
        match fill {
            Some(c) => {
                let _ = write!(self.body, " fill=\"{}\" fill-opacity=\"{}\"", c.hex(), c.a);
            }
            None => {
                let _ = write!(self.body, " fill=\"none\"");
            }
        }
        if let Some((c, w)) = stroke {
            let _ = write!(
                self.body,
                " stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\"",
                c.hex(),
                c.a,
                fmt_coord(w)
            );
        }
        let _ = writeln!(self.body, "/>");
    }

    pub fn polygon(&mut self, points: &[V2], fill: Rgba) {
        if points.len() < 3 {
            return;
        }
        let pts: Vec<String> = points
            .iter()
            .map(|p| format!("{},{}", fmt_coord(p.x), fmt_coord(p.y)))
            .collect();
        let _ = writeln!(
            self.body,
            "<polygon points=\"{}\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"none\"/>",
            pts.join(" "),
            fill.hex(),
            fill.a
        );
    }

    pub fn polyline(&mut self, points: &[V2], stroke: Rgba, width: f64) {
        if points.len() < 2 {
            return;
        }
        let pts: Vec<String> = points
            .iter()
            .map(|p| format!("{},{}", fmt_coord(p.x), fmt_coord(p.y)))
            .collect();
        let _ = writeln!(
            self.body,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{}\" \
             stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            pts.join(" "),
            stroke.hex(),
            stroke.a,
            fmt_coord(width)
        );
    }

    pub fn text(&mut self, pos: V2, size: f64, content: &str, color: Rgba, font: &str) {
        let escaped = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let _ = writeln!(
            self.body,
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-family=\"{}\" fill=\"{}\" \
             text-anchor=\"middle\" dominant-baseline=\"central\">{}</text>",
            fmt_coord(pos.x),
            fmt_coord(pos.y),
            fmt_coord(size),
            font,
            color.hex(),
            escaped
        );
    }

    pub fn finish(self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
             viewBox=\"0 0 {w} {h}\">\n{body}</svg>\n",
            w = fmt_coord(self.width),
            h = fmt_coord(self.height),
            body = self.body
        )
    }
}

/// Maps layout (world, y-up) coordinates to SVG pixels (y-down).
#[derive(Clone, Copy)]
pub struct Frame {
    pub scale: f64,
    pub world_min: V2,
    pub world_max_y: f64,
}

impl Frame {
    pub fn to_px(self, p: V2) -> V2 {
        V2 {
            x: (p.x - self.world_min.x) * self.scale,
            y: (self.world_max_y - p.y) * self.scale,
        }
    }

    pub fn len_px(self, world_len: f64) -> f64 {
        world_len * self.scale
    }
}
