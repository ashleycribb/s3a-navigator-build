//! Renderers: first view, warehouse, floor detail, floor transition (§5.8).
//!
//! Text is painted as text (never executed as HTML/JS). BLOB and PROVENANCE
//! floors show metadata only, never raw payload bytes (AT-107). Floors whose
//! layout kite cannot decode show metadata only. Withheld payloads do not
//! appear in `text_runs`, `semantics`, pixels, or hit-test targets (DD-125).
//!
//! Pixel note (§5.8): this renderer paints no text into `rgba` (all-zero
//! pixmap); real pixel paint is step 8c. The §5.8 render-model control check
//! is therefore vacuous until 8c: fixture pixmap == control pixmap because
//! neither paints text. The meaningful withheld-absence checks (text_runs,
//! semantics, hit-test targets) are real and asserted.

use s3a_core::consent::{ConsentPurpose, ConsentRecord};
use s3a_core::{MarkRecord, TileType};
use s3a_edge::{
    DeviceIdentityRecord, EdgeConsentRecord, EquipmentEventRecord, TILE_TYPE_CONSENT,
    TILE_TYPE_RECORD, TILE_TYPE_SEAL,
};
use s3a_engine::MmapReader;

use crate::consent::ConsentEval;
use crate::open::Handle;
use crate::types::{Floor, Kind, ManifestState};
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

/// First view: manifest / derived header sheet / banner, with the quarantine
/// banner and the per-file `withheld_count` (§5.3 item 4).
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

    if handle.kind_enum() == Kind::Unsupported {
        if let ManifestState::Unsupported { banner } = handle.manifest() {
            push_lines(&mut runs, &banner, &mut y);
        }
    } else {
        let eval = handle.consent_eval()?;
        let report = handle.report();
        match handle.kind_enum() {
            Kind::HyperTile => match handle.manifest() {
                ManifestState::Text(text) => push_lines(&mut runs, &text, &mut y),
                ManifestState::Absent => push_lines(&mut runs, "manifest absent", &mut y),
                ManifestState::NotUtf8 => {
                    push_lines(&mut runs, "manifest not UTF-8", &mut y)
                }
                ManifestState::Unsupported { banner } => push_lines(&mut runs, &banner, &mut y),
            },
            Kind::Edge => {
                push_lines(&mut runs, "derived header sheet", &mut y);
                if let Some(info) = handle.edge_header() {
                    push_lines(
                        &mut runs,
                        &format!("stride {}", info.stride),
                        &mut y,
                    );
                    push_lines(
                        &mut runs,
                        &format!("committed tile_count {}", info.tile_count),
                        &mut y,
                    );
                    push_lines(
                        &mut runs,
                        &format!(
                            "file_uuid {:016x}{:016x}",
                            info.file_uuid[0], info.file_uuid[1]
                        ),
                        &mut y,
                    );
                    push_lines(
                        &mut runs,
                        &format!("generation {}", info.generation),
                        &mut y,
                    );
                    push_lines(
                        &mut runs,
                        &format!("created {}", rfc3339_secs(info.created_timestamp)),
                        &mut y,
                    );
                    match &info.device {
                        Some(d) => {
                            push_lines(
                                &mut runs,
                                &format!("device_id {}", hex_bytes(&d.device_id)),
                                &mut y,
                            );
                            push_lines(
                                &mut runs,
                                &format!("hardware_model {}", d.hardware_model),
                                &mut y,
                            );
                            push_lines(
                                &mut runs,
                                &format!(
                                    "firmware_version {}.{}.{}.{}",
                                    d.firmware_version[0],
                                    d.firmware_version[1],
                                    d.firmware_version[2],
                                    d.firmware_version[3]
                                ),
                                &mut y,
                            );
                        }
                        None => push_lines(
                            &mut runs,
                            "device identity record absent",
                            &mut y,
                        ),
                    }
                }
            }
            Kind::Unsupported => {}
        }
        // Verification label, quarantine banner, invalid list, withheld count.
        push_lines(
            &mut runs,
            &format!("status: {} seal: {}", report.status, report.seal_status),
            &mut y,
        );
        if let Some(reason) = &report.quarantine_reason {
            push_lines(
                &mut runs,
                &format!("quarantine: {} ({})", reason, quarantine_plain(reason)),
                &mut y,
            );
        }
        if !report.invalid_tile_indices.is_empty() {
            let list: Vec<String> = report
                .invalid_tile_indices
                .iter()
                .map(|i| i.to_string())
                .collect();
            push_lines(&mut runs, &format!("invalid tiles: {}", list.join(", ")), &mut y);
        }
        if let Some(b) = report.break_index {
            push_lines(&mut runs, &format!("break_index: {b}"), &mut y);
        }
        if let Some(tail) = handle.derived().unsigned_tail_list() {
            if !tail.is_empty() {
                let list: Vec<String> = tail.iter().map(|i| i.to_string()).collect();
                push_lines(
                    &mut runs,
                    &format!("unsigned tail: {}", list.join(", ")),
                    &mut y,
                );
            }
        }
        push_lines(
            &mut runs,
            &format!("withheld_count: {}", eval.withheld_file),
            &mut y,
        );
        semantics.push(SemanticNode {
            role: "heading".into(),
            label: "manifest".into(),
            x: 16,
            y: 16,
            width: 200,
            height: 20,
            href: None,
            paper_id: None,
            floor: None,
            opacity: None,
        });
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
    // First-view aliases share one normalized frame key ("first", §5.1).
    handle.store_frame("first", None, frame.clone());
    handle.store_frame("manifest", None, frame.clone());
    if handle.first_view_enum() == crate::types::FirstView::Header {
        handle.store_frame("header", None, frame.clone());
    }
    if handle.kind_enum() == Kind::Unsupported
        || handle.first_view_enum() == crate::types::FirstView::Banner
    {
        handle.store_frame("banner", None, frame.clone());
    }
    Ok(frame)
}

/// Warehouse floor grid. Floor bays are square at reference scale (§5.8):
/// side 32 (4 KiB), 45 (8 KiB), 181 (128 KiB).
pub fn render_warehouse(
    handle: &Handle,
    width: f64,
    height: f64,
    scroll_x: f64,
    scroll_y: f64,
    device_pixel_ratio: f64,
) -> Result<RenderFrame, KiteError> {
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
        push_lines(&mut runs, &line, &mut y);
        if let Some((nx, ny, nw, nh)) =
            warehouse_floor_rect(&floors, floor.index, width, height, scroll_x, scroll_y)
        {
            semantics.push(floor_node(floor, nx, ny, nw, nh));
        }
    }
    let frame = finish_frame(pix_w, pix_h, runs, semantics);
    handle.store_frame("warehouse", None, frame.clone());
    Ok(frame)
}

/// Floor N records, including the per-floor `withheld_count` (§5.3 item 4).
/// `floor_index` is the tile index, not an ordinal among valid floors (DD-82).
pub fn render_floor_detail(
    handle: &Handle,
    floor_index: u32,
    width: f64,
    height: f64,
    scroll_y: f64,
    device_pixel_ratio: f64,
) -> Result<RenderFrame, KiteError> {
    let _ = scroll_y;
    if handle.kind_enum() == Kind::Unsupported {
        return Err(KiteError::new(
            "unsupported",
            "render_floor_detail is not defined for kind unsupported",
        ));
    }
    if !handle.proceeded() {
        return Err(KiteError::new(
            "view_mismatch",
            "floor-detail is not the current view until proceed_to_warehouse",
        ));
    }
    if !handle.is_valid_floor(floor_index) {
        return Err(KiteError::new(
            "invalid_floor",
            format!("floor {floor_index} is not a valid floor"),
        ));
    }
    let (pix_w, pix_h) = pixmap_size(handle, width, height, device_pixel_ratio)?;
    let (lines, mut semantics) = floor_detail_content(handle, floor_index)?;
    let mut runs = Vec::new();
    let mut y = 16i32;
    push_lines(&mut runs, &format!("floor {floor_index}"), &mut y);
    for line in &lines {
        push_lines(&mut runs, line, &mut y);
    }
    semantics.insert(
        0,
        SemanticNode {
            role: "heading".into(),
            label: format!("floor {floor_index}"),
            x: 16,
            y: 16,
            width: 200,
            height: 20,
            href: None,
            paper_id: None,
            floor: Some(floor_index),
            opacity: None,
        },
    );
    let frame = finish_frame(pix_w, pix_h, runs, semantics);
    handle.store_frame("floor-detail", Some(floor_index), frame.clone());
    Ok(frame)
}

/// Floor-to-detail animation (§5.8). Start rect is the warehouse `floor`
/// node rect rendered with the same params; end rect is
/// (0.06w, 0.08h, 0.88w, 0.84h); opacity 0.2 -> 1 along CSS
/// cubic-bezier(.2,.8,.2,1) at t/0.45. `instant` returns only the end frame.
#[allow(clippy::too_many_arguments)]
pub fn render_floor_transition(
    handle: &Handle,
    floor_index: u32,
    t_seconds: f64,
    width: f64,
    height: f64,
    scroll_x: f64,
    scroll_y: f64,
    instant: bool,
    device_pixel_ratio: f64,
) -> Result<RenderFrame, KiteError> {
    if handle.kind_enum() == Kind::Unsupported {
        return Err(KiteError::new(
            "unsupported",
            "render_floor_transition is not defined for kind unsupported",
        ));
    }
    if !handle.proceeded() {
        return Err(KiteError::new(
            "view_mismatch",
            "floor-transition is not the current view until proceed_to_warehouse",
        ));
    }
    if !handle.is_valid_floor(floor_index) {
        return Err(KiteError::new(
            "invalid_floor",
            format!("floor {floor_index} is not a valid floor"),
        ));
    }
    let (pix_w, pix_h) = pixmap_size(handle, width, height, device_pixel_ratio)?;
    let floors = handle.floors();
    let side = floors
        .iter()
        .find(|f| f.index == floor_index)
        .map(|f| bay_side(f.size_bytes))
        .unwrap_or(32);
    let (sx, sy, sw, sh) = warehouse_floor_rect(&floors, floor_index, width, height, scroll_x, scroll_y)
        .unwrap_or((16, 16, side as u32, side as u32));
    let (ex, ey, ew, eh) = (
        (0.06 * width).round() as i32,
        (0.08 * height).round() as i32,
        (0.88 * width).round() as u32,
        (0.84 * height).round() as u32,
    );
    let progress = if instant {
        1.0
    } else {
        let t = t_seconds.clamp(0.0, 0.45);
        cubic_bezier_y(t / 0.45)
    };
    let opacity = 0.2 + 0.8 * progress;
    let lerp = |a: i32, b: i32| (a as f64 + (b as f64 - a as f64) * progress).round() as i32;
    let lerp_u = |a: u32, b: u32| (a as f64 + (b as f64 - a as f64) * progress).round() as u32;
    let card = SemanticNode {
        role: "card".into(),
        label: format!("floor {floor_index}"),
        x: lerp(sx, ex),
        y: lerp(sy, ey),
        width: lerp_u(sw, ew),
        height: lerp_u(sh, eh),
        href: None,
        paper_id: None,
        floor: Some(floor_index),
        opacity: Some(opacity),
    };
    // The card shows floor-detail content (same text layer), so withheld
    // payloads stay absent here too (AT-11, AT-23).
    let (lines, mut semantics) = floor_detail_content(handle, floor_index)?;
    let mut runs = Vec::new();
    let mut y = 16i32;
    push_lines(&mut runs, &format!("floor {floor_index}"), &mut y);
    for line in &lines {
        push_lines(&mut runs, line, &mut y);
    }
    semantics.push(card);
    let frame = finish_frame(pix_w, pix_h, runs, semantics);
    handle.store_frame("floor-transition", Some(floor_index), frame.clone());
    Ok(frame)
}

/// Floor-detail text lines plus extra semantic nodes (severity badges).
/// Withheld payloads contribute nothing (DD-125).
fn floor_detail_content(
    handle: &Handle,
    floor_index: u32,
) -> Result<(Vec<String>, Vec<SemanticNode>), KiteError> {
    let reader = handle.reader()?;
    let eval = handle.consent_eval()?;
    let floor = handle
        .floors()
        .into_iter()
        .find(|f| f.index == floor_index)
        .ok_or_else(|| KiteError::new("invalid_floor", format!("floor {floor_index} not found")))?;
    let mut lines: Vec<String> = Vec::new();
    let mut nodes: Vec<SemanticNode> = Vec::new();
    let withheld_here = eval.withheld_floor.get(&floor_index).copied().unwrap_or(0);
    lines.push(format!("withheld_count: {withheld_here}"));

    if reader.is_edge_archive() {
        edge_floor_content(&reader, &floor, &eval, &mut lines, &mut nodes)?;
    } else {
        hyper_floor_content(&reader, &floor, &eval, &mut lines)?;
    }
    Ok((lines, nodes))
}

fn hyper_floor_content(
    reader: &MmapReader,
    floor: &Floor,
    eval: &ConsentEval,
    lines: &mut Vec<String>,
) -> Result<(), KiteError> {
    let Ok((header, payload)) = reader.try_get_tile_unverified(floor.index as usize) else {
        lines.push(metadata_line(floor));
        return Ok(());
    };
    match header.tile_type {
        TileType::CONSENT => {
            let sz = std::mem::size_of::<ConsentRecord>();
            let n = payload.len() / sz;
            for r in 0..n {
                let rec: ConsentRecord =
                    bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                if eval.consent_record_shown(floor.index, r as u32, &rec.subject_id) {
                    lines.push(format!(
                        "consent subject_id {} purpose {} granted_at {} expires_at {} withdrawn_at {}",
                        hex_bytes(&rec.subject_id),
                        purpose_name(rec.purpose),
                        rec.granted_at,
                        rec.expires_at,
                        rec.withdrawn_at
                    ));
                }
            }
        }
        TileType::MANIFEST => {
            let len = (header.payload_bytes as usize).min(payload.len());
            match std::str::from_utf8(&payload[..len]) {
                Ok(s) => {
                    for line in s.split('\n') {
                        lines.push(line.to_string());
                    }
                }
                Err(_) => lines.push("manifest not UTF-8".to_string()),
            }
        }
        TileType::MARKS => {
            let sz = std::mem::size_of::<MarkRecord>();
            let n = payload.len() / sz;
            for r in 0..n {
                let m: MarkRecord =
                    bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                lines.push(format!(
                    "mark target_type {} target_tile_id {} record_index {} kind {} set {}",
                    m.target_tile_type, m.target_tile_id, m.record_index, m.mark_kind, m.set_or_clear
                ));
            }
        }
        TileType::PROVENANCE => {
            lines.push(metadata_line(floor));
            // PROVENANCE text is shown; raw captured bodies are not (the
            // payload here is the provenance document, not a BLOB body).
            if let Ok(s) = std::str::from_utf8(payload) {
                for line in s.split('\n') {
                    if !line.is_empty() {
                        lines.push(line.to_string());
                    }
                }
            }
        }
        _ => {
            // BLOB and every floor whose layout kite cannot decode: metadata
            // only, never raw payload bytes (AT-107).
            lines.push(metadata_line(floor));
        }
    }
    Ok(())
}

fn edge_floor_content(
    reader: &MmapReader,
    floor: &Floor,
    eval: &ConsentEval,
    lines: &mut Vec<String>,
    nodes: &mut Vec<SemanticNode>,
) -> Result<(), KiteError> {
    let Ok((header, payload)) = reader.try_get_edge_tile_unverified(floor.index as usize) else {
        lines.push(metadata_line(floor));
        return Ok(());
    };
    let type_id = header.type_id;
    match type_id {
        TILE_TYPE_RECORD => {
            let record_count = header.record_count;
            let rec_size = if record_count > 0 {
                payload.len() / record_count as usize
            } else {
                0
            };
            if rec_size == 32 && record_count > 0 {
                if eval.edge_shown {
                    let sz = std::mem::size_of::<EquipmentEventRecord>();
                    let mut y = 16i32;
                    for r in 0..record_count as usize {
                        let rec: EquipmentEventRecord = bytemuck::pod_read_unaligned(
                            &payload[r * sz..(r + 1) * sz],
                        );
                        lines.push(format!(
                            "event {} {} severity {} category {} operator {}",
                            rfc3339_secs(rec.timestamp_sec),
                            event_name(rec.event_type),
                            rec.severity,
                            event_category(rec.event_type),
                            hex_bytes(&rec.operator_id),
                        ));
                        nodes.push(SemanticNode {
                            role: "label".into(),
                            label: format!("severity: {}", rec.severity),
                            x: 16,
                            y,
                            width: 24,
                            height: 16,
                            href: None,
                            paper_id: None,
                            floor: Some(floor.index),
                            opacity: None,
                        });
                        y += 20;
                    }
                }
                // Withheld event-log payloads render no rows (DD-125).
            } else if rec_size == 96 && record_count > 0 {
                // DeviceIdentityRecord floors are exempt from withholding.
                let sz = std::mem::size_of::<DeviceIdentityRecord>();
                for r in 0..record_count as usize {
                    let rec: DeviceIdentityRecord =
                        bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                    let model = rec
                        .hardware_model
                        .iter()
                        .take_while(|b| **b != 0)
                        .map(|b| *b as char)
                        .collect::<String>();
                    lines.push(format!(
                        "identity device_id {} hardware_model {} firmware_version {}.{}.{}.{} provisioned_at {}",
                        hex_bytes(&rec.device_id),
                        model,
                        rec.firmware_version[0],
                        rec.firmware_version[1],
                        rec.firmware_version[2],
                        rec.firmware_version[3],
                        rec.provisioned_at
                    ));
                }
            } else {
                // record_count 0 or a record size that is neither 32 nor 96:
                // "unknown layout" floor, metadata only (AT-67).
                lines.push("unknown layout".to_string());
                lines.push(metadata_line(floor));
                lines.push(format!("record size {rec_size}"));
            }
        }
        TILE_TYPE_SEAL => {
            let sz = std::mem::size_of::<s3a_edge::DeviceSealRecord>();
            let n = payload.len() / sz;
            for r in 0..n {
                let rec: s3a_edge::DeviceSealRecord =
                    bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                lines.push(format!(
                    "seal tile_index {} generation {} seal_hash {} prev_seal_hash {}",
                    rec.tile_index,
                    rec.generation,
                    hex_bytes(&rec.seal_hash),
                    hex_bytes(&rec.prev_seal_hash),
                ));
            }
        }
        TILE_TYPE_CONSENT => {
            let sz = std::mem::size_of::<EdgeConsentRecord>();
            let n = payload.len() / sz;
            for r in 0..n {
                let rec: EdgeConsentRecord =
                    bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                lines.push(format!(
                    "consent owner_uuid {} allowed_purposes {} granted_at {} expires_at {}",
                    hex_bytes(&rec.owner_uuid),
                    rec.allowed_purposes,
                    rec.granted_at,
                    rec.expires_at
                ));
            }
        }
        _ => {
            // Every Edge floor whose type is other than 64, 70, or 71 shows
            // metadata only, never raw or decoded payload fields (AT-232).
            lines.push(metadata_line(floor));
        }
    }
    Ok(())
}

fn metadata_line(floor: &Floor) -> String {
    format!(
        "type {} size {} record_count {} payload_hash_short {}",
        floor.type_id, floor.size_bytes, floor.record_count, floor.payload_hash_short
    )
}

fn purpose_name(purpose: u32) -> String {
    match ConsentPurpose::from_u32(purpose) {
        Some(p) => p.as_str().to_string(),
        None => format!("{purpose}"),
    }
}

/// EDGE §5.2.2 event names.
fn event_name(event_type: u16) -> &'static str {
    match event_type {
        0x0001 => "machine_start",
        0x0002 => "machine_stop",
        0x0003 => "emergency_stop",
        0x0004 => "fault_code",
        0x0005 => "maintenance",
        0x0006 => "calibration",
        0x0007 => "operator_change",
        _ => "unknown",
    }
}

/// §5.4 item 5 category labels (never the only channel; severity also shown).
fn event_category(event_type: u16) -> &'static str {
    match event_type {
        0x0003 | 0x0004 => "fault",
        0x0005 | 0x0006 => "maintenance",
        0x0001 | 0x0002 | 0x0007 => "normal",
        _ => "unknown",
    }
}

fn quarantine_plain(reason: &str) -> &'static str {
    match reason {
        "seal_trailer" => "seal failure",
        "crc" => "CRC failure",
        "chain_break" => "seal-chain break",
        "signature_known_signer" => "signature failure",
        "signer_not_pinned" => "signer not pinned",
        "missing_signature" => "missing signatures",
        _ => "quarantined",
    }
}

/// Bay side at reference scale (§5.8): nearest integer to 32*sqrt(stride/4096).
fn bay_side(size_bytes: u32) -> i32 {
    let side = 32.0 * ((size_bytes as f64) / 4096.0).sqrt();
    side.round() as i32
}

/// Warehouse grid rect for one floor, shared by render_warehouse and
/// render_floor_transition so the start rect agrees (AT-62).
fn warehouse_floor_rect(
    floors: &[Floor],
    floor_index: u32,
    width: f64,
    _height: f64,
    scroll_x: f64,
    scroll_y: f64,
) -> Option<(i32, i32, u32, u32)> {
    let pos = floors.iter().position(|f| f.index == floor_index)?;
    let floor = &floors[pos];
    let side = bay_side(floor.size_bytes);
    let gap = 8i32;
    let stride = side + gap;
    let cols = (((width - 16.0) / stride as f64).floor() as usize).max(1);
    let col = (pos % cols) as i32;
    let row = (pos / cols) as i32;
    let x = 16 + col * stride - scroll_x as i32;
    let y = 16 + row * stride - scroll_y as i32;
    Some((x, y, side as u32, side as u32))
}

fn floor_node(floor: &Floor, x: i32, y: i32, w: u32, h: u32) -> SemanticNode {
    SemanticNode {
        role: "floor".into(),
        label: format!("floor {}", floor.index),
        x,
        y,
        width: w,
        height: h,
        href: None,
        paper_id: None,
        floor: Some(floor.index),
        opacity: None,
    }
}

/// y-value of CSS cubic-bezier(.2,.8,.2,1) at unit time `u` in [0,1].
/// Solves x(t) = u by bisection (x is monotonic), then returns y(t).
fn cubic_bezier_y(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    if u <= 0.0 {
        return 0.0;
    }
    if u >= 1.0 {
        return 1.0;
    }
    // x(t) = 0.6*t*(1-t) + t^3 ; y(t) = 2.4*t*(1-t)^2 + 3*t^2*(1-t) + t^3
    let x = |t: f64| 0.6 * t * (1.0 - t) + t * t * t;
    let y = |t: f64| 2.4 * t * (1.0 - t) * (1.0 - t) + 3.0 * t * t * (1.0 - t) + t * t * t;
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if x(mid) < u {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    y((lo + hi) / 2.0)
}

/// RFC3339 seconds UTC (e.g. 2026-11-18T00:00:00Z) without a date crate.
fn rfc3339_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };
    format!("{yr:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
