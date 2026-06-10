//! SetReplace's exact light-theme style constants, transcribed from
//! Kernel/A0$style.m. Colors are Hue[h, s, b] (HSV) converted to sRGB.

#[derive(Debug, Clone, Copy)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Rgba {
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

const fn rgba(r: u8, g: u8, b: u8, a: f64) -> Rgba {
    Rgba { r, g, b, a }
}

// --- HypergraphPlot ($vertexStyle / $edgeLineStyle / $edgePolygonStyle) ----

/// Hue[0.63, 0.26, 0.89] — vertex disk fill.
pub const VERTEX_FILL: Rgba = rgba(168, 181, 227, 1.0);
/// Hue[0.63, 0.7, 0.33] at opacity 0.95 — vertex disk border.
pub const VERTEX_BORDER: Rgba = rgba(25, 38, 84, 0.95);
/// Hue[0.63, 0.7, 0.5] at opacity 0.7 — edge lines, arrowheads, unary circles.
pub const EDGE_LINE: Rgba = rgba(38, 58, 128, 0.7);
/// Hue[0.63, 0.66, 0.81] at opacity 0.1 — hyperedge polygons (no border).
pub const EDGE_POLYGON: Rgba = rgba(70, 100, 207, 0.1);

/// $vertexSize (disk radius, in units of mean drawn-edge length).
pub const VERTEX_SIZE: f64 = 0.06;

/// $arrowheadLengthFunction: Max[0.1, Min[0.185, 0.066 + 0.017 plotRange]],
/// where plotRange is the larger extent of the vertex coordinates.
pub fn arrowhead_length(plot_range: f64) -> f64 {
    (0.066 + 0.017 * plot_range).clamp(0.1, 0.185)
}

/// $edgeArrowheadShape — exact polygon, tip at (0, 0), pointing +x, unit
/// length scale (extends to x ≈ -1.102).
pub const ARROWHEAD_SHAPE: [(f64, f64); 15] = [
    (-1.10196, -0.289756),
    (-1.08585, -0.257073),
    (-1.05025, -0.178048),
    (-1.03171, -0.130243),
    (-1.01512, -0.0824391),
    (-1.0039, -0.037561),
    (-1.0, 0.0),
    (-1.0039, 0.0341466),
    (-1.01512, 0.0780486),
    (-1.03171, 0.127805),
    (-1.05025, 0.178538),
    (-1.08585, 0.264878),
    (-1.10196, 0.301464),
    (0.0, 0.0),
    (-1.10196, -0.289756),
];

/// $hypergraphPlotImageSize -> {{360}, {420}} (max width/height in printer's
/// points), scaled per dimension by Min[1, 0.7 * plotRangeExtent].
pub const MAX_IMAGE_SIZE: (f64, f64) = (360.0, 420.0);

// --- Causal / token-event graphs ($eventVertexStyle etc.) ------------------

/// Hue[0.11, 1, 0.97] — event vertices (causal graph).
pub const EVENT_VERTEX: Rgba = rgba(247, 163, 0, 1.0);
/// RGBColor[0.259, 0.576, 1] — the initial boundary event vertex.
pub const INITIAL_EVENT_VERTEX: Rgba = rgba(66, 147, 255, 1.0);
/// Hue[0, 1, 0.56] — causal edges.
pub const CAUSAL_EDGE: Rgba = rgba(143, 0, 0, 1.0);
/// $tokenVertexStyle fill: Hue[0.63, 0.66, 0.81] at opacity 0.1,
/// border Hue[0.63, 0.7, 0.5] at 0.7 (token vertices in token-event graphs).
pub const TOKEN_VERTEX_FILL: Rgba = rgba(70, 100, 207, 0.1);
pub const TOKEN_VERTEX_BORDER: Rgba = rgba(38, 58, 128, 0.7);

/// Label text color (plain black, Mathematica default).
pub const LABEL_COLOR: Rgba = rgba(0, 0, 0, 1.0);
pub const LABEL_FONT: &str =
    "'Source Sans Pro', 'Source Sans 3', 'Helvetica Neue', Helvetica, Arial, sans-serif";
