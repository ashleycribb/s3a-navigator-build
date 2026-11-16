# Round 22 disposition — s3a-kite SPEC v23

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round22.md` (copied to `reviews/goldfish_s3a-kite_round22.md`).
13 findings (0 blockers, 2 majors, 11 minors). Archived v22: `reviews/SPEC-v22.md`.

**Counts:** 13 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-143, DD-147, DD-148, and DD-150 are unchanged (byte-identical to v22). DD-61, DD-108, DD-113, DD-120, DD-121, DD-122, DD-149, DD-151, and DD-152 keep their prior intent and are tightened. No new DD. New **Q-20** (maximum concurrent-connection count on a non-loopback bind; no invented cap). Q-19 remains answered (DD-152).

This v23 is not frozen (DD-149): round 22 still had majors.

## How the majors were resolved

**F-01** (AT-225 required `page_path` on an `AcademicPaperRecord`, but §5.2.1 / §5.5 / AT-101 define that field as `null`). Left `page_path` as `null` for `AcademicPaperRecord` candidates. Added optional `archive_path` (canonical `.s3a` path, or `null` for a dossier candidate) to the `/v1/candidates` schema and to §5.5. AT-225 asserts `page_path` is `null` and `archive_path` is the first file in bytewise canonical-path order. `archive_path` is not a dossier path; opening it does not yield `paper_identity`. Tightened DD-61.

**F-02** (a control fixture with different on-disk payload bytes would change `payload_hash_short`, CRC, and seal/signature values). Redefined the §5.8 oracle as a **render-model control**: the control pixmap is produced from the **same archive bytes** as the fixture; withheld field values are replaced after record decode, before paint, by different same-length bytes; derived displays (payload digests, CRCs, seal/signature status) stay equal. The fixture pixmap must be byte-identical to that control. Tests do not substitute different on-disk payload bytes. AT-11, AT-23, AT-176, AT-184, AT-200, and AT-204 now cite the §5.8 render-model pixmap oracle.

## Minors

**F-03.** Fixed: the `/v1/select` row, the §5.5 step-2 / failure paragraph, the inclusion-index paragraph, and DD-120 now say "recorded, **present**, and fails the reuse checks" for `snapshot_invalid`, and "a **usable** snapshot" / "the only recorded `{sha}.s3a` files are missing (DD-152)" for re-fetch.

**F-04.** Fixed: usability and `snapshot_invalid` look only at the **last** index line for the key that has a non-null sha (DD-104). If that last file is missing, the request re-fetches even if an earlier sha is present but failing; if that last file is present and failing, the result is `snapshot_invalid` even if an earlier sha would be usable. AT-228 adds that mixed sub-case. Tightened DD-152 (intent kept).

**F-05.** Fixed: step 0 now says 2b runs when there are bytes to deliver (usable on-disk fd, an existing usable snapshot, or a snapshot this request or the GET it waited on has just published), and does not run after a failed reuse check, a fetch failure, `snapshot_too_large`, or `snapshot_conflict`.

**F-06.** Fixed: a qualifying `on` unlinks leftover `revoke-pending` and `fsync`s the parent before persisting `on`. Failure of that unlink is treated as a DD-138 fail-closed event and does not write `on`. Hook `fail_revoke_pending_remove` also applies at runtime step 4 and on that `on`. New AT-229. The failed step-4 unlink itself still returns 200 (not fail-closed). Tightened DD-113; DD-138 is unchanged.

**F-07.** Fixed: every `s3a-kite-tmp-*` temp (snapshot, download, inclusion index, consent file, `revoke-pending`) is held `LOCK_EX` on its own fd from create until publish or delete. AT-187 adds two hosts with distinct consent files in the same parent.

**F-08.** Fixed: closed `fetch_error` vocabulary in §5.2.4: `scheme_not_allowed`, `egress_denied`, `egress_not_allowlisted`, `connect_failed`, `timeout`, `too_many_redirects`, `http_status`, `unknown_encoding`, `headers_too_large`, `body_too_large`, `snapshot_too_large`, with first-failing-check precedence. The listed ATs assert the specific code.

**F-09.** Fixed: each event row's severity badge is a `SemanticNode` with role `label` and label `severity: N` carrying its CSS rect. AT-19 samples the pixmap at that rect's centre, scaled by `device_pixel_ratio`.

**F-10.** Fixed: an unreadable or `FileHeader::verify`-failing working archive in the step-0/1 hold is `500` `write_failed` (nothing written, no `lrs_status`; AT-74). If `--data-dir/lrs-retry` exists, `lstat` (no following) must show a directory; otherwise startup errors naming `lrs-retry` (AT-199). Revoke deletion uses `unlinkat` within a directory fd opened `O_NOFOLLOW|O_DIRECTORY`.

**F-11.** Fixed: `--handle-idle-secs` also bounds header-read and inter-byte idle time on every connection, including unauthenticated peers, on every bind including non-loopback TLS; the connection is then closed without a response (AT-230). A maximum concurrent-connection count on a non-loopback bind is **Q-20**; no cap is invented. Tightened DD-108.

**F-12.** Fixed: covering cases on AT-77 (`--lrs-url` path/userinfo/query/fragment), AT-117 (existing consent file `0644` / foreign owner), AT-224 (token-file FIFO, symlink, oversize), AT-68 (two identity records, first in tile order), and AT-103 (redirect chain whose total time exceeds `--fetch-timeout-secs`). AT-222's `test_hooks`-build half is listed under `S3A_KITE_BIN_TEST_HOOKS`. DD-121 now attributes `parse_allowlist_entry` to AT-183. The `/v1/download` row's `download_conflict` `included: true` wording matches the catalogue (after step 0/1, this request or earlier).

**F-13.** Fixed: `/v1/open` of any file under `--snapshot-dir` larger than `--max-open-bytes` (for example a planted file) is `file_too_large` (AT-170). Dropped the AT-31 / AT-80 citations. Kite still does not publish an oversize snapshot (AT-227).

## AT audit (v22 → v23)

- **Defined range:** AT-01–AT-230 (230 tests, no gaps).
- **Changed (24):** AT-11, AT-19, AT-23, AT-54, AT-68, AT-74, AT-77, AT-88, AT-103, AT-117, AT-122, AT-139, AT-140, AT-176, AT-183, AT-184, AT-187, AT-199, AT-200, AT-204, AT-224, AT-225, AT-226, AT-228.
- **Added (2):** AT-229 (stale `revoke-pending` and a later qualifying `on`), AT-230 (`--handle-idle-secs` on an unauthenticated non-loopback TLS connection).
- **Removed:** none.

## New and tightened decisions / questions

- **New DD:** none.
- **Tightened:** DD-61, DD-108, DD-113, DD-120, DD-121, DD-122, DD-149, DD-151, DD-152.
- **New Q:** Q-20 (max concurrent connections on a non-loopback bind; no invented cap).
