//! Shared helpers for step-8 pack-linking tests.
//!
//! Shared helpers: not every test binary uses every helper, so dead-code
//! warnings are silenced here.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use s3a_pack_kite::{KiteContext, RenderFrame};

pub const MAX_OPEN_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_FRAME_PIXELS: u64 = 2_000_000;
pub const CSS_WIDTH: f64 = 800.0;
pub const CSS_HEIGHT: f64 = 600.0;
pub const DPR: f64 = 1.0;

const REQUIRED_UA_FONTS: &[&str] = &[
    "Noto Sans",
    "Noto Sans Italic",
    "Noto Serif",
    "Noto Sans Mono",
    "Noto Sans Symbols",
    "Noto Sans SC",
    "Noto Emoji",
];

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixture(rel: &str) -> PathBuf {
    repo_root().join("fixtures").join(rel)
}

pub fn load_bytes(rel: &str) -> Vec<u8> {
    let path = fixture(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

pub fn test_context() -> KiteContext {
    let fonts = REQUIRED_UA_FONTS
        .iter()
        .map(|name| ((*name).to_string(), format!("{name} fixture bytes").into_bytes()));
    KiteContext::new(fonts).expect("complete required UA font set")
}

/// A shared-mutable clock for step-8b tests (AT-205): the pack holds the
/// clock behind `Arc<dyn Clock>`, so the test advances the same instance.
#[derive(Clone)]
pub struct TestClock(pub std::sync::Arc<std::sync::atomic::AtomicI64>);

impl s3a_pack_kite::Clock for TestClock {
    fn unix_utc_secs(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl TestClock {
    pub fn advance(&self, secs: i64) {
        self.0
            .fetch_add(secs, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn test_context_with_clock(now: i64) -> (KiteContext, TestClock) {
    let clock = TestClock(std::sync::Arc::new(std::sync::atomic::AtomicI64::new(now)));
    let fonts = REQUIRED_UA_FONTS
        .iter()
        .map(|name| ((*name).to_string(), format!("{name} fixture bytes").into_bytes()));
    (
        KiteContext::new_with_clock(fonts, clock.clone()).expect("complete required UA font set"),
        clock,
    )
}

pub fn text_layer(frame: &RenderFrame) -> String {
    frame
        .text_runs
        .iter()
        .map(|run| run.text.as_str())
        .collect()
}

pub fn assert_pixmap_size(frame: &RenderFrame, css_w: f64, css_h: f64, dpr: f64) {
    let width = (css_w * dpr).ceil() as u32;
    let height = (css_h * dpr).ceil() as u32;
    assert_eq!(frame.width, width, "pixmap width");
    assert_eq!(frame.height, height, "pixmap height");
    assert_eq!(
        frame.rgba.len(),
        (width as usize) * (height as usize) * 4,
        "rgba length must be width*height*4"
    );
}

pub fn exists(rel: &str) -> bool {
    Path::new(&fixture(rel)).is_file()
}
