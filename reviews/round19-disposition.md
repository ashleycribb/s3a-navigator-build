# Round 19 disposition — s3a-kite SPEC v20

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round19.md` (copied to `reviews/goldfish_s3a-kite_round19.md`).
15 findings (0 blockers, 1 major, 14 minors). Archived v19: `reviews/SPEC-v19.md`.

**Counts:** 15 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-143, DD-147, and DD-148 are unchanged (byte-identical to v19). DD-111, DD-145, DD-146, DD-149, and DD-150 keep their prior intent and are tightened (coverage scoped to Edge files with type-70 records; `chain_break` in the attribution list; unpinned embedded-key hash mismatch is `signer_not_pinned`; normalize-then-unique-root; config/state list matches §5.2.2; inclusion index not capped by `--max-open-bytes`; v20 is not frozen). DD-61, DD-106, DD-121, and DD-125 are also tightened to the same rules. New DD-151 records the test-hook mechanism. No new open questions. No invented numeric defaults (the inclusion index has no rotation bound; `--max-fetch-bytes` ≤ `--max-open-bytes` is a consistency check between two already-required flags).

This v20 is not frozen (DD-149): round 19 still had a major.

## How the major was resolved

**F-01** (many fail-first ATs depended on "a defined test hook" with no mechanism). New §5.2.7 / DD-151:

1. Cargo feature `test_hooks`, not in the default set, compiled out of release builds (same rule as `egress_test`).
2. Control channel is environment variables, not flags (DD-106 stays complete): `S3A_KITE_TEST_HOOKS` is a comma-separated `POINT=ACTION` list. Actions are the closed set `pause_until_signal` (READY/CONTINUE absolute paths), `fail_with_errno=N`, `grow` (only `grow_drive_path_during_copy`), and `issue` (only `window_issue_select`).
3. A closed hook-point list is mapped to the ATs that use one: `pause_before_snapshot_rename` (AT-187), `pause_after_devino_check` (AT-219), `fail_revoke_pending_write` (AT-189), `fail_lrs_retry_delete` (AT-124), `fail_index_append` (AT-158), `fail_snapshot_write_{temp,fsync,rename}` (AT-212), `fail_snapshot_index_append` (AT-213), `fail_download_write` (AT-214), `grow_drive_path_during_copy` (AT-202 mid-copy), `window_issue_select` (AT-125). Those ATs now name their hook point.
4. Reclassified as no host hook: AT-101 mid-call swap lives in `kite-pack-tests` (open fd, swap path, call `open_path`); AT-202 symlink/FIFO, AT-203 post-start swap, AT-217 oversize/symlink/FIFO, and AT-218 FIFO/symlink are ordinary filesystem operations between requests.
5. New AT-222: a build without `test_hooks` (including `--release`) rejects `S3A_KITE_TEST_HOOKS` / READY / CONTINUE at startup, binds nothing, and performs no hooked action; a `test_hooks` build rejects an unknown point or a malformed value.

## Minors

**F-02.** Fixed: §5.4 rule 2, DD-111, DD-150, and §4.7 prefix the coverage rule with "In an Edge file that contains at least one type-70 `DeviceSealRecord`". Added: "In a hyper-tile, or an Edge file with no type-70 records, a valid floor of a file that is not unauthenticated as a whole is trusted."

**F-03.** Fixed: the label-table row is "Seal-chain break with `FLAG_SEAL_CHAIN_ACTIVE` set … `seal_failed`". Added a row "Seal-hash mismatch with chain inactive … Quarantined / `unsealed`".

**F-04.** Fixed: a well-typed optional field that a known view does not use is ignored; a malformed one is `400` `invalid_param` (catalogue paragraph, DD-125, already AT-171 / DD-69).

**F-05.** Fixed: a record that fails every pinned key and verifies under an embedded key is `signer_not_pinned` whether or not its stored hash matches (`signer_not_pinned` outranks `chain_break`). Added as AT-201 case (10). Tightened DD-145.

**F-06.** Fixed: `chain_break` is in DD-145's outcome list.

**F-07.** Fixed: DD-146 now says a lexically normalized path that lies under no allowed root, or under more than one, is `path_escape`. AT-52 adds a cross-root `{wiki}/../{corpus}/...` case that opens the corpus file.

**F-08.** Fixed: DD-146 points at the §5.2.2 list verbatim (`--auth-token-file`, `--lrs-credentials-file`, `--pinned-keys-file`, `--consent-state-file`, `--tls-cert`, `--tls-key`, and the inclusion index). The typo `--lrs-credential-file` is gone.

**F-09.** Fixed: AT-201 case (8) keeps only the authentic-at-3 / chain_break-at-4 / signature_known_signer-at-7 fixture (`break_index` 4; every index ≥ 4 invalid).

**F-10.** Fixed: silent skip means the candidate walk writes nothing to `--log-dir`. AT-24, AT-25, and AT-29 say that; AT-25 also says `POST /v1/skip` itself never writes a log line.

**F-11.** Fixed: the host library and binary live in `crates/s3a-kite`, excluded from the s3a-storage workspace (same rule as `kite-pack-tests`). Until the pack exists the crate has no pack dependency, so AT-183 lands in that crate's tests in §11 step 1. Tightened DD-121.

**F-12.** Fixed: AT-169 and AT-181 require a non-root runner for the non-writable-parent injection, or skip and report the skip if the process is running as root (same sentence as AT-171).

**F-13.** Fixed: an oversize existing download-dir file is `409` `download_conflict` (AT-216, AT-223). The inclusion index is opened under the shared safe-open rule but is **not** subject to `--max-open-bytes`; no rotation bound is invented (AT-224). Scholar start with `--max-fetch-bytes` > `--max-open-bytes` is a startup error that names both flags, so a successfully fetched body always fits the hash-check cap (AT-223). Tightened DD-61 and DD-146.

**F-14.** Fixed: startup order is (1) canonicalize and `lstat` every path and run DD-93 / DD-129 / reserved-name / pairwise-distinct checks, with no creation; (2) take sidecar locks; (3) any other read, cleanup, or bootstrap. Step (1) is the only exception to "locks first". AT-208 asserts that a colliding start creates no `.lock` file.

**F-15.** Fixed: new AT-225 covers duplicate-`paper_id` dedupe (first sorted `.s3a` path, then first record in tile order). AT-110 now asserts `EXT_CONTENT_SHA256` presence/absence, pre-redirect `EXT_SOURCE_URL` / `object.id`, and the no-`source_url` fallback IRI. New AT-226 is a parameterized deny-list over CGNAT, site-local, NAT64/6to4, and `metadata.google.internal`, plus LRS-host re-resolution at every connect and redirect hop. `/v1/open` of the not-yet-created working-archive path is `404` `not_found` (path rule and AT-53).

## AT audit (v19 → v20)

- **Defined range:** AT-01–AT-226 (226 tests, no gaps).
- **Changed (26):** AT-24, AT-25, AT-29, AT-52, AT-53, AT-101, AT-110, AT-124, AT-125, AT-158, AT-169, AT-181, AT-183, AT-187, AT-189, AT-201, AT-202, AT-203, AT-208, AT-212, AT-213, AT-214, AT-216, AT-217, AT-218, AT-219.
- **Added (5):** AT-222 (release builds reject hook env vars), AT-223 (fetch vs open cap; oversize download_conflict), AT-224 (inclusion index larger than `--max-open-bytes` still starts), AT-225 (duplicate `paper_id` dedupe), AT-226 (parameterized deny-list literals and LRS re-resolution).
- **Removed:** none.

## New and tightened decisions / questions

- **New:** DD-151 (test hooks).
- **Tightened:** DD-61, DD-106, DD-111, DD-121, DD-125, DD-145, DD-146, DD-149, DD-150.
- No new Qs.
