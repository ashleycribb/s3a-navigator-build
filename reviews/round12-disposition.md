# Round 12 disposition — s3a-kite SPEC v13

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round12.md` (copied to `reviews/goldfish_s3a-kite_round12.md`).
20 findings (0 blockers, 5 majors, 15 minors). Archived v12: `reviews/SPEC-v12.md`.

**Counts:** 20 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-142, DD-143, and DD-145 are unchanged (byte-identical to v12). DD-144 and DD-146 keep their v12 intent and are tightened (CAS / register-recheck-send; lexical walk and once-per-request fd). No new open questions. No invented numeric defaults.

## How majors were resolved

**F-01** (Edge valid-floor denial with no grant: no withholding rule) and **F-02** (hyper-tile "grant" / filter-mode undefined for expired or withdrawn trusted records). Stopped piecemeal patching. Replaced the hyper-tile and Edge consent rules (§5.3 item 4, §5.4 item 6, DD-67, DD-111) with **one** normative per-subject evaluation, recorded as **DD-147** and defined once in §5.3 item 4. Every other site references that table. A subject's data is shown only if the latest trusted record (on a valid, authenticated floor of a file that is not unauthenticated as a whole) for that subject is an unexpired grant **and** no parseable record from any tile (valid or invalid) for that subject is a later denial, withdrawal, or expiry. Any denial, withdrawal, or expiry from any parseable tile restricts; untrusted grants never release. The file is **unrestricted** only if authentication is trusted and there are zero consent records of any kind in any parseable tile; any tampering or untrusted state with consent records present is default-withhold. AT-23, AT-86, AT-161, AT-184, AT-185, and AT-200 follow the table: AT-184 Y is now withheld (evaluated mode; no trusted grant); AT-135 (only-non-research records) is no longer unrestricted. Added AT-204 (denial-only) and AT-205 (expiry-only). `AcademicPaperRecord` is mark-only (no `subject_id`).

**F-03** (latch atomicity / representation / pre-send TOCTOU). The latch is a pair of monotonic counters, `epoch` and `cleared_epoch` (inhibit iff unequal; `epoch` only increases). Revoke increments `epoch` on receipt, cancels in-flight POSTs through the current token, and swaps in a fresh token. A qualifying `on` clears only by compare-and-swap of `cleared_epoch` to the epoch it observed, and only if `epoch` still equals that value. Every sender (1) registers with the current token, (2) re-checks, then (3) sends. Tightened DD-144. AT-198 is black-box only (observable host and disk results; no internal latch assertions). F-12 is folded into that rewrite.

**F-04** (no pack accessor for `KiteVerificationReport`; AT-165 untestable; mapping circular). The pack exposes `verification_report(handle) -> Result<KiteVerificationReport, KiteError>` for an open archive handle. After `open` / `open_path` the pack returns the report it computed (including valid floors in `0 .. tiles_total-1`). After `open_verified` it returns the supplied report, not a recomputation. Phone UniFFI exports the call (AT-99). AT-165 compares `verification_report` after pack `open` to a mapped reader-core `verify`. The mapping's `invalid_tile_indices` is defined in reader-core terms (union of torn, CRC-failed, uncommitted, and at-or-after-`break_index` indices; all indices for `signer_not_pinned` / `missing_signature`).

**F-05** (`tile_count` vs `tiles_total`). One range is used everywhere: `0 .. tiles_total-1`, where `tiles_total` is every index `MmapReader` enumerates, including torn and uncommitted tiles, and may differ from the header `tile_count`. The pack does not use `tile_count` for `invalid_tile_indices`. Named in DD-130.

## Minors

**F-06.** Fixed: the `drive_path` fd is opened and validated once per request, before step 0, outside the mutex (`O_NONBLOCK` on the final component). The source-table row uses that result. Mid-copy cap overflow is `download_error: "source_changed"`, inclusion kept, temp deleted, no mid-request fetch switch.

**F-07.** Fixed: request paths are lexically normalized (a `..` that would leave the root is `path_escape`; in-root `..` is allowed), then walked from the root dirfd with `openat(O_NOFOLLOW)`. `ELOOP` / `ENOTDIR` from a symlink is `path_escape` for a request path. The working archive is opened via the canonical parent dirfd plus `openat(O_NOFOLLOW)`. Tightened DD-146.

**F-08.** Fixed: reserved names are `--data-dir/{.lock, inclusion-index.jsonl, revoke-pending, lrs-retry/}` and the sidecar `*.lock` files. A `--consent-state-file` or `--working-archive` path equal to or inside a reserved name is a startup error naming both flags (AT-208).

**F-09.** Fixed: the create-time `ConsentRecord` is written on `create` of the **working archive only**. Snapshots get no `ConsentRecord` and are encoded as BLOB + PROVENANCE (AT-107).

**F-10.** Fixed: `AcademicPaperRecord` has no `subject_id` in the grounded layout; it is mark-only, plus the Quarantined-corpus rule. AT-161 no longer omits an identity from `GET /v1/candidates` by ConsentRecord expiry. AT-166 asserts marks / Quarantine, not ConsentRecord withholding.

**F-11.** Fixed: for a handle opened from bytes, `Link` `href` is the raw attribute with `#fragment` stripped, not a resolved path (AT-206).

**F-12.** Fixed: folded into the AT-198 black-box rewrite (F-03).

**F-13.** Fixed: an aborted or skipped replay leaves `{id}.claimed` for a revoke delete and performs no rename. If `lrs-retry/` is already gone, the replay does nothing and must not recreate that directory.

**F-14.** Fixed: if any snapshot exists for `(file_uuid, identity)`, download delivers from it; a later-usable `drive_path` is not copied in its place.

**F-15.** Fixed: a repeat select or download of a recorded snapshot runs the DD-103 reuse checks. Failure is `200` with `snapshot: null` and `snapshot_error: "snapshot_invalid"`, inclusion kept, no automatic re-fetch and no `drive_path` fallback (AT-209).

**F-16.** Fixed: `--lrs-url` must be `scheme://host[:port]` with an optional single trailing `/` that is stripped. Any other path, userinfo, query, or fragment is a startup error naming `--lrs-url`.

**F-17.** Fixed: every route uses one global order: auth → 405 → 413/415 → `viewer_only` → JSON/param → route-specific. A JSON `null` equals omission for optional fields; a required `null` is `missing_param`. `/v1/frame` and `/v1/hit-test` keep their existing route-specific tail.

**F-18.** Fixed: DD-101 now names `KiteContext::new` (there is no v1 `register_ua_fonts`). DD-94 points at that. DD-74 defers floor/alias hit-test rules to DD-125. DD-99 lists `tiles_crc_failed`.

**F-19.** Fixed: `paper_identity` is `None` for the losing (`corpus_root`) copy of a wiki-root-winning dossier (AT-207).

**F-20.** Fixed: for Edge, a tile is torn or uncommitted when its index is ≥ header `tile_count`, or the header is absent, is erased (`0xFF` fill), or fails validation, or the tile is a partial trailing tile. A committed CRC failure is index < `tile_count`, header validates, payload CRC fails. `torn_tile_indices` uses this predicate, not `tile_at` `Err`.

## Acceptance-test audit (test-by-test)

### Changed

| AT | Finding / change |
| --- | --- |
| AT-23 | Lone valid-floor Edge denial with no grant withholds event-log payloads (F-01). |
| AT-83 | Repeat download uses a snapshot that passed reuse checks; no second fetch even if `drive_path` is usable (F-14, F-15). |
| AT-85 | Unrestricted only with zero consent records of any kind and trusted auth (F-01, F-02). |
| AT-90 | Seal-failed withholding cites the DD-147 table, not "filter mode" (F-02). |
| AT-99 | Phone UniFFI exports `verification_report` (F-04). |
| AT-107 | Snapshot unrestricted only with zero consent records and trusted auth; no create-time ConsentRecord (F-09). |
| AT-112 | Unrestricted only with zero consent records of any kind (F-02). |
| AT-135 | Only-non-research ConsentRecords are not unrestricted; evaluated-mode withhold (F-02). |
| AT-137 | `verification_report` after `open_verified` returns the supplied report (F-04). |
| AT-161 | Expiry withholds under the table; `AcademicPaperRecord` is not omitted by ConsentRecord expiry (F-02, F-10). |
| AT-165 | Compares `verification_report(handle)` after pack `open` to a mapped reader-core `verify` (F-04, F-05). |
| AT-166 | `AcademicPaperRecord` withheld by marks / Quarantine, not ConsentRecord (F-10). |
| AT-184 | Subject Y is withheld (evaluated mode; no trusted grant) (F-02). |
| AT-185 | After tamper, no grant is trusted; withdrawal still restricts (F-02). |
| AT-198 | Black-box only: raced `on` returns `off`, GET/file are `off`, no LRS and no retry file; no internal latch assertions (F-03, F-12). |
| AT-200 | Intact and post-tamper Y are withheld under evaluated / default-withhold; cites DD-147 (F-02). |
| AT-203 | Working archive opened via parent dirfd + `openat(O_NOFOLLOW)` (F-07). |

### Added

| AT | Finding |
| --- | --- |
| AT-204 | Hyper-tile lone valid-floor withdrawal/denial-only withholds X and Y (F-01, F-02). |
| AT-205 | Lone expired grant / expiry-only withholds; pre-expiry shows X (F-02). |
| AT-206 | Bytes-opened handle: `Link` href is the raw attribute with fragment stripped (F-11). |
| AT-207 | `paper_identity` is `None` for the losing wiki/corpus duplicate (F-19). |
| AT-208 | Reserved `--data-dir` / sidecar name collision is a startup error (F-08). |
| AT-209 | Repeat select of a snapshot that fails DD-103 reuse checks returns `snapshot_error: "snapshot_invalid"`; no re-fetch (F-15). |

### Removed

None.

Passed unchanged: AT-01–AT-22, AT-24–AT-82, AT-84, AT-86–AT-89, AT-91–AT-98, AT-100–AT-106, AT-108–AT-111, AT-113–AT-134, AT-136, AT-138–AT-160, AT-162–AT-164, AT-167–AT-183, AT-186–AT-197, AT-199, AT-201, AT-202.

Full AT inventory: AT-01–AT-209, 209 tests defined, no gaps, no duplicate definitions, no references to undefined AT numbers.

## Contract range

Acceptance tests: AT-01–AT-209.
Draft decisions: DD-01–DD-147 (DD-63 retired; DD-126 unused). New: DD-147 (single per-subject consent table). Tightened with intent kept: DD-144, DD-146.
Open questions: Q-01–Q-17 (Q-03 answered; no new question).
