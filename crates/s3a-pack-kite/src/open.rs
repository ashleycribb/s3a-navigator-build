use std::cell::RefCell;

use s3a_core::{
    FileHeader, TileType, FILE_HEADER_SLOT_A_OFFSET, FILE_HEADER_SLOT_B_OFFSET,
    FILE_HEADER_SLOT_SIZE, FLAG_PROFILE_EDGE, FLAG_TILE_SIZE_4KB, FLAG_TILE_SIZE_8KB,
    HYPER_TILE_SIZE, S3A_MAGIC,
};
use s3a_edge::FLAG_SEAL_CHAIN_ACTIVE;
use s3a_engine::{
    probe_seal_footer, verify_header_slot, verify_sealed_bytes, CorruptReason, MmapReader,
};
use sha2::{Digest, Sha256};

use crate::render::{cache_frame, last_frame, RenderFrame};
use crate::report::KiteVerificationReport;
use crate::types::{FirstView, Floor, Hit, Kind, ManifestState};
use crate::{KiteContext, KiteError};

const VALID_STRIDES: &[u32] = &[0, 4096, 8192, HYPER_TILE_SIZE as u32];

pub struct Handle {
    kind: Kind,
    first_view: FirstView,
    max_frame_pixels: u64,
    inner: RefCell<HandleInner>,
}

struct HandleInner {
    report: KiteVerificationReport,
    floors: Vec<Floor>,
    manifest: ManifestState,
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
        self.inner.borrow().report.clone()
    }

    pub(crate) fn floors(&self) -> Vec<Floor> {
        self.inner.borrow().floors.clone()
    }

    pub(crate) fn manifest(&self) -> ManifestState {
        self.inner.borrow().manifest.clone()
    }
}

pub fn open(
    _ctx: &KiteContext,
    bytes: &[u8],
    name_hint: Option<String>,
    max_frame_pixels: u64,
    max_open_bytes: u64,
    _pinned_keys: Vec<[u8; 32]>,
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

    if reader.is_edge_archive() {
        Ok(open_edge(max_frame_pixels, &reader))
    } else {
        Ok(open_hyper_tile(max_frame_pixels, bytes, &reader))
    }
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
    let kind = handle.kind_enum();
    if kind == Kind::Unsupported {
        if view == "banner" || view == "manifest" {
            return Ok(Hit::None);
        }
        return Err(KiteError::new(
            "unsupported",
            format!("hit_test view {view} is not valid for kind unsupported"),
        ));
    }
    // Pure: look at the last frame's semantics if any; otherwise None.
    if let Some(frame) = handle
        .frame(view, floor)
        .or_else(|| handle.frame(view, None))
    {
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
        inner: RefCell::new(HandleInner {
            report: KiteVerificationReport::unsigned_unsealed(),
            floors: Vec::new(),
            manifest: ManifestState::Unsupported { banner },
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

fn open_hyper_tile(max_frame_pixels: u64, bytes: &[u8], reader: &MmapReader) -> Handle {
    let (report, floors, manifest) = inspect_hyper_tile(bytes, reader);
    Handle {
        kind: Kind::HyperTile,
        first_view: FirstView::Manifest,
        max_frame_pixels,
        inner: RefCell::new(HandleInner {
            report,
            floors,
            manifest,
            proceeded: false,
            last_frames: Vec::new(),
        }),
    }
}

fn open_edge(max_frame_pixels: u64, reader: &MmapReader) -> Handle {
    let (report, floors) = inspect_edge(reader);
    let tile_count = reader.tile_count();
    let stride = reader.tile_stride();
    let header_text =
        format!("derived header sheet\nstride {stride}\ncommitted tile_count {tile_count}\n");
    Handle {
        kind: Kind::Edge,
        first_view: FirstView::Header,
        max_frame_pixels,
        inner: RefCell::new(HandleInner {
            report,
            floors,
            manifest: ManifestState::Text(header_text),
            proceeded: false,
            last_frames: Vec::new(),
        }),
    }
}

fn inspect_hyper_tile(
    bytes: &[u8],
    reader: &MmapReader,
) -> (KiteVerificationReport, Vec<Floor>, ManifestState) {
    let count = reader.tile_count() as usize;
    let mut torn = Vec::new();
    let mut crc_failed = Vec::new();
    let mut stop: Option<u32> = None;
    let mut floors = Vec::new();
    let mut first_manifest: Option<(bool, Vec<u8>)> = None;

    for i in 0..count {
        if stop.is_some() {
            break;
        }
        match reader.try_get_tile(i) {
            Ok((header, payload)) => {
                let idx = i as u32;
                floors.push(floor_from(
                    idx,
                    header.tile_type,
                    header.record_count,
                    HYPER_TILE_SIZE as u32,
                    payload,
                ));
                if header.tile_type == TileType::MANIFEST && first_manifest.is_none() {
                    let len = (header.payload_bytes as usize).min(payload.len());
                    first_manifest = Some((true, payload[..len].to_vec()));
                }
            }
            Err(err) => {
                let idx = err.tile as u32;
                match err.reason {
                    CorruptReason::DataCrcMismatch { .. }
                    | CorruptReason::HeaderCrcMismatch { .. } => {
                        crc_failed.push(idx);
                        stop = Some(idx);
                    }
                    _ => {
                        torn.push(idx);
                        stop = Some(idx);
                    }
                }
            }
        }
    }

    let sealed = verify_sealed_bytes(bytes);
    let mut report = KiteVerificationReport::unsigned_unsealed();
    match sealed {
        Ok(_) => {
            report.seal_status = "sealed".into();
            report.status = "unsigned".into();
        }
        Err(_) if probe_seal_footer(bytes).has_endcap_magic => {
            report.seal_status = "seal_failed".into();
            report.status = "quarantined".into();
            report.quarantine_reason = Some("seal_trailer".into());
            if !crc_failed.is_empty() {
                report.quarantine_reason = Some("crc".into());
            }
        }
        Err(_) => {
            report.seal_status = "unsealed".into();
            report.status = "unsigned".into();
        }
    }
    if !crc_failed.is_empty() {
        report.status = "quarantined".into();
        if report.quarantine_reason.is_none() {
            report.quarantine_reason = Some("crc".into());
        }
        // CRC rule: failed tile and every subsequent tile are not floors.
        floors.retain(|f| f.index < crc_failed[0]);
    }
    if let Some(s) = stop {
        report.break_index = Some(s);
        for i in s..count as u32 {
            report.invalid_tile_indices.push(i);
        }
    }
    report.tiles_crc_failed = crc_failed;
    report.torn_tile_indices = torn;
    // Torn-only does not quarantine (AT-17 analogue for hyper-tile / DD-100).

    let manifest = match first_manifest {
        Some((_, bytes)) => match String::from_utf8(bytes) {
            Ok(s) => ManifestState::Text(s),
            Err(_) => ManifestState::NotUtf8,
        },
        None => ManifestState::Absent,
    };
    (report, floors, manifest)
}

fn inspect_edge(reader: &MmapReader) -> (KiteVerificationReport, Vec<Floor>) {
    let count = reader.tile_count() as usize;
    let stride = reader.tile_stride() as u32;
    let mut torn = Vec::new();
    let mut crc_failed = Vec::new();
    let mut stop: Option<u32> = None;
    let mut floors = Vec::new();

    for i in 0..count {
        if stop.is_some() {
            break;
        }
        match reader.try_get_edge_tile(i) {
            Ok((header, payload)) => {
                let idx = i as u32;
                // Type-70 seal tiles may still be floors when they are valid
                // (AT-210 is later); include CRC-valid tiles as floors.
                floors.push(floor_from(
                    idx,
                    header.type_id as u32,
                    header.record_count,
                    stride,
                    payload,
                ));
            }
            Err(err) => {
                let idx = err.tile as u32;
                match err.reason {
                    CorruptReason::DataCrcMismatch { .. }
                    | CorruptReason::HeaderCrcMismatch { .. } => {
                        crc_failed.push(idx);
                        stop = Some(idx);
                    }
                    CorruptReason::SlotPastEof { .. } | CorruptReason::IndexOutOfRange => {
                        torn.push(idx);
                        stop = Some(idx);
                    }
                    _ => {
                        torn.push(idx);
                        stop = Some(idx);
                    }
                }
            }
        }
    }

    let mut report = KiteVerificationReport::unsigned_unsealed();
    let flags = reader.file_header().flags;
    if flags & FLAG_SEAL_CHAIN_ACTIVE != 0 {
        if let Some(id) = reader.edge_device_identity() {
            let chain = reader.edge_seal_chain(&id.device_id);
            if chain.len() < count && crc_failed.is_empty() && torn.is_empty() {
                // Chain stopped early without a torn/crc classification already.
                report.status = "quarantined".into();
                report.quarantine_reason = Some("chain_break".into());
            }
        }
    }
    if !crc_failed.is_empty() {
        report.status = "quarantined".into();
        report.quarantine_reason = Some("crc".into());
        if flags & FLAG_SEAL_CHAIN_ACTIVE != 0 {
            report.seal_status = "seal_failed".into();
        }
        floors.retain(|f| f.index < crc_failed[0]);
    }
    if let Some(s) = stop {
        report.break_index = Some(s);
        for i in s..count as u32 {
            report.invalid_tile_indices.push(i);
        }
    }
    report.tiles_crc_failed = crc_failed;
    report.torn_tile_indices = torn;
    // Type-70 records are listed when they are valid CRC tiles; AT-18 cares
    // that a mutated tile and subsequent tiles are omitted.
    (report, floors)
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
