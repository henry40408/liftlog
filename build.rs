use std::path::Path;
use std::process::Command;

use resvg::{tiny_skia, usvg};

const APPLE_TOUCH_ICON_SIZE: u32 = 180;
const FAVICON_SVG_PATH: &str = "assets/favicon.svg";

fn main() {
    // Re-run build script when git state changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    let git_version = get_git_version();
    println!("cargo:rustc-env=GIT_VERSION={}", git_version);

    render_apple_touch_icon();
}

fn get_git_version() -> String {
    // Tier 1: Use environment variable (for Docker/CI builds)
    if let Ok(version) = std::env::var("GIT_VERSION") {
        if !version.is_empty() && version != "dev" {
            return version;
        }
    }

    // Tier 2: Use git describe
    Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "dev".to_string()) // Tier 3: fallback to "dev"
}

fn render_apple_touch_icon() {
    println!("cargo:rerun-if-changed={}", FAVICON_SVG_PATH);

    let svg = std::fs::read_to_string(FAVICON_SVG_PATH)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", FAVICON_SVG_PATH, e));

    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", FAVICON_SVG_PATH, e));

    let mut pixmap = tiny_skia::Pixmap::new(APPLE_TOUCH_ICON_SIZE, APPLE_TOUCH_ICON_SIZE)
        .expect("failed to allocate pixmap");

    let scale = APPLE_TOUCH_ICON_SIZE as f32 / tree.size().width();
    let transform = tiny_skia::Transform::from_scale(scale, scale);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir).join("apple-touch-icon.png");
    pixmap
        .save_png(&out_path)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));
}
