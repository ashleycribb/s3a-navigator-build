//! Open paths: pack `open` (DD-115 / DD-150 derivation) and `open_verified`
//! (§5.1 supplied-report validation, DD-80 / AT-137 / AT-211).
//!
//! `open` derives the verification report with the §5.4 / DD-150 table
//! (`verify::derive_verification`) and never treats an embedded key as a trust
//! anchor when the pin list is non-empty (DD-79, DD-98). `open_verified`
//! validates the supplied report against the bytes and does not recompute
//! verification or the unsigned-tail union (AT-211).

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use s3a_core::{
    FileHeader, TileType, FILE_HEADER_SLOT_A_OFFSET, FILE_HEADER_SLOT_B_OFFSET,
    FILE_HEADER_SLOT_SIZE, FLAG_PROFILE_EDGE, FLAG_TILE_SIZE_4KB, FLAG_TILE_SIZE_8KB,
    HYPER_TILE_SIZE, S3A_MAGIC,
};
use s3a_engine::{verify_header_slot, MmapReader};
use sha2::{Digest, Sha256};

use crate::consent::{evaluate, ConsentEval, EvalInputs};
use crate::context::Clock;
use crate::render::{cache_frame, last_frame, RenderFrame};
use crate::report::KiteVerificationReport;
use crate::types::{EdgeDeviceInfo, EdgeHeaderInfo, FirstView, Floor, Hit, Kind, ManifestState};
use crate::verify::{derive_verification, DerivedVerification};
use crate::{KiteContext, KiteError};

const VALID_STRIDES: &[u32] = &[0, 4096, 8192, HYPER_TILE_SIZE as u32];

pub struct Handle {
    kind: Kind,
    first_view: FirstView,
    max_frame_pixels: u64,
    bytes: Vec<u8>,
    derived: DerivedVerification,
    clock: Arc<dyn Clock + Send + Sync>,
    report: KiteVerificationReport,
    floors: Vec<Floor>,
    manifest: ManifestState,
    edge_header: Option<EdgeHeaderInfo>,
    inner: RefCell<HandleInner>,
}

struct HandleInner {
    proceeded: bool,
    last_frames: Vec<(String, Option<u32>, RenderFrame)>,
}

impl Handle {
    pub fn kind(&self) -> &str {
        self.kind.as_str()
    }

    pub fn first_view(&self) -> &str {
        self.first_view.as_str()
    }

    pub(crate) fn kind_enum(&self) -> Kind {
        self.kind
    }

    pub(crate) fn first_view_enum(&self) -> FirstView {
        self.first_view
    }

    pub(crate) fn max_frame_pixels(&self) -> u64 {
        self.max_frame_pixels
    }

    pub(crate) fn proceeded(&self) -> bool {
        self.inner.borrow().proceeded
    }

    pub(crate) fn mark_proceeded(&self) {
        self.inner.borrow_mut().proceeded = true;
    }

    pub(crate) fn store_frame(&self, view: &str, floor: Option<u32>, frame: RenderFrame) {
        cache_frame(&mut self.inner.borrow_mut().last_frames, view, floor, frame);
    }

    pub(crate) fn frame(&self, view: &str, floor: Option<u32>) -> Option<RenderFrame> {
        last_frame(&self.inner.borrow().last_frames, view, floor)
    }

    pub(crate) fn report(&self) -> KiteVerificationReport {
        self.report.clone()
    }

    pub(crate) fn floors(&self) -> Vec<Floor> {
        self.floors.clone()
    }

    pub(crate) fn manifest(&self) -> ManifestState {
        self.manifest.clone()
    }

    pub(crate) fn edge_header(&self) -> Option<EdgeHeaderInfo> {
        self.edge_header.clone()
    }

    pub(crate) fn derived(&self) -> &DerivedVerification {
        &self.derived
    }

    /// A fresh reader over the stored bytes (DD-75: from_bytes, no path mmap).
    pub(crate) fn reader(&self) -> Result<MmapReader, KiteError> {
        MmapReader::from_bytes(&self.bytes)
            .map_err(|e| KiteError::new("invalid_magic", format!("MmapReader::from_bytes: {e}")))
    }

    /// Current clock seconds (injected clock for tests, process clock live).
    pub(crate) fn now_secs(&self) -> i64 {
        self.clock.unix_utc_secs()
    }

    /// Re-evaluate the DD-147 consent table against the current clock.
    pub(crate) fn consent_eval(&self) -> Result<ConsentEval, KiteError> {
        let reader = self.reader()?;
        Ok(evaluate(&EvalInputs {
            reader: &reader,
            report: &self.report,
            highest_authentic: self.derived.highest_authentic,
            has_seal_records: self.derived.has_seal_records,
            now: self.now_secs(),
        }))
    }

    /// Is this tile index a valid floor?
    pub(crate) fn is_valid_floor(&self, index: u32) -> bool {
        !self.report.invalid_tile_indices.contains(&index)
    }
}

pub fn open(
    ctx: &KiteContext,
    bytes: &[u8],
    name_hint: Option<String>,
    max_frame_pixels: u64,
    max_open_bytes: u64,
    pinned_keys: Vec<[u8; 32]>,
) -> Result<Handle, KiteError> {
    let _ = name_hint;
    if bytes.len() as u64 > max_open_bytes {
        return Err(KiteError::new(
            "file_too_large",
            "input exceeds max_open_bytes",
        ));
    }
    if bytes.len() < 4 || bytes[..4] != S3A_MAGIC {
        return Err(KiteError::new(
            "invalid_magic",
            "bytes do not begin with S3A_MAGIC",
        ));
    }
    let Some(header) = active_header(bytes) else {
        return Err(KiteError::new(
            "invalid_magic",
            "S3A header cannot be opened",
        ));
    };
    if header.version != 1 {
        return Err(KiteError::new(
            "unsupported_version",
            format!("Unsupported file version: {}", header.version),
        ));
    }

    if let Some(banner) = unsupported_banner(&header) {
        return Ok(unsupported_handle(max_frame_pixels, banner));
    }

    // Every supported archive file type is opened through from_bytes (DD-75).
    let reader = MmapReader::from_bytes(bytes)
        .map_err(|e| KiteError::new("invalid_magic", format!("MmapReader::from_bytes: {e}")))?;

    let derived = derive_verification(bytes, &reader, &pinned_keys);
    let is_edge = reader.is_edge_archive();
    let floors = build_floors(&reader, &derived.report);
    let (manifest, edge_header) = if is_edge {
        (
            ManifestState::Text(String::new()),
            Some(edge_header_info(&reader)),
        )
    } else {
        (hyper_manifest(bytes, &reader, &derived.report), None)
    };
    Ok(Handle {
        kind: if is_edge { Kind::Edge } else { Kind::HyperTile },
        first_view: if is_edge {
            FirstView::Header
        } else {
            FirstView::Manifest
        },
        max_frame_pixels,
        bytes: bytes.to_vec(),
        report: derived.report.clone(),
        derived,
        clock: ctx.clock_arc(),
        floors,
        manifest,
        edge_header,
        inner: RefCell::new(HandleInner {
            proceeded: false,
            last_frames: Vec::new(),
        }),
    })
}

/// `open_verified` (§5.1): use the supplied report for label, seal_status,
/// quarantine banner, and valid floors; do not recompute verification or the
/// unsigned-tail union. Violations of the report checks are `KiteError`
/// `invalid_report` (AT-211); `status: unverified` is `KiteError` (AT-154);
/// `unsupported` bytes are `not_archive` (AT-97).
pub fn open_verified(
    ctx: &KiteContext,
    bytes: &[u8],
    name_hint: Option<String>,
    max_frame_pixels: u64,
    max_open_bytes: u64,
    report: KiteVerificationReport,
) -> Result<Handle, KiteError> {
    let _ = name_hint;
    if bytes.len() as u64 > max_open_bytes {
        return Err(KiteError::new(
            "file_too_large",
            "input exceeds max_open_bytes",
        ));
    }
    if bytes.len() < 4 || bytes[..4] != S3A_MAGIC {
        return Err(KiteError::new(
            "invalid_magic",
            "bytes do not begin with S3A_MAGIC",
        ));
    }
    let Some(header) = active_header(bytes) else {
        return Err(KiteError::new(
            "invalid_magic",
            "S3A header cannot be opened",
        ));
    };
    if header.version != 1 {
        return Err(KiteError::new(
            "unsupported_version",
            format!("Unsupported file version: {}", header.version),
        ));
    }
    if unsupported_banner(&header).is_some() {
        // SPEC §5.1 (AT-97): bytes that classify as `unsupported` are
        // KiteError `not_archive` from open_verified, not `invalid_report`.
        // (The `open` path keeps the banner handle; only open_verified
        // errors here.)
        return Err(KiteError::new(
            "not_archive",
            "bytes classify as unsupported, not an archive (AT-97)",
        ));
    }
    let reader = MmapReader::from_bytes(bytes)
        .map_err(|e| KiteError::new("invalid_magic", format!("MmapReader::from_bytes: {e}")))?;
    let is_edge = reader.is_edge_archive();
    let tiles_total = reader.tile_count();
    validate_report(&report, tiles_total, is_edge)?;

    // Floors are the complement of invalid_tile_indices in 0..tiles_total.
    let invalid: HashSet<u32> = report.invalid_tile_indices.iter().copied().collect();
    let floors = build_floors_for(&reader, &invalid);
    let (manifest, edge_header) = if is_edge {
        (
            ManifestState::Text(String::new()),
            Some(edge_header_info(&reader)),
        )
    } else {
        (hyper_manifest(bytes, &reader, &report), None)
    };
    // Supplied report: no recomputation, so no authentic-record coverage is
    // known; the supplied floor list already encodes the unsigned tail.
    let derived = DerivedVerification {
        report: report.clone(),
        highest_authentic: None,
        has_seal_records: false,
        unsigned_tail: Vec::new(),
        tiles_total,
    };
    Ok(Handle {
        kind: if is_edge { Kind::Edge } else { Kind::HyperTile },
        first_view: if is_edge {
            FirstView::Header
        } else {
            FirstView::Manifest
        },
        max_frame_pixels,
        bytes: bytes.to_vec(),
        derived,
        clock: ctx.clock_arc(),
        report,
        floors,
        manifest,
        edge_header,
        inner: RefCell::new(HandleInner {
            proceeded: false,
            last_frames: Vec::new(),
        }),
    })
}

/// The §5.1 supplied-report checks. `status: unverified` is a KiteError with
/// code `unverified` (AT-154), not `invalid_report` (AT-211).
fn validate_report(
    report: &KiteVerificationReport,
    tiles_total: u32,
    is_edge: bool,
) -> Result<(), KiteError> {
    let invalid_report = |msg: &str| KiteError::new("invalid_report", msg.to_string());
    if report.status == "unverified" {
        return Err(KiteError::new(
            "unverified",
            "status unverified: the caller must run verify first (AT-154)",
        ));
    }
    let in_range = |i: u32| i < tiles_total;
    if report
        .invalid_tile_indices
        .iter()
        .any(|i| !in_range(*i))
        || report.torn_tile_indices.iter().any(|i| !in_range(*i))
    {
        return Err(invalid_report(
            "invalid_tile_indices or torn_tile_indices outside 0..tiles_total-1",
        ));
    }
    let invalid_set: HashSet<u32> = report.invalid_tile_indices.iter().copied().collect();
    if report
        .torn_tile_indices
        .iter()
        .any(|i| !invalid_set.contains(i))
    {
        return Err(invalid_report(
            "torn_tile_indices is not a subset of invalid_tile_indices",
        ));
    }
    if let Some(b) = report.break_index {
        if b >= tiles_total {
            return Err(invalid_report("break_index >= tiles_total"));
        }
        if (b..tiles_total).any(|i| !invalid_set.contains(&i)) {
            return Err(invalid_report(
                "every index at or after break_index must be in invalid_tile_indices",
            ));
        }
    }
    if matches!(
        report.quarantine_reason.as_deref(),
        Some("signer_not_pinned") | Some("missing_signature")
    ) && report.invalid_tile_indices.len() as u32 != tiles_total
    {
        return Err(invalid_report(
            "signer_not_pinned / missing_signature requires invalid_tile_indices to be the full range",
        ));
    }
    match (&report.status, &report.quarantine_reason) {
        (s, None) if s == "quarantined" => {
            return Err(invalid_report(
                "status quarantined requires a quarantine_reason",
            ));
        }
        (s, Some(_)) if s != "quarantined" => {
            return Err(invalid_report(
                "quarantine_reason requires status quarantined",
            ));
        }
        _ => {}
    }
    if matches!(
        report.quarantine_reason.as_deref(),
        Some("chain_break") | Some("signature_known_signer") | Some("crc")
    ) && report.break_index.is_none()
    {
        return Err(invalid_report(
            "quarantine_reason chain_break / signature_known_signer / crc requires break_index",
        ));
    }
    if !is_edge {
        if matches!(
            report.quarantine_reason.as_deref(),
            Some("signature_known_signer")
                | Some("chain_break")
                | Some("missing_signature")
                | Some("signer_not_pinned")
        ) {
            return Err(invalid_report(
                "a hyper-tile report does not use that quarantine_reason",
            ));
        }
        if report.status == "verified" || report.status == "self_signed" {
            return Err(invalid_report(
                "a hyper-tile report does not use status verified or self_signed",
            ));
        }
        let seal_failed = report.seal_status == "seal_failed";
        let reason_is_trailer = report.quarantine_reason.as_deref() == Some("seal_trailer");
        if reason_is_trailer != seal_failed {
            return Err(invalid_report(
                "a hyper-tile report has quarantine_reason seal_trailer iff seal_status is seal_failed",
            ));
        }
        if seal_failed && report.status != "quarantined" {
            return Err(invalid_report(
                "a hyper-tile report with seal_status seal_failed has status quarantined",
            ));
        }
    } else if report.quarantine_reason.as_deref() == Some("seal_trailer") {
        return Err(invalid_report(
            "an Edge report does not use quarantine_reason seal_trailer",
        ));
    }
    if report.quarantine_reason.as_deref() == Some("crc") && report.tiles_crc_failed.is_empty() {
        return Err(invalid_report(
            "quarantine_reason crc requires tiles_crc_failed to be non-empty",
        ));
    }
    if report
        .tiles_crc_failed
        .iter()
        .any(|i| !invalid_set.contains(i))
    {
        return Err(invalid_report(
            "tiles_crc_failed must be a subset of invalid_tile_indices",
        ));
    }
    if let Some(min_crc) = report.tiles_crc_failed.iter().min().copied() {
        match report.break_index {
            Some(b) if b <= min_crc => {}
            _ => {
                return Err(invalid_report(
                    "tiles_crc_failed non-empty requires break_index <= min(tiles_crc_failed)",
                ));
            }
        }
    }
    Ok(())
}

pub fn close(_handle: Handle) -> Result<(), KiteError> {
    Ok(())
}

pub fn verification_report(handle: &Handle) -> Result<KiteVerificationReport, KiteError> {
    if handle.kind_enum() == Kind::Unsupported {
        return Err(KiteError::new(
            "not_archive",
            "verification_report is not defined for kind unsupported",
        ));
    }
    Ok(handle.report())
}

pub fn proceed_to_warehouse(handle: &Handle) -> Result<(), KiteError> {
    match handle.kind_enum() {
        Kind::Unsupported => Err(KiteError::new(
            "unsupported",
            "proceed is not defined for kind unsupported",
        )),
        Kind::HyperTile | Kind::Edge => {
            handle.mark_proceeded();
            Ok(())
        }
    }
}

/// `hit_test` (§5.1, §5.2.6). Error order: finite x/y, then floor-param
/// errors (missing_param / invalid_param), then the §5.2.6 matrix
/// (view_mismatch / not_archive / unsupported), then no_frame. Pure: does
/// not change handle state (DD-97).
pub fn hit_test(
    handle: &Handle,
    view: &str,
    floor: Option<u32>,
    x: f64,
    y: f64,
) -> Result<Hit, KiteError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(KiteError::new(
            "invalid_param",
            "hit_test x/y must be finite",
        ));
    }
    let is_floor_view = view == "floor-detail" || view == "floor-transition";
    if is_floor_view && floor.is_none() {
        return Err(KiteError::new(
            "missing_param",
            "floor is required on floor-detail and floor-transition",
        ));
    }
    if !is_floor_view && floor.is_some() {
        return Err(KiteError::new(
            "invalid_param",
            "floor must be None on a non-floor view",
        ));
    }
    // §5.2.6 matrix.
    let kind = handle.kind_enum();
    if kind == Kind::Unsupported {
        if view == "banner" || view == "manifest" || view == "header" {
            return Ok(Hit::None);
        }
        return Err(KiteError::new(
            "unsupported",
            format!("hit_test view {view} is not valid for kind unsupported"),
        ));
    }
    match view {
        "manifest" | "header" | "banner" => {}
        "warehouse" => {
            if !handle.proceeded() {
                return Err(KiteError::new(
                    "view_mismatch",
                    "warehouse is not the current view until proceed_to_warehouse",
                ));
            }
        }
        "floor-detail" | "floor-transition" => {
            if !handle.proceeded() {
                return Err(KiteError::new(
                    "view_mismatch",
                    "floor views are not the current view until proceed_to_warehouse",
                ));
            }
        }
        _ => {
            return Err(KiteError::new(
                "invalid_view",
                format!("unknown hit_test view {view}"),
            ));
        }
    }
    if is_floor_view {
        // Floor-detail / floor-transition frames carry no hit targets.
        if handle.frame(view, floor).is_some() {
            return Ok(Hit::None);
        }
        return Err(KiteError::new(
            "no_frame",
            "no prior frame for that view and floor",
        ));
    }
    // First-view aliases share one normalized frame key ("first").
    let frame_view = match view {
        "manifest" | "header" | "banner" => "first",
        other => other,
    };
    let frame = handle.frame(frame_view, None).or_else(|| {
        // Back-compat: frames stored under the literal view name.
        handle.frame(view, None)
    });
    if let Some(frame) = frame {
        for node in &frame.semantics {
            if point_in_css(x, y, node.x, node.y, node.width, node.height) {
                match node.role.as_str() {
                    "continue" => return Ok(Hit::Continue),
                    "floor" => {
                        if let Some(n) = node.floor {
                            return Ok(Hit::Floor(n));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(Hit::None)
}

fn point_in_css(x: f64, y: f64, nx: i32, ny: i32, nw: u32, nh: u32) -> bool {
    let x0 = nx as f64;
    let y0 = ny as f64;
    x >= x0 && y >= y0 && x < x0 + nw as f64 && y < y0 + nh as f64
}

fn unsupported_handle(max_frame_pixels: u64, banner: String) -> Handle {
    Handle {
        kind: Kind::Unsupported,
        first_view: FirstView::Banner,
        max_frame_pixels,
        bytes: Vec::new(),
        derived: DerivedVerification {
            report: KiteVerificationReport::unsigned_unsealed(),
            highest_authentic: None,
            has_seal_records: false,
            unsigned_tail: Vec::new(),
            tiles_total: 0,
        },
        clock: Arc::new(crate::context::ProcessClock),
        report: KiteVerificationReport::unsigned_unsealed(),
        floors: Vec::new(),
        manifest: ManifestState::Unsupported { banner },
        edge_header: None,
        inner: RefCell::new(HandleInner {
            proceeded: false,
            last_frames: Vec::new(),
        }),
    }
}

fn active_header(bytes: &[u8]) -> Option<FileHeader> {
    if bytes.len() < FILE_HEADER_SLOT_B_OFFSET + FILE_HEADER_SLOT_SIZE {
        return None;
    }
    let slot =
        |off| bytemuck::pod_read_unaligned::<FileHeader>(&bytes[off..off + FILE_HEADER_SLOT_SIZE]);
    let a: FileHeader = slot(FILE_HEADER_SLOT_A_OFFSET);
    let b: FileHeader = slot(FILE_HEADER_SLOT_B_OFFSET);
    let a_ok = verify_header_slot(&a);
    let b_ok = verify_header_slot(&b);
    match (a_ok, b_ok) {
        (true, true) => {
            if b.generation >= a.generation {
                Some(b)
            } else {
                Some(a)
            }
        }
        (true, false) => Some(a),
        (false, true) => Some(b),
        (false, false) => None,
    }
}

fn unsupported_banner(header: &FileHeader) -> Option<String> {
    let stride = if header._reserved1 != 0 {
        header._reserved1
    } else {
        HYPER_TILE_SIZE as u32
    };
    let edge_flag = header.flags & FLAG_PROFILE_EDGE != 0;
    let flag_4 = header.flags & FLAG_TILE_SIZE_4KB != 0;
    let flag_8 = header.flags & FLAG_TILE_SIZE_8KB != 0;
    if !VALID_STRIDES.contains(&header._reserved1) && header._reserved1 != 0 {
        return Some("unsupported-stride banner".into());
    }
    if flag_4 && flag_8 {
        return Some("inconsistent-header banner".into());
    }
    if edge_flag && stride != 4096 && stride != 8192 {
        return Some("inconsistent-header banner".into());
    }
    if flag_4 && stride != 4096 {
        return Some("inconsistent-header banner".into());
    }
    if flag_8 && stride != 8192 {
        return Some("inconsistent-header banner".into());
    }
    None
}

/// Valid floors from a derived report: every index not in
/// `invalid_tile_indices`, with metadata from the verified read.
fn build_floors(reader: &MmapReader, report: &KiteVerificationReport) -> Vec<Floor> {
    let invalid: HashSet<u32> = report.invalid_tile_indices.iter().copied().collect();
    build_floors_for(reader, &invalid)
}

fn build_floors_for(reader: &MmapReader, invalid: &HashSet<u32>) -> Vec<Floor> {
    let mut floors = Vec::new();
    let is_edge = reader.is_edge_archive();
    let stride = reader.tile_stride() as u32;
    for i in 0..reader.tile_count() {
        if invalid.contains(&i) {
            continue;
        }
        if is_edge {
            if let Ok((header, payload)) = reader.try_get_edge_tile(i as usize) {
                floors.push(floor_from(
                    i,
                    header.type_id as u32,
                    header.record_count,
                    stride,
                    payload,
                ));
            }
        } else if let Ok((header, payload)) = reader.try_get_tile(i as usize) {
            floors.push(floor_from(
                i,
                header.tile_type,
                header.record_count,
                HYPER_TILE_SIZE as u32,
                payload,
            ));
        }
    }
    floors
}

/// First valid-floor tile 78 supplies the manifest text (§5.3 item 2).
fn hyper_manifest(
    _bytes: &[u8],
    reader: &MmapReader,
    report: &KiteVerificationReport,
) -> ManifestState {
    let invalid: HashSet<u32> = report.invalid_tile_indices.iter().copied().collect();
    for i in 0..reader.tile_count() {
        if invalid.contains(&i) {
            continue;
        }
        if let Ok((header, payload)) = reader.try_get_tile(i as usize) {
            if header.tile_type == TileType::MANIFEST {
                let len = (header.payload_bytes as usize).min(payload.len());
                return match String::from_utf8(payload[..len].to_vec()) {
                    Ok(s) => ManifestState::Text(s),
                    Err(_) => ManifestState::NotUtf8,
                };
            }
        }
    }
    ManifestState::Absent
}

/// Derived Edge header-sheet components (§5.4 item 3): only fields present.
/// The first DeviceIdentityRecord in tile order supplies device fields.
fn edge_header_info(reader: &MmapReader) -> EdgeHeaderInfo {
    let header = reader.file_header();
    let mut device = None;
    for i in 0..reader.tile_count() as usize {
        if let Ok((h, payload)) = reader.try_get_edge_tile_unverified(i) {
            if h.type_id == s3a_edge::TILE_TYPE_RECORD
                && payload.len() >= std::mem::size_of::<s3a_edge::DeviceIdentityRecord>()
            {
                let sz = std::mem::size_of::<s3a_edge::DeviceIdentityRecord>();
                // Only 96-byte records are DeviceIdentityRecords.
                if h.record_count > 0 && payload.len() / h.record_count as usize == sz {
                    let rec: s3a_edge::DeviceIdentityRecord =
                        bytemuck::pod_read_unaligned(&payload[..sz]);
                    if rec.verify() {
                        let model = rec
                            .hardware_model
                            .iter()
                            .take_while(|b| **b != 0)
                            .map(|b| *b as char)
                            .collect::<String>();
                        device = Some(EdgeDeviceInfo {
                            device_id: rec.device_id,
                            hardware_model: model,
                            firmware_version: rec.firmware_version,
                        });
                        break;
                    }
                }
            }
        }
    }
    EdgeHeaderInfo {
        stride: reader.tile_stride(),
        tile_count: reader.tile_count(),
        file_uuid: header.file_uuid,
        generation: header.generation,
        created_timestamp: header.created_timestamp,
        device,
    }
}

fn floor_from(
    index: u32,
    type_id: u32,
    record_count: u32,
    size_bytes: u32,
    payload: &[u8],
) -> Floor {
    let digest = Sha256::digest(payload);
    let payload_hash_short = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    Floor {
        index,
        type_id,
        record_count,
        size_bytes,
        payload_hash_short,
    }
}
