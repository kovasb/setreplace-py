//! Builds side-by-side comparison images: Wolfram's render (left) vs the
//! Rust engine's render (right), scaled to a common height.

use resvg::tiny_skia::{Pixmap, PixmapPaint, Transform};
use std::fs;
use std::path::Path;

fn load(path: &str) -> Pixmap {
    Pixmap::decode_png(&fs::read(path).unwrap_or_else(|_| panic!("missing {path}")))
        .expect("decode png")
}

fn compose(left: &Pixmap, right: &Pixmap, out: &Path) {
    let target_h = 560u32;
    let scale_l = target_h as f32 / left.height() as f32;
    let scale_r = target_h as f32 / right.height() as f32;
    let w_l = (left.width() as f32 * scale_l).ceil() as u32;
    let w_r = (right.width() as f32 * scale_r).ceil() as u32;
    let gutter = 24u32;
    let mut canvas = Pixmap::new(w_l + gutter + w_r, target_h).unwrap();
    canvas.fill(resvg::tiny_skia::Color::WHITE);
    canvas.draw_pixmap(
        0,
        0,
        left.as_ref(),
        &PixmapPaint::default(),
        Transform::from_scale(scale_l, scale_l),
        None,
    );
    // tiny-skia applies `transform` to the placement offset as well, so the
    // right image's offset is baked into the transform instead.
    // Divider line.
    let mut divider = Pixmap::new(2, target_h).unwrap();
    divider.fill(resvg::tiny_skia::Color::from_rgba8(200, 200, 200, 255));
    canvas.draw_pixmap(
        (w_l + gutter / 2) as i32,
        0,
        divider.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    canvas.draw_pixmap(
        0,
        0,
        right.as_ref(),
        &PixmapPaint::default(),
        Transform::from_scale(scale_r, scale_r).post_translate((w_l + gutter) as f32, 0.0),
        None,
    );
    canvas.save_png(out).unwrap();
    println!("wrote {}", out.display());
}

fn main() {
    let out_dir = Path::new("out/comparison");
    fs::create_dir_all(out_dir).unwrap();
    for (wl, mine, name) in [
        (
            "/tmp/wl_ref/basic.png",
            "out/basic_hypergraph_plot.png",
            "basic",
        ),
        (
            "/tmp/wl_ref/step1.png",
            "out/evolution_result_1_step.png",
            "step1",
        ),
        (
            "/tmp/wl_ref/step10.png",
            "out/evolution_result_10_steps.png",
            "step10",
        ),
        (
            "/tmp/wl_ref/step100.png",
            "out/evolution_result_100_steps.png",
            "step100",
        ),
        (
            "/tmp/wl_ref/causal10.png",
            "out/layered_causal_graph.png",
            "causal10",
        ),
    ] {
        compose(
            &load(wl),
            &load(mine),
            &out_dir.join(format!("{name}_wolfram_vs_rust.png")),
        );
    }
}
