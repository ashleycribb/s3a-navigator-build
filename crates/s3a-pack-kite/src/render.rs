//! Renderers: first view, warehouse, floor detail, floor transition (§5.8).
//!
//! Text is painted as text (never executed as HTML/JS). BLOB and PROVENANCE
//! floors show metadata only, never raw payload bytes (AT-107). Floors whose
//! layout kite cannot decode show metadata only. Withheld payloads do not
//! appear in `text_runs`, `semantics`, pixels, or hit-test targets (DD-125).
//!
//! Pixel paint (§5.8, step 8c): `finish_frame` paints text runs as ink
//! rects and semantic node fills (severity badges in the Okabe-Ito DD-07
//! colours) into `rgba`; it is rect-fill paint, not glyph rasterization
//! (no glyph rasterizer is wired in this tree). TextRun rects are pixmap
//! coordinates; SemanticNode rects stay CSS pixels.

use s3a_core::consent::{ConsentPurpose, ConsentRecord};
use s3a_core::{MarkRecord, TileType};
use s3a_edge::{
    DeviceIdentityRecord, EdgeConsentRecord, EquipmentEventRecord, TILE_TYPE_CONSENT,
    TILE_TYPE_PROVENANCE, TILE_TYPE_RECORD, TILE_TYPE_SEAL,
};
use s3a_engine::MmapReader;
use sha2::{Digest, Sha256};

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
    if matches!(
        handle.kind_enum(),
        Kind::Wiki | Kind::Html | Kind::Other
    ) {
        return Err(KiteError::new(
            "view_mismatch",
            "manifest is not a view for a page kind",
        ));
    }
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
            // Page kinds are refused above (view_mismatch).
            Kind::Wiki | Kind::Html | Kind::Other => unreachable!(),
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
    let frame = finish_frame(pix_w, pix_h, runs, semantics, device_pixel_ratio);
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
    if matches!(
        handle.kind_enum(),
        Kind::Wiki | Kind::Html | Kind::Other
    ) {
        return Err(KiteError::new(
            "not_archive",
            "render_warehouse is not defined for a page kind",
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
    let frame = finish_frame(pix_w, pix_h, runs, semantics, device_pixel_ratio);
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
    if matches!(
        handle.kind_enum(),
        Kind::Wiki | Kind::Html | Kind::Other
    ) {
        return Err(KiteError::new(
            "not_archive",
            "render_floor_detail is not defined for a page kind",
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
    let mut runs = Vec::new();
    let mut y = 16i32;
    push_lines(&mut runs, &format!("floor {floor_index}"), &mut y);
    // Severity badges align with their event rows: the first content line
    // (withheld_count) is at this y, event r at y + 20*(1+r).
    let content_y = y;
    let (lines, mut semantics) = floor_detail_content(handle, floor_index, content_y)?;
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
    let frame = finish_frame(pix_w, pix_h, runs, semantics, device_pixel_ratio);
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
    if matches!(
        handle.kind_enum(),
        Kind::Wiki | Kind::Html | Kind::Other
    ) {
        return Err(KiteError::new(
            "not_archive",
            "render_floor_transition is not defined for a page kind",
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
    let mut runs = Vec::new();
    let mut y = 16i32;
    push_lines(&mut runs, &format!("floor {floor_index}"), &mut y);
    let content_y = y;
    let (lines, mut semantics) = floor_detail_content(handle, floor_index, content_y)?;
    for line in &lines {
        push_lines(&mut runs, line, &mut y);
    }
    semantics.push(card);
    let frame = finish_frame(pix_w, pix_h, runs, semantics, device_pixel_ratio);
    handle.store_frame("floor-transition", Some(floor_index), frame.clone());
    Ok(frame)
}

/// Floor-detail text lines plus extra semantic nodes (severity badges).
/// Withheld payloads contribute nothing (DD-125).
fn floor_detail_content(
    handle: &Handle,
    floor_index: u32,
    content_y: i32,
) -> Result<(Vec<String>, Vec<SemanticNode>), KiteError> {
    let reader = handle.reader()?;
    let eval = handle.consent_eval()?;
    let floors = handle.floors();
    let floor = floors
        .iter()
        .find(|f| f.index == floor_index)
        .ok_or_else(|| KiteError::new("invalid_floor", format!("floor {floor_index} not found")))?
        .clone();
    let mut lines: Vec<String> = Vec::new();
    let mut nodes: Vec<SemanticNode> = Vec::new();
    let withheld_here = eval.withheld_floor.get(&floor_index).copied().unwrap_or(0);
    lines.push(format!("withheld_count: {withheld_here}"));

    if reader.is_edge_archive() {
        edge_floor_content(&reader, &floor, &floors, &eval, content_y, &mut lines, &mut nodes)?;
    } else {
        hyper_floor_content(&reader, &floor, &floors, &eval, &mut lines)?;
    }
    Ok((lines, nodes))
}

/// A chunked-payload prefix (pin registry.md, Tiles 68/72): 32-byte key,
/// `chunk_index` LE u32 at offset 32, `total_chunks` LE u32 at offset 36,
/// then chunk data. A payload that does not parse as a chunk (shorter than
/// 40 bytes, `total_chunks` 0, or `chunk_index >= total_chunks`) is not a
/// chunked payload.
struct ChunkPrefix {
    key: [u8; 32],
    index: u32,
    total: u32,
}

fn parse_chunk_prefix(payload: &[u8]) -> Option<ChunkPrefix> {
    if payload.len() < 40 {
        return None;
    }
    let total = u32::from_le_bytes(payload[36..40].try_into().ok()?);
    let index = u32::from_le_bytes(payload[32..36].try_into().ok()?);
    if total == 0 || index >= total {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&payload[..32]);
    Some(ChunkPrefix { key, index, total })
}

fn chunk_metadata_line(prefix: &ChunkPrefix) -> String {
    format!(
        "chunk {}/{} blob_key {}",
        prefix.index,
        prefix.total,
        hex_bytes(&prefix.key)
    )
}

/// Reassemble one key's chunk portions (AT-32). `chunks` holds
/// (chunk_index, data) for a single key; chunks are ordered by index, and
/// every chunk 0..total-1 must be present, or reassembly fails (None).
fn reassemble_chunks(chunks: &[(u32, Vec<u8>)], total: u32) -> Option<Vec<u8>> {
    let mut doc = Vec::new();
    for want in 0..total {
        let (_, data) = chunks.iter().find(|(idx, _)| *idx == want)?;
        doc.extend_from_slice(data);
    }
    Some(doc)
}

/// A chunk prefix is credible only if the reassembled content hashes to
/// the key (SHA-256: AT-32 for PROVENANCE; the registry calls the BLOB key
/// a "content hash"). A raw-text payload whose bytes 32..40 happen to form
/// a plausible index/total — the AT-107 shape — fails this check and is
/// treated as unprefixed. Without it, a 40+ byte text payload would be
/// misread as a chunk and only its tail shown.
fn credible_chunk(
    payload: &[u8],
    chunks: &[(u32, Vec<u8>)],
) -> Option<(ChunkPrefix, Vec<u8>)> {
    let prefix = parse_chunk_prefix(payload)?;
    let content = reassemble_chunks(chunks, prefix.total)?;
    let hash: [u8; 32] = Sha256::digest(&content).into();
    if hash == prefix.key {
        Some((prefix, content))
    } else {
        None
    }
}

/// Push PROVENANCE document lines for one floor: metadata, then the
/// chunk line when the 40-byte prefix parses, then the reassembled document
/// for the key when every chunk is present (AT-32), else this chunk's own
/// portion; without a prefix, the whole payload as text when valid UTF-8
/// (AT-107). Never raw BLOB bytes.
fn push_provenance_lines(
    lines: &mut Vec<String>,
    floor: &Floor,
    payload: &[u8],
    chunks: &[(u32, Vec<u8>)],
) {
    lines.push(metadata_line(floor));
    match credible_chunk(payload, chunks) {
        Some((prefix, doc)) => {
            lines.push(chunk_metadata_line(&prefix));
            push_text_lines(lines, &doc);
        }
        None => push_text_lines(lines, payload),
    }
}

fn push_text_lines(lines: &mut Vec<String>, bytes: &[u8]) {
    if let Ok(s) = std::str::from_utf8(bytes) {
        for line in s.split('\n') {
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
    }
}

/// All chunk portions for one key on valid hyper-tile floors of the
/// given tile type (`TileType::*` constants are `u32`).
fn hyper_chunks(
    reader: &MmapReader,
    floors: &[Floor],
    key: &[u8; 32],
    want_type: u32,
) -> Vec<(u32, Vec<u8>)> {
    let valid: std::collections::HashSet<u32> = floors.iter().map(|f| f.index).collect();
    let mut chunks = Vec::new();
    for i in 0..reader.tile_count() {
        if !valid.contains(&i) {
            continue;
        }
        let Ok((header, payload)) = reader.try_get_tile_unverified(i as usize) else {
            continue;
        };
        if header.tile_type != want_type {
            continue;
        }
        let Some(prefix) = parse_chunk_prefix(payload) else {
            continue;
        };
        if prefix.key != *key {
            continue;
        }
        chunks.push((prefix.index, payload[40..].to_vec()));
    }
    chunks
}

/// All chunk portions for one key on valid Edge floors of the given type id.
fn edge_chunks(
    reader: &MmapReader,
    floors: &[Floor],
    key: &[u8; 32],
    want_type_id: u16,
) -> Vec<(u32, Vec<u8>)> {
    let valid: std::collections::HashSet<u32> = floors.iter().map(|f| f.index).collect();
    let mut chunks = Vec::new();
    for i in 0..reader.tile_count() {
        if !valid.contains(&i) {
            continue;
        }
        let Ok((header, payload)) = reader.try_get_edge_tile_unverified(i as usize) else {
            continue;
        };
        if header.type_id != want_type_id {
            continue;
        }
        let Some(prefix) = parse_chunk_prefix(payload) else {
            continue;
        };
        if prefix.key != *key {
            continue;
        }
        chunks.push((prefix.index, payload[40..].to_vec()));
    }
    chunks
}

fn hyper_floor_content(
    reader: &MmapReader,
    floor: &Floor,
    floors: &[Floor],
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
            // PROVENANCE text is shown; raw captured bodies are not (the
            // payload here is the provenance document, not a BLOB body).
            // Chunked PROVENANCE (40-byte prefix) shows the reassembled
            // document for its key (AT-32); unprefixed payloads keep the
            // AT-107 whole-payload-as-text shape.
            let chunks = match parse_chunk_prefix(payload) {
                Some(p) => hyper_chunks(reader, floors, &p.key, TileType::PROVENANCE),
                None => Vec::new(),
            };
            push_provenance_lines(lines, floor, payload, &chunks);
        }
        _ => {
            // BLOB and every floor whose layout kite cannot decode: metadata
            // only, never raw payload bytes (AT-107). A chunked BLOB shows
            // its chunk index/total and key as metadata (AT-107 / §5.3
            // item 5) — never the chunk bytes — but only when the chunk
            // prefix is credible (reassembled content hashes to the key).
            lines.push(metadata_line(floor));
            if let Some(prefix) = parse_chunk_prefix(payload) {
                let chunks = hyper_chunks(reader, floors, &prefix.key, header.tile_type);
                if let Some((prefix, _)) = credible_chunk(payload, &chunks) {
                    lines.push(chunk_metadata_line(&prefix));
                }
            }
        }
    }
    Ok(())
}

fn edge_floor_content(
    reader: &MmapReader,
    floor: &Floor,
    floors: &[Floor],
    eval: &ConsentEval,
    content_y: i32,
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
                    let mut y = content_y + 20;
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
            // PROVENANCE (72) shows its reassembled document (AT-32); a
            // chunked payload of any other type shows its chunk index/total
            // and key as metadata — never the chunk bytes.
            if type_id == TILE_TYPE_PROVENANCE {
                let chunks = match parse_chunk_prefix(payload) {
                    Some(p) => edge_chunks(reader, floors, &p.key, TILE_TYPE_PROVENANCE),
                    None => Vec::new(),
                };
                push_provenance_lines(lines, floor, payload, &chunks);
            } else {
                lines.push(metadata_line(floor));
                if let Some(prefix) = parse_chunk_prefix(payload) {
                    let chunks = edge_chunks(reader, floors, &prefix.key, type_id);
                    if let Some((prefix, _)) = credible_chunk(payload, &chunks) {
                        lines.push(chunk_metadata_line(&prefix));
                    }
                }
            }
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

pub(crate) fn pixmap_size(
    handle: &Handle,
    width: f64,
    height: f64,
    dpr: f64,
) -> Result<(u32, u32), KiteError> {
    // §5.8 / §5.2.1 catalogue (AT-141): non-finite w/h/dpr, or negative w/h,
    // is invalid_param; w or h == 0, a well-typed dpr <= 0, or a pixmap
    // product over max_frame_pixels is invalid_frame_size.
    if !width.is_finite() || !height.is_finite() || !dpr.is_finite() {
        return Err(KiteError::new(
            "invalid_param",
            "width, height, and device_pixel_ratio must be finite numbers",
        ));
    }
    if width < 0.0 || height < 0.0 {
        return Err(KiteError::new(
            "invalid_param",
            "width and height must not be negative",
        ));
    }
    if width == 0.0 || height == 0.0 || dpr <= 0.0 {
        return Err(KiteError::new(
            "invalid_frame_size",
            "width and height must be > 0 and device_pixel_ratio must be > 0",
        ));
    }
    let pix_w = (width * dpr).ceil() as u32;
    let pix_h = (height * dpr).ceil() as u32;
    let product = (pix_w as u64).saturating_mul(pix_h as u64);
    if product > handle.max_frame_pixels() {
        return Err(KiteError::new(
            "invalid_frame_size",
            "pixmap width*height exceeds max_frame_pixels",
        ));
    }
    Ok((pix_w, pix_h))
}

pub(crate) fn push_lines(runs: &mut Vec<TextRun>, text: &str, y: &mut i32) {
    for line in text.split('\n') {
        let w = (line.len() as u32).saturating_mul(8).max(8);
        push_run(runs, line, 16, *y, w, 16);
        *y = y.saturating_add(20);
    }
}

pub(crate) fn push_run(runs: &mut Vec<TextRun>, text: &str, x: i32, y: i32, width: u32, height: u32) {
    runs.push(TextRun {
        text: text.to_string(),
        x,
        y,
        width,
        height,
    });
}

/// Paint palette (step 8c, §5.8). Only the severity badge fills are SPEC
/// (DD-07 / Okabe-Ito); the rest are implementation choices, logged in the
/// step-8c test log. Paint is a pure function of (text_runs, semantics) plus
/// CSS size and dpr — that purity is what makes the §5.8 control oracle
/// testable. This is rect-fill paint, not glyph rasterization: no glyph
/// rasterizer is wired in this tree (no vello/font crate is a dependency).
pub const PAINT_BACKGROUND: [u8; 4] = [255, 255, 255, 255];
const PAINT_INK: [u8; 4] = [0x22, 0x22, 0x22, 0xFF];
const PAINT_BAY: [u8; 4] = [0xE8, 0xE8, 0xE8, 0xFF];
const PAINT_BUTTON: [u8; 4] = [0xDD, 0xDD, 0xDD, 0xFF];
const PAINT_CARD: [u8; 4] = [0xF2, 0xF2, 0xF2, 0xFF];
const PAINT_LABEL: [u8; 4] = [0x99, 0x99, 0x99, 0xFF];

/// Severity badge fills (DD-07, Okabe-Ito): 0 #0072B2, 1 #E69F00, 2 and 3
/// #D55E00. Colour is never the only signal: the `severity: N` label and
/// the non-colour category label are always present alongside the fill.
fn severity_fill(severity: u8) -> [u8; 4] {
    match severity {
        0 => [0x00, 0x72, 0xB2, 0xFF],
        1 => [0xE6, 0x9F, 0x00, 0xFF],
        _ => [0xD5, 0x5E, 0x00, 0xFF],
    }
}

fn fill_rect(rgba: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: [u8; 4]) {
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = (x as i64 + w as i64).clamp(0, width as i64) as u32;
    let y1 = (y as i64 + h as i64).clamp(0, height as i64) as u32;
    for py in y0..y1 {
        for px in x0..x1 {
            let idx = ((py * width + px) * 4) as usize;
            rgba[idx..idx + 4].copy_from_slice(&color);
        }
    }
}

/// Scale a CSS rect to pixmap coordinates (round; exact at dpr 1.0/2.0).
fn scale_rect(x: i32, y: i32, w: u32, h: u32, dpr: f64) -> (i32, i32, u32, u32) {
    (
        (x as f64 * dpr).round() as i32,
        (y as f64 * dpr).round() as i32,
        (w as f64 * dpr).round().max(0.0) as u32,
        (h as f64 * dpr).round().max(0.0) as u32,
    )
}

pub(crate) fn finish_frame(
    width: u32,
    height: u32,
    text_runs: Vec<TextRun>,
    semantics: Vec<SemanticNode>,
    dpr: f64,
) -> RenderFrame {
    // §5.8: TextRun coordinates are pixmap coordinates; SemanticNode
    // coordinates stay CSS pixels. Runs are scaled here, once, so every
    // caller passes CSS rects and the returned runs are pixmap rects.
    let scaled_runs: Vec<TextRun> = text_runs
        .into_iter()
        .map(|r| {
            let (x, y, w, h) = scale_rect(r.x, r.y, r.width, r.height, dpr);
            TextRun {
                text: r.text,
                x,
                y,
                width: w,
                height: h,
            }
        })
        .collect();
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&PAINT_BACKGROUND);
    }
    // Paint order: text rects first, node fills on top, so a badge centre
    // always samples its severity fill even where the badge overlaps its
    // row's text rect.
    for run in &scaled_runs {
        fill_rect(&mut rgba, width, height, run.x, run.y, run.width, run.height, PAINT_INK);
    }
    for node in &semantics {
        let (x, y, w, h) = scale_rect(node.x, node.y, node.width, node.height, dpr);
        let color = match node.role.as_str() {
            "label" => {
                // A "severity: N" label is a severity badge (DD-07); any
                // other label node gets the neutral label fill.
                node.label
                    .strip_prefix("severity: ")
                    .and_then(|n| n.trim().parse::<u8>().ok())
                    .map(severity_fill)
                    .unwrap_or(PAINT_LABEL)
            }
            "floor" => PAINT_BAY,
            "continue" | "select" => PAINT_BUTTON,
            "card" => PAINT_CARD,
            _ => continue,
        };
        fill_rect(&mut rgba, width, height, x, y, w, h, color);
    }
    RenderFrame {
        rgba,
        width,
        height,
        text_runs: scaled_runs,
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
