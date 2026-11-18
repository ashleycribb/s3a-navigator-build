//! Research-consent evaluation (§5.3 item 4 / DD-147). This table is the only
//! withholding rule for research consent, for hyper-tile and Edge alike.
//!
//! Grounded record shapes (pin 2cf2b45):
//! - Hyper-tile research records are core `ConsentRecord`s (tile 71, 136 bytes)
//!   with `purpose == Research` (1). Grant: `withdrawn_at == 0` and
//!   (`expires_at == 0` or `now <= expires_at`). Withdrawal: `withdrawn_at != 0`.
//!   Expiry: `withdrawn_at == 0`, `expires_at != 0`, `now > expires_at`. There is
//!   no hyper-tile "denial" record kind; a withdrawal is the negative event.
//!   Non-research `ConsentRecord`s count for the unrestricted test but do not
//!   enter the per-subject table.
//! - Edge research records are type-71 `EdgeConsentRecord`s (64 bytes). Grant:
//!   `(allowed_purposes & PURPOSE_RESEARCH_ANALYTICS) != 0` and unexpired.
//!   Denial: any type-71 record that does not allow that purpose. Expiry:
//!   allows the purpose but `expires_at != 0` and `now > expires_at`.
//! - Only `ConsentRecord` carries a `subject_id` in the grounded registry, so
//!   hyper-tile subject-bearing payload records are exactly the consent records
//!   (any purpose). `AcademicPaperRecord` and `HumanLrsRecord` have no
//!   `subject_id` and are mark-only. Edge type-71 records apply to all 32-byte
//!   type-64 event-log payloads as one research-purpose class.
//! - Type 82 is never consent (the `tour.rs` type-82 branch is a known bug).

use std::collections::{HashMap, HashSet};

use s3a_core::consent::{ConsentPurpose, ConsentRecord};
use s3a_core::{MarkRecord, TileType, MARK_CONSENT_WITHDRAWN, MARK_TARGET_ALL_RECORDS};
use s3a_edge::{EdgeConsentRecord, PURPOSE_RESEARCH_ANALYTICS, TILE_TYPE_CONSENT};
use s3a_engine::MmapReader;

use crate::report::KiteVerificationReport;
use crate::verify::unauthenticated_as_a_whole;

/// Classification of one parseable research record at evaluation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentClass {
    Grant,
    Withdrawal,
    Expiry,
    /// Edge only: a type-71 record that does not allow the research purpose.
    Denial,
}

/// The outcome of the DD-147 evaluation, recomputed on every render against
/// the handle's clock (AT-205).
#[derive(Debug, Clone)]
pub struct ConsentEval {
    /// Trusted authentication AND zero consent records of any kind in any
    /// parseable tile (AT-112, AT-85, AT-107).
    pub unrestricted: bool,
    /// DD-148 predicate applied to the report.
    pub unauthenticated_as_a_whole: bool,
    /// Hyper-tile per-subject decision: true = this subject's data is shown.
    pub subject_shown: HashMap<[u8; 16], bool>,
    /// Edge research-class decision: true = event-log payloads are shown.
    pub edge_shown: bool,
    /// Per-file withheld count (valid floors only, except no-valid-floors
    /// counts parseable records anywhere).
    pub withheld_file: u32,
    /// Per-floor withheld counts, keyed by tile index (valid floors only).
    pub withheld_floor: HashMap<u32, u32>,
    /// (tile_index, record_index) pairs withheld by a MARK_CONSENT_WITHDRAWN
    /// mark (hyper-tile only). A whole-tile mark withholds every record on
    /// the tile; those pairs are materialised here.
    pub mark_withheld: HashSet<(u32, u32)>,
    /// Number of consent records of any kind in any parseable tile.
    pub consent_records_any_kind: u32,
}

impl ConsentEval {
    /// Is the record at (tile_index, record_index) on a hyper-tile consent
    /// tile shown? (Subject shown and not mark-withheld.)
    pub fn consent_record_shown(&self, tile_index: u32, record_index: u32, subject: &[u8; 16]) -> bool {
        if self.mark_withheld.contains(&(tile_index, record_index)) {
            return false;
        }
        self.subject_shown.get(subject).copied().unwrap_or(false)
    }
}

struct ResearchRecord {
    /// Walk order position (tile_index, record_index).
    order: (u32, u32),
    /// Hyper-tile subject, or None for the Edge single class.
    subject: Option<[u8; 16]>,
    class: ConsentClass,
    trusted: bool,
}

pub struct EvalInputs<'a> {
    pub reader: &'a MmapReader,
    pub report: &'a KiteVerificationReport,
    /// Highest authentic type-70 tile_index, if any (Edge, pack-computed open).
    /// None under open_verified (the supplied floor list already encodes the
    /// unsigned tail) and when no authentic record exists.
    pub highest_authentic: Option<u32>,
    /// A type-70 record exists in the file (Edge, pack-computed open).
    pub has_seal_records: bool,
    pub now: i64,
}

/// Evaluate the DD-147 table for one render. `now` is Unix UTC seconds.
pub fn evaluate(inputs: &EvalInputs) -> ConsentEval {
    let reader = inputs.reader;
    let report = inputs.report;
    let is_edge = reader.is_edge_archive();
    let unauth = unauthenticated_as_a_whole(report, !is_edge);
    let invalid: HashSet<u32> = report.invalid_tile_indices.iter().copied().collect();
    let tiles_total = reader.tile_count();
    let valid_floors: Vec<u32> = (0..tiles_total).filter(|i| !invalid.contains(i)).collect();
    let has_valid_floors = !valid_floors.is_empty();

    // Trusted(tile): valid floor, file not unauthenticated as a whole, and
    // (Edge with type-70 records) covered by an authentic record.
    let trusted = |idx: u32| -> bool {
        if invalid.contains(&idx) || unauth {
            return false;
        }
        if is_edge && inputs.has_seal_records {
            match inputs.highest_authentic {
                Some(h) => idx <= h,
                None => false,
            }
        } else {
            true
        }
    };

    let mut research: Vec<ResearchRecord> = Vec::new();
    let mut consent_any_kind: u32 = 0;
    // (tile_index, record_index, subject) for every parseable consent record
    // of any purpose (hyper-tile), for counting.
    let mut consent_payloads: Vec<(u32, u32, [u8; 16])> = Vec::new();
    // Edge event-log payload counts per tile (32-byte type-64 records only).
    let mut edge_event_counts: HashMap<u32, u32> = HashMap::new();
    // Marks, in walk order: (target_tile_type, target_tile_id, record_index,
    // set_or_clear). Only MARK_CONSENT_WITHDRAWN withholds.
    let mut marks: Vec<(u32, u64, u32, u8)> = Vec::new();
    // (tile_index -> tile_id) for mark matching, from unverified headers.
    let mut tile_ids: HashMap<u32, u64> = HashMap::new();
    // (tile_index -> (type_id, record_count)) for mark materialisation.
    let mut tile_meta: HashMap<u32, (u32, u32)> = HashMap::new();

    for idx in 0..tiles_total {
        if is_edge {
            let Ok((header, payload)) = reader.try_get_edge_tile_unverified(idx as usize) else {
                continue;
            };
            tile_meta.insert(idx, (header.type_id as u32, header.record_count));
            if header.type_id == TILE_TYPE_CONSENT {
                let sz = std::mem::size_of::<EdgeConsentRecord>();
                let n = payload.len() / sz;
                consent_any_kind += n as u32;
                for r in 0..n {
                    let rec: EdgeConsentRecord =
                        bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                    let class = edge_class(&rec, inputs.now);
                    research.push(ResearchRecord {
                        order: (idx, r as u32),
                        subject: None,
                        class,
                        trusted: trusted(idx),
                    });
                }
            } else if header.type_id == s3a_edge::TILE_TYPE_RECORD {
                // 32-byte event-log payloads only; 96-byte identity records
                // are exempt (DD-53) and other sizes are unknown layout.
                if header.record_count > 0 {
                    let sz = payload.len() / header.record_count as usize;
                    if sz == 32 {
                        edge_event_counts.insert(idx, header.record_count);
                    }
                }
            }
        } else {
            let Ok((header, payload)) = reader.try_get_tile_unverified(idx as usize) else {
                continue;
            };
            tile_ids.insert(idx, header.tile_id);
            tile_meta.insert(idx, (header.tile_type, header.record_count));
            if header.tile_type == TileType::CONSENT {
                let sz = std::mem::size_of::<ConsentRecord>();
                let n = payload.len() / sz;
                consent_any_kind += n as u32;
                for r in 0..n {
                    let rec: ConsentRecord =
                        bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                    consent_payloads.push((idx, r as u32, rec.subject_id));
                    if rec.purpose == ConsentPurpose::Research as u32 {
                        let class = hyper_class(&rec, inputs.now);
                        research.push(ResearchRecord {
                            order: (idx, r as u32),
                            subject: Some(rec.subject_id),
                            class,
                            trusted: trusted(idx),
                        });
                    }
                }
            } else if header.tile_type == TileType::MARKS {
                let sz = std::mem::size_of::<MarkRecord>();
                let n = payload.len() / sz;
                for r in 0..n {
                    let m: MarkRecord =
                        bytemuck::pod_read_unaligned(&payload[r * sz..(r + 1) * sz]);
                    if m.mark_kind == MARK_CONSENT_WITHDRAWN {
                        marks.push((
                            m.target_tile_type,
                            m.target_tile_id,
                            m.record_index,
                            m.set_or_clear,
                        ));
                    }
                }
            }
        }
    }

    let unrestricted = !unauth && consent_any_kind == 0;

    // Per-subject / per-class decision.
    let mut subject_shown: HashMap<[u8; 16], bool> = HashMap::new();
    let mut edge_shown = false;
    if is_edge {
        edge_shown = class_shown(&research, None, unrestricted);
    } else {
        // Subjects with research records, plus subjects that only have
        // non-research records (withheld: no trusted research grant).
        let mut subjects: HashSet<[u8; 16]> = HashSet::new();
        for r in &research {
            if let Some(s) = r.subject {
                subjects.insert(s);
            }
        }
        for (_, _, s) in &consent_payloads {
            subjects.insert(*s);
        }
        for s in subjects {
            subject_shown.insert(s, class_shown(&research, Some(s), unrestricted));
        }
    }

    // Marks: latest per (type, tile_id, record-or-whole) wins; record-specific
    // beats whole-tile (engine is_marked rule). Marks are read from parseable
    // tiles (DD-111: marks from parseable invalid tiles can restrict).
    // Materialise whole-tile marks into per-record pairs using tile_meta.
    let mut mark_withheld: HashSet<(u32, u32)> = HashSet::new();
    if !is_edge && !marks.is_empty() {
        // (target_tile_type, target_tile_id) -> (latest record-specific state
        // per record_index, latest whole-tile state).
        let mut record_state: HashMap<(u32, u64, u32), bool> = HashMap::new();
        let mut tile_state: HashMap<(u32, u64), bool> = HashMap::new();
        for (tt, tid, ridx, set) in &marks {
            let is_set = *set != 0;
            if *ridx == MARK_TARGET_ALL_RECORDS {
                tile_state.insert((*tt, *tid), is_set);
            } else {
                record_state.insert((*tt, *tid, *ridx), is_set);
            }
        }
        // For each tile, resolve each record.
        for (idx, (type_id, record_count)) in &tile_meta {
            let Some(tid) = tile_ids.get(idx) else {
                continue;
            };
            let whole = tile_state.get(&(*type_id, *tid)).copied();
            for r in 0..*record_count {
                let specific = record_state.get(&(*type_id, *tid, r)).copied();
                let withheld = specific.or(whole).unwrap_or(false);
                if withheld {
                    mark_withheld.insert((*idx, r));
                }
            }
        }
    }

    // Withheld counts. Scope: valid floors only, except no-valid-floors
    // counts parseable records anywhere.
    let count_tiles: HashSet<u32> = if has_valid_floors {
        valid_floors.iter().copied().collect()
    } else {
        (0..tiles_total).collect()
    };
    let mut withheld_file: u32 = 0;
    let mut withheld_floor: HashMap<u32, u32> = HashMap::new();
    if is_edge {
        if !edge_shown {
            for (idx, n) in &edge_event_counts {
                if count_tiles.contains(idx) {
                    withheld_file += *n;
                    *withheld_floor.entry(*idx).or_insert(0) += *n;
                }
            }
        }
    } else {
        // Table-withheld consent records.
        for (idx, _r, subject) in &consent_payloads {
            let shown = subject_shown.get(subject).copied().unwrap_or(false);
            if !shown && count_tiles.contains(idx) {
                withheld_file += 1;
                *withheld_floor.entry(*idx).or_insert(0) += 1;
            }
        }
        // Mark-withheld records of any type (count once each).
        for (idx, r) in &mark_withheld {
            if !count_tiles.contains(idx) {
                continue;
            }
            // If this record is a consent record already counted as
            // table-withheld, do not double-count.
            let already = if tile_meta.get(idx).map(|(t, _)| *t) == Some(TileType::CONSENT) {
                // Find its subject: consent_payloads has (idx, r, subject).
                consent_payloads
                    .iter()
                    .find(|(i, ri, _)| i == idx && ri == r)
                    .map(|(_, _, s)| !subject_shown.get(s).copied().unwrap_or(false))
                    .unwrap_or(false)
            } else {
                false
            };
            if !already {
                withheld_file += 1;
                *withheld_floor.entry(*idx).or_insert(0) += 1;
            }
        }
    }

    ConsentEval {
        unrestricted,
        unauthenticated_as_a_whole: unauth,
        subject_shown,
        edge_shown,
        withheld_file,
        withheld_floor,
        mark_withheld,
        consent_records_any_kind: consent_any_kind,
    }
}

fn hyper_class(rec: &ConsentRecord, now: i64) -> ConsentClass {
    if rec.withdrawn_at != 0 {
        ConsentClass::Withdrawal
    } else if rec.expires_at != 0 && now > rec.expires_at as i64 {
        ConsentClass::Expiry
    } else {
        ConsentClass::Grant
    }
}

fn edge_class(rec: &EdgeConsentRecord, now: i64) -> ConsentClass {
    let allows = (rec.allowed_purposes & PURPOSE_RESEARCH_ANALYTICS) != 0;
    if !allows {
        ConsentClass::Denial
    } else if rec.expires_at != 0 && now > rec.expires_at as i64 {
        ConsentClass::Expiry
    } else {
        ConsentClass::Grant
    }
}

/// The per-subject (hyper-tile) or per-class (Edge, subject None) decision:
/// shown only if the latest trusted record is an unexpired grant AND no
/// later parseable denial/withdrawal/expiry exists (any trust). In
/// unrestricted mode the table withholds nothing.
fn class_shown(
    research: &[ResearchRecord],
    subject: Option<[u8; 16]>,
    unrestricted: bool,
) -> bool {
    if unrestricted {
        return true;
    }
    let mine: Vec<&ResearchRecord> = research
        .iter()
        .filter(|r| r.subject == subject)
        .collect();
    if mine.is_empty() {
        return false;
    }
    // Latest trusted record (walk order = tile index, then record index).
    let latest_trusted = mine.iter().filter(|r| r.trusted).last();
    let Some(lt) = latest_trusted else {
        return false;
    };
    if lt.class != ConsentClass::Grant {
        return false;
    }
    // No later parseable denial / withdrawal / expiry (any trust).
    for r in &mine {
        if r.order > lt.order {
            match r.class {
                ConsentClass::Withdrawal | ConsentClass::Expiry | ConsentClass::Denial => {
                    return false;
                }
                ConsentClass::Grant => {}
            }
        }
    }
    true
}
