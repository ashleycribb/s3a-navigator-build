//! Verification derivation: DD-145 signer attribution + DD-150 report table
//! (SPEC §5.4). This is the single total derivation of `status`, `seal_status`,
//! `quarantine_reason`, and `break_index` for hyper-tile and Edge files, used
//! identically by pack `open` / `open_path` (DD-115, DD-150).
//!
//! Grounded in `s3a-engine` (`MmapReader`, `edge_seal_chain`,
//! `edge_seal_records`, `edge_device_identity`) and `s3a-edge`
//! (`compute_seal_hash` via the chain, `verify_signature_raw`).
//! The pack never treats a public key embedded in the file as a trust anchor
//! when the pin list is non-empty (DD-79, DD-98).

use s3a_core::{FILE_HEADER_SIZE, HYPER_TILE_SIZE};
use s3a_edge::{
    DeviceIdentityRecord, FLAG_ED25519_SIGNED, FLAG_SEAL_CHAIN_ACTIVE, TILE_TYPE_RECORD,
    TILE_TYPE_SEAL,
};
use s3a_engine::{probe_seal_footer, verify_sealed_bytes, CorruptReason, MmapReader};

use crate::report::KiteVerificationReport;

/// What `open` computed, beyond the report itself: the pieces the consent
/// evaluator (§5.3 item 4 / DD-147) needs.
#[derive(Debug, Clone)]
pub struct DerivedVerification {
    pub report: KiteVerificationReport,
    /// Highest `tile_index` of an authentic type-70 record, if any (Edge only).
    pub highest_authentic: Option<u32>,
    /// A type-70 record exists in the file (Edge only).
    pub has_seal_records: bool,
    /// Indices in the unsigned tail (AT-210): greater than the highest authentic
    /// signed `tile_index`, except verifying type-70 containers.
    pub unsigned_tail: Vec<u32>,
    pub tiles_total: u32,
}

impl DerivedVerification {
    /// The unsigned-tail indices (AT-210), if any, for banner text.
    pub(crate) fn unsigned_tail_list(&self) -> Option<&[u32]> {
        if self.unsigned_tail.is_empty() {
            None
        } else {
            Some(&self.unsigned_tail)
        }
    }
}

/// Quarantine-reason priority (DD-150): signer_not_pinned > missing_signature >
/// seal_trailer > signature_known_signer > chain_break > crc.
fn reason_priority(reason: &str) -> u8 {
    match reason {
        "signer_not_pinned" => 6,
        "missing_signature" => 5,
        "seal_trailer" => 4,
        "signature_known_signer" => 3,
        "chain_break" => 2,
        "crc" => 1,
        _ => 0,
    }
}

/// Classify one tile read failure as CRC vs torn (DD-100 / §5.4 per-tile
/// outcomes). An erased (0xFF-filled) header is torn, not a CRC failure; a
/// committed CRC failure requires a header that validates.
fn classify_failure(reader: &MmapReader, index: usize, edge: bool) -> TileFault {
    // Erased header bytes (0xFF fill) are torn, even though their CRC fails.
    let erased = if edge {
        match reader.try_get_edge_tile_unverified(index) {
            Ok(_) => false,
            Err(e) => {
                // The unverified accessor fails before returning a header when
                // the slot is past EOF; that is torn as well.
                matches!(
                    e.reason,
                    CorruptReason::SlotPastEof { .. } | CorruptReason::IndexOutOfRange
                )
            }
        }
    } else {
        match reader.try_get_tile_unverified(index) {
            Ok(_) => false,
            Err(e) => {
                matches!(
                    e.reason,
                    CorruptReason::SlotPastEof { .. } | CorruptReason::IndexOutOfRange
                )
            }
        }
    };
    if erased {
        return TileFault::Torn;
    }
    // A header that is present but entirely 0xFF-filled is torn. Read the raw
    // slot header bytes through the unverified accessor's payload start: the
    // tile header is the first bytes of the slot. We approximate by re-reading
    // the unverified tile and checking the header bytes via the payload slice
    // start is not possible here; instead check the error reason of the
    // verified read.
    let err = if edge {
        reader.try_get_edge_tile(index).err()
    } else {
        reader.try_get_tile(index).err()
    };
    match err.map(|e| e.reason) {
        Some(
            CorruptReason::DataCrcMismatch { .. } | CorruptReason::HeaderCrcMismatch { .. },
        ) => TileFault::Crc,
        _ => TileFault::Torn,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TileFault {
    Crc,
    Torn,
}

/// Read the raw header bytes of one tile slot, if the slot is in range, to
/// test for an erased (0xFF) header. Returns true when the slot header is
/// entirely 0xFF.
fn slot_header_erased(bytes: &[u8], reader: &MmapReader, index: usize, edge: bool) -> bool {
    // Slot layout: hyper-tile slots start at FILE_HEADER_SIZE (512) with
    // stride 128 KiB; Edge slots start at header_extent with the file stride.
    // The common tile header is 64 bytes; the hyper-tile header is larger,
    // but an erased header is 0xFF for at least its first 64 bytes either way.
    let (start, stride) = if edge {
        (reader.header_extent(), reader.tile_stride())
    } else {
        (FILE_HEADER_SIZE, HYPER_TILE_SIZE)
    };
    let off = start + index * stride;
    if off + 64 > bytes.len() {
        return false;
    }
    bytes[off..off + 64].iter().all(|b| *b == 0xFF)
}

pub fn derive_verification(
    bytes: &[u8],
    reader: &MmapReader,
    pinned_keys: &[[u8; 32]],
) -> DerivedVerification {
    if reader.is_edge_archive() {
        derive_edge(bytes, reader, pinned_keys)
    } else {
        derive_hyper_tile(bytes, reader)
    }
}

fn derive_hyper_tile(bytes: &[u8], reader: &MmapReader) -> DerivedVerification {
    let tiles_total = reader.tile_count();
    let mut crc_failed: Vec<u32> = Vec::new();
    let mut torn: Vec<u32> = Vec::new();
    for i in 0..tiles_total as usize {
        if reader.try_get_tile(i).is_ok() {
            continue;
        }
        if slot_header_erased(bytes, reader, i, false) {
            torn.push(i as u32);
        } else {
            match classify_failure(reader, i, false) {
                TileFault::Crc => crc_failed.push(i as u32),
                TileFault::Torn => torn.push(i as u32),
            }
        }
    }

    let mut report = KiteVerificationReport::unsigned_unsealed();
    let mut break_index: Option<u32> = None;
    let mut reason: Option<&str> = None;

    // Seal trailer.
    let seal_failed = match verify_sealed_bytes(bytes) {
        Ok(_) => {
            report.seal_status = "sealed".into();
            report.status = "unsigned".into();
            false
        }
        Err(_) if probe_seal_footer(bytes).has_endcap_magic => {
            report.seal_status = "seal_failed".into();
            report.status = "quarantined".into();
            reason = Some("seal_trailer");
            true
        }
        Err(_) => {
            report.seal_status = "unsealed".into();
            report.status = "unsigned".into();
            false
        }
    };
    let _ = seal_failed;

    if !crc_failed.is_empty() {
        report.status = "quarantined".into();
        // Priority: seal_trailer outranks crc (DD-150).
        if reason.is_none() {
            reason = Some("crc");
        }
        if report.seal_status != "seal_failed" {
            // A committed CRC with a trailer present is seal_failed; without
            // a trailer it stays unsealed (AT-35).
            report.seal_status = if probe_seal_footer(bytes).has_endcap_magic {
                "seal_failed".into()
            } else {
                "unsealed".into()
            };
        }
        break_index = Some(crc_failed[0]);
    }
    if let Some(t) = torn.iter().min().copied() {
        // A torn-only stop is not a chain break and does not quarantine by
        // itself; break_index is still the first torn tile (DD-150).
        break_index = Some(match break_index {
            Some(b) => b.min(t),
            None => t,
        });
    }

    // Valid floors end before break_index (CRC rule decides floors, AT-35).
    let floor_end = break_index.unwrap_or(tiles_total);
    let mut invalid: Vec<u32> = Vec::new();
    for i in floor_end..tiles_total {
        invalid.push(i);
    }
    // Torn/CRC indices at or after the stop are already in the invalid range
    // (they are >= first stop by construction); indices before the stop are
    // CRC-ok by definition of first stop.

    report.tiles_crc_failed = crc_failed;
    report.torn_tile_indices = torn;
    report.invalid_tile_indices = invalid;
    report.break_index = break_index;
    report.quarantine_reason = reason.map(str::to_string);
    report.detail = format!(
        "hyper-tile: {} tiles, status {}, seal {}",
        tiles_total, report.status, report.seal_status
    );
    DerivedVerification {
        report,
        highest_authentic: None,
        has_seal_records: false,
        unsigned_tail: Vec::new(),
        tiles_total,
    }
}

fn derive_edge(
    bytes: &[u8],
    reader: &MmapReader,
    pinned_keys: &[[u8; 32]],
) -> DerivedVerification {
    let tiles_total = reader.tile_count();
    let flags = reader.file_header().flags;
    let chain_active = flags & FLAG_SEAL_CHAIN_ACTIVE != 0;
    let signed_flag = flags & FLAG_ED25519_SIGNED != 0;

    let mut crc_failed: Vec<u32> = Vec::new();
    let mut torn: Vec<u32> = Vec::new();
    for i in 0..tiles_total as usize {
        if reader.try_get_edge_tile(i).is_ok() {
            continue;
        }
        if slot_header_erased(bytes, reader, i, true) {
            torn.push(i as u32);
        } else {
            match classify_failure(reader, i, true) {
                TileFault::Crc => crc_failed.push(i as u32),
                TileFault::Torn => torn.push(i as u32),
            }
        }
    }

    // --- Signer attribution (DD-145) ---
    let seal_entries = reader.edge_seal_records();
    let has_seal_records = !seal_entries.is_empty();
    let identity = reader.edge_device_identity();
    let embedded_keys = all_embedded_keys(reader);
    let device_id = identity.map(|id| id.device_id);
    let chain: Vec<[u8; 32]> = match device_id {
        Some(id) => reader.edge_seal_chain(&id),
        None => Vec::new(),
    };

    // Candidate keys: pinned list if non-empty, else every embedded key.
    let candidate_keys: Vec<[u8; 32]> = if !pinned_keys.is_empty() {
        pinned_keys.to_vec()
    } else {
        embedded_keys.clone()
    };

    let mut records_sorted: Vec<(u32, u32, s3a_edge::DeviceSealRecord)> = seal_entries
        .iter()
        .map(|e| (e.record.tile_index, e.container_tile, e.record))
        .collect();
    records_sorted.sort_by_key(|(idx, _, _)| *idx);

    let mut highest_authentic: Option<u32> = None;
    let mut authentic_before: Vec<u32> = Vec::new();
    // (reason, break_index) causes from attribution.
    let mut attr_causes: Vec<(&'static str, Option<u32>)> = Vec::new();
    // Per-record authenticity, for the container exemption (AT-210).
    let mut authentic_by_container: Vec<(u32, bool)> = Vec::new();

    for (tile_index, container, rec) in &records_sorted {
        let verifies_pinned = pinned_keys
            .iter()
            .any(|k| s3a_edge::verify_signature_raw(k, &rec.seal_hash, &rec.signature));
        let verifies_embedded = embedded_keys
            .iter()
            .any(|k| s3a_edge::verify_signature_raw(k, &rec.seal_hash, &rec.signature));
        let verifies_any_candidate = candidate_keys
            .iter()
            .any(|k| s3a_edge::verify_signature_raw(k, &rec.seal_hash, &rec.signature));
        let hash_ok = chain
            .get(*tile_index as usize)
            .map(|h| h == &rec.seal_hash)
            .unwrap_or(false);

        if verifies_any_candidate && hash_ok {
            // Authentic. Covers 0..=tile_index.
            highest_authentic = Some(match highest_authentic {
                Some(h) => h.max(*tile_index),
                None => *tile_index,
            });
            authentic_before.push(*tile_index);
            authentic_by_container.push((*container, true));
            continue;
        }
        authentic_by_container.push((*container, false));

        // Failing record. highest preceding authentic tile_index.
        let preceding = authentic_before.iter().max().copied();
        let brk = preceding.map(|s| s + 1).unwrap_or(0);

        if !pinned_keys.is_empty() {
            if !verifies_pinned && verifies_embedded {
                // Verifies only under an embedded unpinned key: whole file,
                // no valid floors; outranks chain_break (DD-145 rule 3).
                attr_causes.push(("signer_not_pinned", None));
            } else if verifies_pinned && !hash_ok {
                attr_causes.push(("chain_break", Some(brk)));
            } else if !verifies_pinned && !verifies_embedded {
                if preceding.is_some() {
                    attr_causes.push(("signature_known_signer", Some(brk)));
                } else {
                    // Attribution impossible: no earlier authentic record.
                    attr_causes.push(("signer_not_pinned", None));
                }
            } else {
                // verifies_pinned && hash_ok would have been authentic.
                // verifies_embedded only + hash mismatch is signer_not_pinned
                // (handled above). Unreachable otherwise.
                attr_causes.push(("chain_break", Some(brk)));
            }
        } else {
            // Empty pin list (DD-145 rule 4).
            if candidate_keys.is_empty() {
                attr_causes.push(("missing_signature", None));
            } else if verifies_embedded && !hash_ok {
                attr_causes.push(("chain_break", Some(brk)));
            } else if !verifies_embedded {
                if preceding.is_some() {
                    attr_causes.push(("signature_known_signer", Some(brk)));
                } else {
                    attr_causes.push(("missing_signature", None));
                }
            } else {
                attr_causes.push(("chain_break", Some(brk)));
            }
        }
    }

    // DD-145 rule 5: FLAG_ED25519_SIGNED set and (no type-70 tile or no usable
    // candidate key) is missing_signature.
    if signed_flag && (!has_seal_records || candidate_keys.is_empty()) {
        attr_causes.push(("missing_signature", None));
    }

    // --- Combine causes (DD-150) ---
    let mut reason: Option<&str> = None;
    let mut break_index: Option<u32> = None;
    let mut push_reason = |r: &'static str, b: Option<u32>| {
        if reason.map_or(true, |cur| reason_priority(r) > reason_priority(cur)) {
            reason = Some(r);
        }
        if let Some(b) = b {
            break_index = Some(match break_index {
                Some(cur) => cur.min(b),
                None => b,
            });
        }
    };
    for (r, b) in &attr_causes {
        push_reason(r, *b);
    }
    if !crc_failed.is_empty() {
        push_reason("crc", Some(crc_failed[0]));
    }
    if let Some(t) = torn.iter().min().copied() {
        // Torn-only stop contributes break_index but no quarantine reason.
        break_index = Some(match break_index {
            Some(b) => b.min(t),
            None => t,
        });
    }

    // --- Status / seal_status ---
    let quarantined = reason.is_some();
    // A chain_break cause anywhere (even outranked by signer_not_pinned) breaks
    // an active chain: seal_failed follows the cause, not the top reason.
    let has_chain_break = attr_causes.iter().any(|(r, _)| *r == "chain_break");
    let chain_broken_active = chain_active && (has_chain_break || !crc_failed.is_empty());
    let seal_status: &str = if chain_broken_active {
        "seal_failed"
    } else if chain_active {
        "sealed"
    } else {
        "unsealed"
    };
    let status = if quarantined {
        "quarantined"
    } else if !pinned_keys.is_empty() && highest_authentic.is_some() {
        "verified"
    } else if pinned_keys.is_empty() && highest_authentic.is_some() {
        "self_signed"
    } else {
        "unsigned"
    };

    // --- Floors ---
    let no_floors = reason == Some("signer_not_pinned") || reason == Some("missing_signature");
    // Unsigned tail (AT-210): indices greater than the highest authentic signed
    // tile_index, except a type-70 container whose records all verify.
    let mut unsigned_tail: Vec<u32> = Vec::new();
    if let Some(highest) = highest_authentic {
        for i in (highest + 1)..tiles_total {
            if is_verifying_seal_container(reader, i, &authentic_by_container) {
                continue;
            }
            unsigned_tail.push(i);
        }
    }

    let mut invalid: Vec<u32> = Vec::new();
    if no_floors {
        for i in 0..tiles_total {
            invalid.push(i);
        }
    } else {
        let floor_end = break_index.unwrap_or(tiles_total);
        for i in 0..tiles_total {
            if i >= floor_end || unsigned_tail.contains(&i) {
                invalid.push(i);
            }
        }
    }

    let mut report = KiteVerificationReport::unsigned_unsealed();
    report.status = status.into();
    report.seal_status = seal_status.into();
    report.tiles_crc_failed = crc_failed;
    report.torn_tile_indices = torn;
    report.invalid_tile_indices = invalid;
    report.break_index = break_index;
    report.quarantine_reason = reason.map(str::to_string);
    report.detail = format!(
        "edge: {} tiles, status {}, seal {}",
        tiles_total, report.status, report.seal_status
    );
    DerivedVerification {
        report,
        highest_authentic,
        has_seal_records,
        unsigned_tail,
        tiles_total,
    }
}

/// Every `public_key` from every `DeviceIdentityRecord` in the file (all
/// embedded keys; several identity records may disagree). Reads with the
/// unverified accessor so a record in a damaged tile is still seen; each
/// record must pass its own CRC.
fn all_embedded_keys(reader: &MmapReader) -> Vec<[u8; 32]> {
    let mut keys: Vec<[u8; 32]> = Vec::new();
    if !reader.is_edge_archive() {
        return keys;
    }
    let sz = std::mem::size_of::<DeviceIdentityRecord>();
    for i in 0..reader.tile_count() as usize {
        let Ok((h, payload)) = reader.try_get_edge_tile_unverified(i) else {
            continue;
        };
        if h.type_id != TILE_TYPE_RECORD || payload.len() < sz {
            continue;
        }
        // Only 96-byte records are DeviceIdentityRecords.
        if h.record_count == 0 || payload.len() / (h.record_count as usize) != sz {
            // Fall back to the first record when the layout divides evenly.
            if payload.len() % sz != 0 {
                continue;
            }
        }
        for chunk in payload.chunks_exact(sz) {
            let id: DeviceIdentityRecord = bytemuck::pod_read_unaligned(chunk);
            if id.verify() && id.device_id != [0u8; 16] && !keys.contains(&id.public_key) {
                keys.push(id.public_key);
            }
        }
    }
    keys
}

/// True when tile `index` is a type-70 container whose DeviceSealRecords all
/// verify (authentic). Such a container is exempt from the unsigned tail
/// (AT-210).
fn is_verifying_seal_container(
    reader: &MmapReader,
    index: u32,
    authentic_by_container: &[(u32, bool)],
) -> bool {
    let Ok((h, _)) = reader.try_get_edge_tile(index as usize) else {
        return false;
    };
    if h.type_id != TILE_TYPE_SEAL {
        return false;
    }
    let entries: Vec<bool> = authentic_by_container
        .iter()
        .filter(|(c, _)| *c == index)
        .map(|(_, ok)| *ok)
        .collect();
    !entries.is_empty() && entries.iter().all(|ok| *ok)
}

/// The DD-148 unauthenticated-as-a-whole predicate, applied to a
/// pack-computed or supplied report (§5.3 item 4). A file is unauthenticated
/// as a whole iff (hyper-tile and seal_status == seal_failed) or
/// quarantine_reason is signer_not_pinned or missing_signature.
pub fn unauthenticated_as_a_whole(report: &KiteVerificationReport, is_hyper_tile: bool) -> bool {
    if is_hyper_tile && report.seal_status == "seal_failed" {
        return true;
    }
    matches!(
        report.quarantine_reason.as_deref(),
        Some("signer_not_pinned") | Some("missing_signature")
    )
}
