use crate::open::Handle;
use crate::types::{FirstView, Floor, Kind, ManifestState};
use crate::KiteError;

/// Packed RGBA frame plus text and semantics (§5.8).
#[derive(Debug, Clone)]
pub struct RenderFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub text_runs: Vec<TextRun>,
    pub semantics: Vec<SemanticNode>,
}

#[derive(Debug, Clone)]
pub struct TextRun {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct SemanticNode {
    pub role: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub href: Option<String>,
    pub paper_id: Option<String>,
    pub floor: Option<u32>,
    pub opacity: Option<f64>,
}

pub fn render_manifest(
    handle: &Handle,
    width: f64,
    height: f64,
    scroll_y: f64,
    device_pixel_ratio: f64,
) -> Result<RenderFrame, KiteError> {
    let _ = scroll_y;
    let (pix_w, pix_h) = pixmap_size(handle, width, height, device_pixel_ratio)?;
    let mut runs = Vec::new();
    let mut semantics = Vec::new();
    let mut y = 16i32;
    match handle.manifest() {
        ManifestState::Text(text) => push_lines(&mut runs, &text, &mut y),
        ManifestState::Absent => push_lines(&mut runs, "manifest absent", &mut y),
        ManifestState::NotUtf8 => push_lines(&mut runs, "manifest not UTF-8", &mut y),
        ManifestState::Unsupported { banner } => push_lines(&mut runs, &banner, &mut y),
    }
    if handle.kind_enum() != Kind::Unsupported {
        // Continue control, away from (10, 10) so a point on empty space is None.
        semantics.push(SemanticNode {
            role: "continue".into(),
            label: "Continue".into(),
            x: 40,
            y: (height as i32).saturating_sub(48),
            width: 120,
            height: 32,
            href: None,
            paper_id: None,
            floor: None,
            opacity: None,
        });
        push_run(
            &mut runs,
            "Continue",
            40,
            (height as i32).saturating_sub(48),
            120,
            32,
        );
    }
    let frame = finish_frame(pix_w, pix_h, runs, semantics);
    handle.store_frame("manifest", None, frame.clone());
    if handle.first_view_enum() == FirstView::Header {
        handle.store_frame("header", None, frame.clone());
    }
    if handle.kind_enum() == Kind::Unsupported || handle.first_view_enum() == FirstView::Banner {
        handle.store_frame("banner", None, frame.clone());
    }
    Ok(frame)
}

pub fn render_warehouse(
    handle: &Handle,
    width: f64,
    height: f64,
    scroll_x: f64,
    scroll_y: f64,
    device_pixel_ratio: f64,
) -> Result<RenderFrame, KiteError> {
    let _ = (scroll_x, scroll_y);
    if handle.kind_enum() == Kind::Unsupported {
        return Err(KiteError::new(
            "unsupported",
            "render_warehouse is not defined for kind unsupported",
        ));
    }
    if !handle.proceeded() {
        return Err(KiteError::new(
            "view_mismatch",
            "warehouse is not the current view until proceed_to_warehouse",
        ));
    }
    let (pix_w, pix_h) = pixmap_size(handle, width, height, device_pixel_ratio)?;
    let floors = handle.floors();
    let mut runs = Vec::new();
    let mut semantics = Vec::new();
    let mut y = 16i32;
    push_lines(&mut runs, "warehouse", &mut y);
    for floor in &floors {
        let line = format!(
            "index {} type_id {} record_count {} size_bytes {} payload_hash_short {}",
            floor.index,
            floor.type_id,
            floor.record_count,
            floor.size_bytes,
            floor.payload_hash_short
        );
        let line_y = y;
        push_lines(&mut runs, &line, &mut y);
        semantics.push(floor_node(floor, 16, line_y));
    }
    let frame = finish_frame(pix_w, pix_h, runs, semantics);
    handle.store_frame("warehouse", None, frame.clone());
    Ok(frame)
}

fn floor_node(floor: &Floor, x: i32, y: i32) -> SemanticNode {
    SemanticNode {
        role: "floor".into(),
        label: format!("floor {}", floor.index),
        x,
        y,
        width: 640,
        height: 20,
        href: None,
        paper_id: None,
        floor: Some(floor.index),
        opacity: None,
    }
}

fn pixmap_size(
    handle: &Handle,
    width: f64,
    height: f64,
    dpr: f64,
) -> Result<(u32, u32), KiteError> {
    if !dpr.is_finite()
        || dpr <= 0.0
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return Err(KiteError::new(
            "invalid_param",
            "width, height, and device_pixel_ratio must be finite and > 0",
        ));
    }
    let pix_w = (width * dpr).ceil() as u32;
    let pix_h = (height * dpr).ceil() as u32;
    let product = (pix_w as u64).saturating_mul(pix_h as u64);
    if product > handle.max_frame_pixels() {
        return Err(KiteError::new(
            "frame_too_large",
            "pixmap width*height exceeds max_frame_pixels",
        ));
    }
    Ok((pix_w, pix_h))
}

fn push_lines(runs: &mut Vec<TextRun>, text: &str, y: &mut i32) {
    for line in text.split('\n') {
        let w = (line.len() as u32).saturating_mul(8).max(8);
        push_run(runs, line, 16, *y, w, 16);
        *y = y.saturating_add(20);
    }
}

fn push_run(runs: &mut Vec<TextRun>, text: &str, x: i32, y: i32, width: u32, height: u32) {
    runs.push(TextRun {
        text: text.to_string(),
        x,
        y,
        width,
        height,
    });
}

fn finish_frame(
    width: u32,
    height: u32,
    text_runs: Vec<TextRun>,
    semantics: Vec<SemanticNode>,
) -> RenderFrame {
    let rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    RenderFrame {
        rgba,
        width,
        height,
        text_runs,
        semantics,
    }
}

pub(crate) fn cache_frame(
    cache: &mut Vec<(String, Option<u32>, RenderFrame)>,
    view: &str,
    floor: Option<u32>,
    frame: RenderFrame,
) {
    if let Some(existing) = cache.iter_mut().find(|(v, f, _)| v == view && *f == floor) {
        existing.2 = frame;
    } else {
        cache.push((view.to_string(), floor, frame));
    }
}

pub(crate) fn last_frame(
    cache: &[(String, Option<u32>, RenderFrame)],
    view: &str,
    floor: Option<u32>,
) -> Option<RenderFrame> {
    cache
        .iter()
        .find(|(v, f, _)| v == view && *f == floor)
        .map(|(_, _, frame)| frame.clone())
}
