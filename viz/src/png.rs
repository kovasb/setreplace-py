//! SVG → PNG rasterization via resvg (pure Rust).

use std::path::Path;

use resvg::tiny_skia;
use resvg::usvg;

pub fn svg_to_png(svg: &str, path: &Path) -> Result<(), String> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| "empty pixmap".to_string())?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.save_png(path).map_err(|e| e.to_string())
}
