//! # setreplace-viz
//!
//! Layout and rendering for [`setreplace`] hypergraphs, reproducing the
//! aesthetics of SetReplace's `HypergraphPlot` and `"LayeredCausalGraph"`:
//! spring-electrical embedding with cyclic hyperedge springs, convex-hull
//! polygons for ternary+ edges, the exact arrowhead geometry and light-theme
//! palette from the original's style definitions, and layered causal graphs
//! (orange events, dark-red causal edges).
//!
//! Output is SVG (no dependencies) or PNG (via resvg). No graphviz anywhere.

mod causal;
mod geometry;
mod layout;
mod pcg;
mod plot;
mod png;
pub mod style;
mod svg;
mod vec2;

pub use causal::{layered_causal_graph_svg, CausalGraphOptions};
pub use layout::{layout_hypergraph, layout_hypergraph_with, vertex_list, Layout, LayoutOptions};
pub use plot::{hypergraph_plot_svg, readme_style_labels, HypergraphPlotOptions};
pub use png::svg_to_png;
pub use vec2::V2;
