//! Step-8b fixture builders (built at test time, not checked in).
//!
//! Shared helpers: not every test binary uses every helper, so dead-code
//! warnings are silenced here.

#![allow(dead_code)]
//!
//! Hyper-tile: `TileWriter::create` + `write_hyper_tile` /
//! `write_raw_hyper_tile` + `seal_archive_v2`. Edge: `EdgeWriter` +
//! `RamFlash` + `SigningKey` / `sign_seal_hash` (pattern from
//! `s3a-spec-vectors/tests/seal_and_edge_accessors.rs`, `signed_edge_archive`).
//!
//! Ground truths used here (pin 2cf2b45):
//! - `TileWriter::write_hyper_tile` returns the tile_id; writer-built
//!   archives have tile_id = tile_index + 1. Marks therefore target the
//!   `tile_id` returned here (engine `is_marked` matches
//!   `header.tile_id == target_tile_id`; `tour.rs` compares against the tile
//!   *index* instead — a known divergence, logged in the step-8b log).
//! - No engine code sets `FLAG_ED25519_SIGNED`; fixtures that need it patch
//!   the flag into both header slots and recompute the header CRC.
//! - A seal-hash mismatch (AT-220) is built by editing a payload byte and
//!   recomputing the tile's payload SHA-256, data CRC, and header CRC, so the
//!   tile passes CRC but its stored `seal_hash` no longer equals the
//!   recomputed `compute_seal_hash` (signatures still verify over the stored
//!   stale hash).
//! - A seal-trailer tamper flips a byte inside the endcap `index_hash`,
//!   keeping the `S3AEND` magic so the file reads as seal_failed, not
//!   unsealed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use s3a_core::consent::{ConsentPurpose, ConsentRecord};
use s3a_core::{HyperTileHeader, MarkRecord, TileType};
use s3a_edge::{
    compute_common_tile_header_crc32, compute_file_header_crc32, CommonTileHeader,
    DeviceIdentityRecord, DeviceSealRecord, EdgeConsentRecord, EdgeWriter, EquipmentEventRecord,
    RamFlash, SigningKey, FLAG_ED25519_SIGNED, TILE_TYPE_SEAL,
};
use s3a_engine::{seal_archive_v2, SealOptionsV2, TileWriter};
use sha2::{Digest, Sha256};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn tmp_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "kite8b_{}_{}_{}.s3a",
        std::process::id(),
        n,
        name
    ))
}

/// Deterministic subject ids: 0x5a, n, ..., n.
pub fn subject(n: u8) -> [u8; 16] {
    let mut s = [0u8; 16];
    s[0] = 0x5a;
    s[1] = n;
    s[15] = n;
    s
}

/// Distinctive operator id for one event-log record (unique string = hex).
pub fn operator(n: u8) -> [u8; 8] {
    [0x0eu8, n, 0, 0, 0, 0, 0, n]
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A research-purpose grant for `subject`, unexpired at `now`.
pub fn grant(subject_id: [u8; 16], granted_at: u64, expires_at: u64) -> ConsentRecord {
    ConsentRecord::new(
        subject_id,
        0,
        ConsentPurpose::Research,
        0,
        granted_at,
        expires_at,
        false,
        1,
        [0u8; 32],
    )
}

/// A research-purpose grant with a non-research purpose.
pub fn grant_purpose(
    subject_id: [u8; 16],
    purpose: ConsentPurpose,
    granted_at: u64,
    expires_at: u64,
) -> ConsentRecord {
    ConsentRecord::new(
        subject_id, 0, purpose, 0, granted_at, expires_at, false, 1, [0u8; 32],
    )
}

/// A research-purpose withdrawal for `subject`.
pub fn withdrawal(subject_id: [u8; 16], withdrawn_at: u64) -> ConsentRecord {
    ConsentRecord::new_withdrawal(
        subject_id,
        0,
        ConsentPurpose::Research,
        0,
        withdrawn_at,
        1,
        [0u8; 32],
    )
}

// ---------------------------------------------------------------- hyper-tile

pub enum HyperTile {
    Consent(Vec<ConsentRecord>),
    Marks(Vec<MarkRecord>),
    Manifest(String),
    Provenance(String),
    /// Raw payload with an explicit record_count (BLOB, type 82, ...).
    Raw(u32, Vec<u8>, u32),
    /// Raw payload with a caller-built header (e.g. a v2 record_layout_version).
    RawWithHeader(HyperTileHeader, Vec<u8>),
    /// Enable an addon bit (e.g. ADDON_EDU) before the following tiles.
    Addon(u32),
}

pub struct HyperFixture {
    pub bytes: Vec<u8>,
    /// tile_id per tile index (writer assigns tile_id = index + 1).
    pub tile_ids: Vec<u64>,
}

pub fn build_hyper(tiles: Vec<HyperTile>, seal: bool) -> HyperFixture {
    let raw_path = tmp_path("hyper_raw");
    let mut w = TileWriter::create(&raw_path).expect("TileWriter::create");
    let mut tile_ids = Vec::new();
    for tile in &tiles {
        if let HyperTile::Addon(mask) = tile {
            w.enable_addon(*mask).expect("enable addon");
            continue;
        }
        let id = match tile {
            HyperTile::Consent(recs) => w
                .write_hyper_tile(TileType::CONSENT, recs, None, None)
                .expect("write consent tile"),
            HyperTile::Marks(recs) => w
                .write_hyper_tile(TileType::MARKS, recs, None, None)
                .expect("write marks tile"),
            HyperTile::Manifest(text) => w
                .write_hyper_tile(TileType::MANIFEST, text.as_bytes(), None, None)
                .expect("write manifest tile"),
            HyperTile::Provenance(text) => w
                .write_hyper_tile(TileType::PROVENANCE, text.as_bytes(), None, None)
                .expect("write provenance tile"),
            HyperTile::RawWithHeader(h, payload) => {
                w.write_raw_hyper_tile(h, payload)
                    .expect("write raw tile with header")
            }
            HyperTile::Raw(type_id, payload, record_count) => {
                let mut h = HyperTileHeader::new(0, *type_id);
                h.record_count = *record_count;
                w.write_raw_hyper_tile(&h, payload)
                    .expect("write raw tile")
            }
            HyperTile::Addon(_) => unreachable!("addon handled above"),
        };
        tile_ids.push(id);
    }
    drop(w);
    let bytes = if seal {
        let sealed_path = tmp_path("hyper_sealed");
        seal_archive_v2(&raw_path, &sealed_path, &SealOptionsV2::default())
            .expect("seal_archive_v2");
        std::fs::read(&sealed_path).expect("read sealed")
    } else {
        std::fs::read(&raw_path).expect("read raw")
    };
    HyperFixture { bytes, tile_ids }
}

/// Flip one payload byte of hyper-tile `tile_index` (breaks its data CRC).
pub fn flip_hyper_payload_byte(bytes: &mut [u8], tile_index: usize, offset: usize) {
    let off = 512 + tile_index * 131072 + 512 + offset;
    assert!(off < bytes.len(), "flip offset {off} out of range");
    bytes[off] ^= 0xff;
}

/// Tamper the seal trailer so `verify_sealed_bytes` fails (AT-185/AT-200).
/// Flips a byte inside the endcap `index_hash` (keeping the `S3AEND` magic)
/// AND recomputes the endcap CRC32C over the first 60 endcap bytes, so the
/// verifier reaches the index-hash check and fails with
/// `SealedIndexHashMismatch` instead of rebuilding the directory by
/// scanning (which is what a bare endcap-CRC break does).
pub fn tamper_seal_trailer(bytes: &mut [u8]) {
    assert!(bytes.len() >= 64, "file too short for an endcap");
    let start = bytes.len() - 64;
    bytes[start + 30] ^= 0xff;
    let crc = crc32c(&bytes[start..start + 60]);
    bytes[start + 60..start + 64].copy_from_slice(&crc.to_le_bytes());
}

/// Fill one hyper-tile header with 0xFF (erased header = torn, not CRC).
pub fn erase_hyper_tile_header(bytes: &mut [u8], tile_index: usize) {
    let off = 512 + tile_index * 131072;
    assert!(off + 512 <= bytes.len(), "erase offset out of range");
    for b in &mut bytes[off..off + 512] {
        *b = 0xFF;
    }
}

// ---------------------------------------------------------------------- Edge

pub struct EdgeBuilder {
    w: EdgeWriter<RamFlash<65536>, 4096>,
    sk: SigningKey,
    pk: [u8; 32],
    device_id: [u8; 16],
    /// Seal-chain hash per tile index, filled as tiles seal.
    chain: Vec<[u8; 32]>,
    ts: u64,
}

impl EdgeBuilder {
    /// Fresh builder: identity tile 0 sealed; `sk` from seed byte 0x42.
    pub fn new() -> Self {
        Self::new_with_seed(0x42)
    }

    /// Fresh builder with NO identity record (AT-68a, AT-201 case 7).
    pub fn new_bare() -> Self {
        Self::new_bare_with_seed(0x42)
    }

    pub fn new_bare_with_seed(seed: u8) -> Self {
        let device_id = [0x11u8; 16];
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let pk = sk.verifying_key().to_bytes();
        let flash = RamFlash::<65536>::new(256, 512);
        let w = EdgeWriter::<_, 4096>::open_or_format(
            flash,
            device_id,
            [0x1111_2222_3333_4444, 0x5555_6666_7777_8888],
            1_700_000_000,
        )
        .expect("EdgeWriter::open_or_format");
        Self {
            w,
            sk,
            pk,
            device_id,
            chain: Vec::new(),
            ts: 1_700_000_010,
        }
    }

    pub fn new_with_seed(seed: u8) -> Self {
        let device_id = [0x11u8; 16];
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let pk = sk.verifying_key().to_bytes();
        let flash = RamFlash::<65536>::new(256, 512);
        let mut w = EdgeWriter::<_, 4096>::open_or_format(
            flash,
            device_id,
            [0x1111_2222_3333_4444, 0x5555_6666_7777_8888],
            1_700_000_000,
        )
        .expect("EdgeWriter::open_or_format");
        let id = DeviceIdentityRecord::new(device_id, "test-model", [1, 0, 0, 0], pk, 1_700_000_000, 1);
        w.append_device_identity(&id)
            .expect("append device identity");
        let seal = w.seal_tile(1_700_000_001).expect("seal identity tile");
        Self {
            w,
            sk,
            pk,
            device_id,
            chain: vec![seal],
            ts: 1_700_000_010,
        }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.pk
    }

    pub fn device_id(&self) -> [u8; 16] {
        self.device_id
    }

    pub fn tile_count(&self) -> u32 {
        self.w.tile_count()
    }

    pub fn w_generation(&self) -> u64 {
        self.w.generation()
    }

    /// Append a type-70 tile holding one already-built seal record; returns
    /// the container index. (AT-201 case 9: a record for an out-of-range
    /// tile_index.)
    pub fn append_raw_seal_record(&mut self, rec: &DeviceSealRecord) -> u32 {
        let idx = self.w.tile_count();
        self.w
            .append_record(TILE_TYPE_SEAL, bytemuck::bytes_of(rec))
            .expect("append raw seal record");
        let ts = self.next_ts();
        let seal = self.w.seal_tile(ts).expect("seal container tile");
        self.chain.push(seal);
        idx
    }

    pub fn seal_hash(&self, tile_index: u32) -> [u8; 32] {
        self.chain[tile_index as usize]
    }

    fn next_ts(&mut self) -> u64 {
        let t = self.ts;
        self.ts += 10;
        t
    }

    /// Append one tile of 32-byte event-log records; returns its index.
    pub fn append_events(&mut self, events: Vec<EquipmentEventRecord>) -> u32 {
        let idx = self.w.tile_count();
        for e in &events {
            self.w.append_equipment_event(e).expect("append event");
        }
        let ts = self.next_ts();
        let seal = self.w.seal_tile(ts).expect("seal event tile");
        self.chain.push(seal);
        idx
    }

    /// Append one type-71 consent tile; returns its index.
    pub fn append_consent(&mut self, rec: EdgeConsentRecord) -> u32 {
        let idx = self.w.tile_count();
        self.w.append_consent(&rec).expect("append consent");
        let ts = self.next_ts();
        let seal = self.w.seal_tile(ts).expect("seal consent tile");
        self.chain.push(seal);
        idx
    }

    /// Append one type-64 identity tile; returns its index.
    pub fn append_identity(&mut self, rec: DeviceIdentityRecord) -> u32 {
        let idx = self.w.tile_count();
        self.w
            .append_device_identity(&rec)
            .expect("append identity");
        let ts = self.next_ts();
        let seal = self.w.seal_tile(ts).expect("seal identity tile");
        self.chain.push(seal);
        idx
    }

    /// Append one tile with a raw payload of `type_id`; returns its index.
    pub fn append_raw(&mut self, type_id: u16, payload: &[u8]) -> u32 {
        let idx = self.w.tile_count();
        self.w
            .append_record(type_id, payload)
            .expect("append raw record");
        let ts = self.next_ts();
        let seal = self.w.seal_tile(ts).expect("seal raw tile");
        self.chain.push(seal);
        idx
    }

    /// Append a type-70 container tile holding the seal record for
    /// `tile_index`, signed with this builder's key; returns the container
    /// index. The record's `prev_seal_hash` is the chain hash of the tile
    /// before the covered tile (zero for tile 0).
    pub fn append_seal_for(&mut self, tile_index: u32) -> u32 {
        let sk = self.sk.clone();
        self.append_seal_for_with_key(tile_index, &sk)
    }

    pub fn append_seal_for_with_key(&mut self, tile_index: u32, sk: &SigningKey) -> u32 {
        let idx = self.w.tile_count();
        let seal = self.chain[tile_index as usize];
        let prev = if tile_index == 0 {
            [0u8; 32]
        } else {
            self.chain[(tile_index - 1) as usize]
        };
        let sig = s3a_edge::sign_seal_hash(sk, &seal);
        let rec = DeviceSealRecord::new(tile_index, self.w.generation(), seal, prev, sig);
        self.w
            .append_record(TILE_TYPE_SEAL, bytemuck::bytes_of(&rec))
            .expect("append seal record");
        let ts = self.next_ts();
        let container_seal = self.w.seal_tile(ts).expect("seal container tile");
        self.chain.push(container_seal);
        idx
    }

    pub fn finish(self) -> Vec<u8> {
        let n = 512 + self.w.tile_count() as usize * 4096;
        self.w.into_storage().as_slice()[..n].to_vec()
    }

    /// Finish with `FLAG_ED25519_SIGNED` set in both header slots (the
    /// engine writer never sets it; recompute each slot's header CRC).
    pub fn finish_with_signed_flag(self) -> Vec<u8> {
        let mut bytes = self.finish();
        set_edge_signed_flag(&mut bytes);
        bytes
    }
}

/// Set `FLAG_ED25519_SIGNED` in both file-header slots, recomputing CRCs.
/// (Slots are 64 bytes: A at 0, B at 64; `FILE_HEADER_SLOT_SIZE` is 64.)
pub fn set_edge_signed_flag(bytes: &mut [u8]) {
    for off in [0usize, 64] {
        let mut h: s3a_edge::FileHeader =
            bytemuck::pod_read_unaligned(&bytes[off..off + 64]);
        h.flags |= FLAG_ED25519_SIGNED;
        h.header_crc32 = 0;
        h.header_crc32 = compute_file_header_crc32(&h);
        bytes[off..off + 64].copy_from_slice(bytemuck::bytes_of(&h));
    }
}

/// Flip one payload byte of Edge tile `tile_index` (breaks its data CRC).
pub fn flip_edge_payload_byte(bytes: &mut [u8], tile_index: usize, offset: usize) {
    let off = 512 + tile_index * 4096 + 64 + offset;
    assert!(off < bytes.len(), "flip offset {off} out of range");
    bytes[off] ^= 0xff;
}

/// Recompute one Edge tile's payload SHA-256, data CRC, and header CRC
/// in place (after the payload was edited). No payload byte is changed.
pub fn recompute_edge_tile_crcs(bytes: &mut [u8], tile_index: usize) {
    let tile_off = 512 + tile_index * 4096;
    let payload_off = tile_off + 64;
    let mut h: CommonTileHeader =
        bytemuck::pod_read_unaligned(&bytes[tile_off..tile_off + 64]);
    let payload_len = h.payload_len as usize;
    let payload = &bytes[payload_off..payload_off + payload_len];
    h.payload_hash = Sha256::digest(payload).into();
    h.data_crc32 = crc32c(payload);
    h.header_crc32 = 0;
    h.header_crc32 = compute_common_tile_header_crc32(&h);
    bytes[tile_off..tile_off + 64].copy_from_slice(bytemuck::bytes_of(&h));
}

/// Edit one payload byte of Edge tile `tile_index` and recompute that tile's
/// payload SHA-256, data CRC, and header CRC, so the tile passes CRC but its
/// stored `seal_hash` no longer equals the recomputed chain hash (AT-220).
pub fn edit_edge_tile_payload_recompute_crc(
    bytes: &mut [u8],
    tile_index: usize,
    offset: usize,
) {
    let tile_off = 512 + tile_index * 4096;
    let payload_off = tile_off + 64;
    bytes[payload_off + offset] ^= 0xff;
    recompute_edge_tile_crcs(bytes, tile_index);
}

/// Zero every Ed25519 signature in every type-70 tile and recompute those
/// tiles' CRCs (AT-195: signature-stripped file). The seal records stay
/// parseable, so attribution reads them as verifying under no key.
pub fn strip_edge_signatures(bytes: &mut [u8]) {
    let tile_count = ((bytes.len() - 512) / 4096) as u32;
    for i in 0..tile_count {
        let tile_off = 512 + i as usize * 4096;
        let h: CommonTileHeader =
            bytemuck::pod_read_unaligned(&bytes[tile_off..tile_off + 64]);
        if h.type_id != TILE_TYPE_SEAL {
            continue;
        }
        let payload_off = tile_off + 64;
        let payload_len = h.payload_len as usize;
        let rec_sz = std::mem::size_of::<DeviceSealRecord>();
        for r in 0..payload_len / rec_sz {
            let rec_off = payload_off + r * rec_sz;
            let rec: DeviceSealRecord =
                bytemuck::pod_read_unaligned(&bytes[rec_off..rec_off + rec_sz]);
            let fresh = DeviceSealRecord::new(
                rec.tile_index,
                rec.generation,
                rec.seal_hash,
                rec.prev_seal_hash,
                [0u8; 64],
            );
            bytes[rec_off..rec_off + rec_sz].copy_from_slice(bytemuck::bytes_of(&fresh));
        }
        recompute_edge_tile_crcs(bytes, i as usize);
    }
}

/// Clear `FLAG_SEAL_CHAIN_ACTIVE` in both file-header slots, recomputing
/// CRCs (AT-220: chain-inactive seal-hash mismatch).
pub fn set_edge_chain_inactive(bytes: &mut [u8]) {
    for off in [0usize, 64] {
        let mut h: s3a_edge::FileHeader =
            bytemuck::pod_read_unaligned(&bytes[off..off + 64]);
        h.flags &= !s3a_edge::FLAG_SEAL_CHAIN_ACTIVE;
        h.header_crc32 = 0;
        h.header_crc32 = compute_file_header_crc32(&h);
        bytes[off..off + 64].copy_from_slice(bytemuck::bytes_of(&h));
    }
}

/// Set both `FLAG_TILE_SIZE_4KB` and `FLAG_TILE_SIZE_8KB` in both
/// file-header slots, recomputing CRCs (AT-97/AT-211: inconsistent header
/// -> unsupported bytes).
pub fn set_inconsistent_header_flags(bytes: &mut [u8]) {
    for off in [0usize, 64] {
        let mut h: s3a_edge::FileHeader =
            bytemuck::pod_read_unaligned(&bytes[off..off + 64]);
        h.flags |= s3a_edge::FLAG_TILE_SIZE_4KB | s3a_edge::FLAG_TILE_SIZE_8KB;
        h.header_crc32 = 0;
        h.header_crc32 = compute_file_header_crc32(&h);
        bytes[off..off + 64].copy_from_slice(bytemuck::bytes_of(&h));
    }
}

fn crc32c(data: &[u8]) -> u32 {
    // CRC32C (Castagnoli) via the crc32c crate re-exported through s3a-edge.
    s3a_edge::crc32c(data)
}

/// Assert a withheld string is absent from a rendered frame: text runs,
/// semantic labels, and paint liveness. 8c form: the pixmap is really
/// painted now (rect-fill painter), so the all-zero check from 8b is
/// replaced — every visible text-run rect must contain painted
/// (non-background) pixels, proving paint happened. Absence-from-pixels
/// for a withheld value then rests on text absence plus painter purity,
/// which the dedicated two-archive §5.8 oracle tests in step8c_paint.rs
/// prove (AT-11, AT-23, AT-67, AT-232). A run whose centre falls outside
/// the pixmap (scrolled off) is skipped, not failed.
pub fn assert_absent_from_frame(frame: &s3a_pack_kite::RenderFrame, needle: &str) {
    for run in &frame.text_runs {
        assert!(
            !run.text.contains(needle),
            "withheld string {needle} present in text run: {}",
            run.text
        );
    }
    for node in &frame.semantics {
        assert!(
            !node.label.contains(needle),
            "withheld string {needle} present in semantic label: {}",
            node.label
        );
    }
    // Paint liveness: every visible run rect contains a painted pixel.
    // TextRun rects are pixmap coordinates (§5.8). A painted pixel is
    // neither background white nor the all-zero of an unpainted pixmap
    // (either one alone would pass vacuously before paint lands).
    let w = frame.width as usize;
    let h = frame.height as usize;
    for run in &frame.text_runs {
        let cx = run.x as usize + run.width as usize / 2;
        let cy = run.y as usize + run.height as usize / 2;
        if cx >= w || cy >= h {
            continue;
        }
        let idx = (cy * w + cx) * 4;
        let px = &frame.rgba[idx..idx + 4];
        assert!(
            px != [255, 255, 255, 255] && px != [0, 0, 0, 0],
            "text run {:?} at ({cx}, {cy}) has no painted pixel (paint missing?)",
            run.text
        );
    }
}

/// Assert a string IS present in a frame's text runs or semantic labels.
pub fn assert_present_in_frame(frame: &s3a_pack_kite::RenderFrame, needle: &str) {
    let in_text = frame.text_runs.iter().any(|r| r.text.contains(needle));
    let in_sem = frame.semantics.iter().any(|n| n.label.contains(needle));
    assert!(
        in_text || in_sem,
        "expected {needle} in frame text/semantics; text: {:?}",
        frame
            .text_runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
    );
}
pub fn event(timestamp_sec: u64, event_type: u16, severity: u8, op: u8) -> EquipmentEventRecord {
    EquipmentEventRecord::new(
        timestamp_sec,
        event_type,
        severity,
        1,
        100 + op as u32,
        operator(op),
        0,
    )
}

/// A type-71 Edge consent record.
pub fn edge_consent(allowed_purposes: u32, granted_at: u64, expires_at: u64) -> EdgeConsentRecord {
    EdgeConsentRecord::new([0x22u8; 16], allowed_purposes, 0, granted_at, expires_at, 7)
}
