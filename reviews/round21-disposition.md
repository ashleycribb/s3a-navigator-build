# Round 21 disposition — s3a-kite SPEC v22

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round21.md` (copied to `reviews/goldfish_s3a-kite_round21.md`).
19 findings (0 blockers, 3 majors, 16 minors). Archived v21: `reviews/SPEC-v21.md`.

**Counts:** 19 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-143, DD-147, DD-148, and DD-150 are unchanged (byte-identical to v21). DD-28, DD-61, DD-103, DD-112, DD-113, DD-121, DD-125, DD-140, DD-149, and DD-151 keep their prior intent and are tightened. New **DD-152** records Ashley's 2026-11-16 answer to Q-19. Q-19 is answered; no other new Q. No invented numeric defaults.

This v22 is not frozen (DD-149): round 21 still had majors.

## How the majors were resolved

**F-01** (startup-only create in DD-28 / DD-146 contradicted AT-79, AT-74, AT-53, the `write_failed` row, and §5.6). Kept the v21 startup-only model and made every site agree:

1. `TileWriter::create` (plus `enable_addon` and the create-time `ConsentRecord`) runs only at startup, on the startup `O_CREAT|O_EXCL` fd. `open_append` runs in the step-0/1 hold.
2. AT-79: the host creates the missing archive at startup; after `/health` the file exists; the first select appends.
3. AT-74 and the `write_failed` row: a running host that cannot **open** the path (missing, or identity changed) returns `500` `write_failed` and does not create mid-process.
4. AT-53 / path rule: the pre-create 404 is dropped. If the canonical working-archive path is missing mid-process, `/v1/open` is `404` `not_found` (the host does not create on open).
5. A failed startup create (parent not writable, `O_CREAT|O_EXCL` fails, `enable_addon` fails, or the create-time `ConsentRecord` write fails) is a startup error that names `--working-archive` and binds nothing (AT-78).

**F-02** (fetch-plus-hook ATs and AT-227 were assigned to builds that cannot fetch). Fixed in §7:

1. Any host-spawn case that needs a successful source fetch, or a loopback fetch fixture, uses an `egress_test` build.
2. AT-187, AT-212, AT-213, AT-214, AT-169, and AT-156 run on `S3A_KITE_BIN_EGRESS_TEST_HOOKS`. The "none is required today" sentence is deleted.
3. AT-227 (and AT-228) run on `S3A_KITE_BIN_EGRESS_TEST`.
4. The matrix lists the other fetch ATs under `S3A_KITE_BIN_EGRESS_TEST` (AT-26, 27, 31, 48, 82, 83, 102, 103, 106, 126, 157, 180, 186, 209, 215, 221, and the prior fixture ATs). An AT may use different `S3A_KITE_BIN*` variables for different cases.

**F-03** (AT-156 case 3 had no hook for the post-503 / pre-retry-write window). Added closed hook point `pause_before_lrs_retry_write` (`pause_until_signal`), cited in AT-156 case 3. AT-156 is assigned to `S3A_KITE_BIN_EGRESS_TEST_HOOKS`.

## Q-19 / DD-152

Ashley decided (2026-11-16): "Q-19: re-fetch the paper when the recorded snapshot is missing." New DD-152:

- A missing recorded `{sha}.s3a` (ENOENT), as opposed to present but failing DD-103 reuse, is not `snapshot_invalid`. There is no usable snapshot; step 2 runs if the source table requires a fetch (AT-228).
- If the re-fetched bytes hash to the recorded sha, the host publishes `{sha}.s3a` (a create, not an overwrite) and the existing index line stands.
- If they hash to a different `new_sha`, the host publishes `{new_sha}.s3a` under the usual new-snapshot rules (reuse / `snapshot_conflict` still apply) and appends a new inclusion-index line for this verb with `snapshot_sha256 = new_sha`. Step 1 stays skipped when a matching verb line already existed (no second tile-10). Download then runs 2b from the published snapshot.
- A present-but-failing recorded file remains AT-209 / `snapshot_invalid` and does not re-fetch.

Q-19 is marked answered.

## Minors

**F-04.** Fixed: on a download, `snapshot_too_large` is `download_error: "fetch_failed"`, `path: null`, `snapshot: null`, 2b skipped. Waiters receive it as a fetch failure. AT-227 extended.

**F-05.** Fixed: both §5.2.1 and DD-125 state the exception: a header block over `--max-request-bytes` is 413 before authentication; body overflow is 413 after authentication.

**F-06.** Fixed: `invalid_view` is returned before a missing or mistyped `handle` (it is never `unknown_handle` merely because `handle` is omitted). `/v1/frame` `instant` is the query literals `true` / `false`; the JSON-boolean wording is dropped.

**F-07.** Fixed: `withheld_count` includes records withheld only by `MARK_CONSENT_WITHDRAWN` or `MARK_TARGET_ALL_RECORDS`. AT-11 adds a mark-only unrestricted fixture that paints `withheld_count: 1`.

**F-08.** Fixed: the BLOB/PROVENANCE metadata-only rule, and a rule that a hyper-tile floor whose layout kite cannot decode shows metadata only, now live in §5.3 item 5. §5.4 item 5 points at that item.

**F-09.** Fixed: `open_verified` rejects `quarantine_reason` `chain_break`, `signature_known_signer`, or `crc` with `break_index: None`, and a hyper-tile report with `status` `verified` or `self_signed` (with or without a reason), as `invalid_report`. AT-211 extended.

**F-10.** Fixed: a failed runtime step-4 unlink of `revoke-pending` after steps 1–3 succeeded is logged, the revoke returns 200, and the leftover is handled at the next start (AT-181). It is not a DD-138 fail-closed event. Tightened DD-113; DD-138 is unchanged.

**F-11.** Fixed: `drive_path` (after resolving a relative path against the dossier parent) is lexically normalized, matched to the unique allowed root by path-component prefix, and walked from that root's startup dirfd with `O_NOFOLLOW`. No match, or more than one, is `on_disk: false`.

**F-12.** Fixed: AT-183 is split into a unit-test half in `crates/s3a-kite` and host-spawn halves in `kite-acceptance` (startup error under `S3A_KITE_BIN`, end-to-end under `S3A_KITE_BIN_EGRESS_TEST`). §5.1 and §7 agree.

**F-13.** Fixed: AT-226 observes "zero DNS queries" by running the host in a network namespace whose `/etc/resolv.conf` points at a counting stub resolver (not a host flag).

**F-14.** Fixed: duplicate `paper_id`s are resolved by bytewise ascending canonical absolute paths, then first record in tile order. AT-225's last sentence is now the observable check that the listed candidate's `page_path` is that first file.

**F-15.** Fixed: AT-66 includes a fixture with both `FLAG_TILE_SIZE_4KB` and `FLAG_TILE_SIZE_8KB` set (`unsupported` / `banner`).

**F-16.** Fixed: AT-187 drops the vacuous distinct-`--snapshot-dir` claim and adds a case that plants a lockable stale `s3a-kite-tmp-*` and a separately locked one in a fresh host's own `--snapshot-dir` (and `--download-dir`); only the unlocked file is deleted.

**F-17.** Fixed: AT-117 / AT-142 other-owner cases require root (or a pre-provisioned file) and are skipped and reported otherwise.

**F-18.** Fixed: AT-125's `--bind 0.0.0.0` without `--headless` case runs on `S3A_KITE_BIN` (no `native-window`), so the host names `--headless` and a missing display is not in play.

**F-19.** Fixed: AT-46 mid-process `ADDON_EDU` clear is in place on the same `(dev, ino)` (a rewrite-and-rename is `write_failed`). Step 3's in-memory flag check takes a brief mutex hold; the inhibit check stays lock-free. §12 history is reordered to v1–v22 chronological.

## AT audit (v21 → v22)

- **Defined range:** AT-01–AT-228 (228 tests, no gaps).
- **Changed (18):** AT-11, AT-46, AT-53, AT-66, AT-74, AT-78, AT-79, AT-117, AT-125, AT-142, AT-156, AT-183, AT-187, AT-209, AT-211, AT-225, AT-226, AT-227.
- **Added (1):** AT-228 (missing recorded snapshot is a re-fetch; same-hash republish and different-hash new sha).
- **Removed:** none.

## New and tightened decisions / questions

- **New:** DD-152 (Q-19 answered: missing recorded snapshot is a re-fetch; hash-mismatch publishes the new sha and appends an index line).
- **Tightened:** DD-28, DD-61, DD-103, DD-112, DD-113, DD-121, DD-125, DD-140, DD-149, DD-151.
- **Answered Q:** Q-19 (Ashley, 2026-11-16).
