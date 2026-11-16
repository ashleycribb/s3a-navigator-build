# Round 20 disposition — s3a-kite SPEC v21

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round20.md` (copied to `reviews/goldfish_s3a-kite_round20.md`).
18 findings (0 blockers, 3 majors, 15 minors). Archived v20: `reviews/SPEC-v20.md`.

**Counts:** 18 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-143, DD-147, DD-148, and DD-150 are unchanged (byte-identical to v20). DD-28, DD-76, DD-86, DD-103, DD-108, DD-114, DD-121, DD-122, DD-140, DD-146, DD-149, and DD-151 keep their prior intent and are tightened (name-binding `fstatat` and no startup-lifetime fd; header and LRS caps tied to existing flags; `snapshot_too_large` and Q-19 for a missing recorded snapshot; parent-dir `fsync` and `lrs-retry/` mode `0700`; `lrs_deny_union` unit test and name-before-DNS deny; hook rename and new hook points; `S3A_KITE_BIN*` matrix; v21 is not frozen). New open question Q-19. No new DD. No invented numeric defaults (no snapshot-size overhead formula; header and LRS caps reuse `--max-request-bytes`, `--max-fetch-bytes`, and `--max-open-bytes`).

This v21 is not frozen (DD-149): round 20 still had majors.

## How the majors were resolved

**F-01** (`(dev,ino)` on an already-open fd cannot see a directory-entry swap; AT-219 therefore could not catch a late symlink; a startup-lifetime fd made AT-203 ambiguous). Fixed:

1. The host does **not** keep a startup-lifetime writer fd. Every step-0/1 hold re-opens the working archive with `openat(O_NOFOLLOW|O_NONBLOCK)` from the parent dirfd. `O_CREAT|O_EXCL` is used only at startup if the final component is missing. Mid-process missing file or `(dev,ino)` mismatch versus the recorded identity is `500` `write_failed` until restart (no mid-process re-record or create).
2. Immediately before each tile-10 write, `fstatat(parent_dirfd, basename, AT_SYMLINK_NOFOLLOW)` must succeed, be a regular file, and equal both `fstat(writer_fd)` and the recorded identity.
3. AT-219's hook is renamed to `pause_before_name_binding_check` and sits just before that `fstatat` check.
4. Header reads for `file_uuid` and `addon_mask` use the same per-hold validated fd (F-06, folded into this rule).
5. Residual recorded in DD-146 / DD-28: a swap *after* the final name-binding check may write the already-open inode, never the symlink target.

**F-02** (AT-226's "newly observed LRS IP" could not be set up under literal addresses, no resolver override, and no global-unicast fixture; LRS hostname resolution failure at startup was undefined). Fixed: the union is a unit test, in `crates/s3a-kite` with no network, of the pure function `s3a_kite::egress::lrs_deny_union(startup_ips, connect_time_ips, addr) -> bool`. An `https` `--lrs-url` hostname that fails to resolve at Scholar start is a startup error that names `--lrs-url` and binds nothing (host-spawn; a reserved TLD not in `/etc/hosts` is allowed). No resolver-override flag is added.

**F-03** (`window_issue_select` depended on a first warehouse frame that the hook could not arrange). Fixed: the hook is self-contained. After `/health` succeeds, the native window issues `GET /v1/candidates` then `POST /v1/select` of the first listed candidate, both with the Bearer token. The "first warehouse frame" precondition is dropped. The native-window dependency is kept so AT-125 still shows that window-originated writes carry the UI token.

## Minors

**F-04.** Fixed: at publish, if the sealed snapshot file is larger than `--max-open-bytes`, the host does not publish it, returns `200` with `fetch_error: "snapshot_too_large"`, keeps the inclusion, records no `snapshot_sha256`, still runs step 3 when consent is on, and removes the registry entry. A later select of the same identity may retry. `--max-fetch-bytes` ≤ `--max-open-bytes` remains the start check for the download-dir hash. No numeric overhead formula is invented. New AT-227.

**F-05.** Fixed: `open_verified` rejects a contradictory report as `invalid_report` (converse: `quarantine_reason` is `Some` only if status is Quarantined; kind-specific enum constraints). Extended AT-211.

**F-06.** Fixed: header reads for `file_uuid` / `addon_mask` use the per-hold validated fd. Mid-process delete or recreate is `500` `write_failed` until restart.

**F-07.** Fixed: consent, revoke-pending, and inclusion-index publishes use a temp named `s3a-kite-tmp-{pid}-*` in the parent directory, `fsync` of the temp, atomic rename, then `fsync` of the parent directory, before the response. The same temp pattern in `--data-dir` and the consent-file parent is cleaned after sidecar locks are held. `lrs-retry/` is mode `0700`. Tightened DD-122.

**F-08.** Fixed: AT-11 and AT-23 also cover a floor-transition at `t=0.225` with `instant=true`.

**F-09.** Fixed: when an AT requires a unique withheld string to be absent from pixels, the oracle is a control fixture whose withheld payload bytes are replaced by different bytes of the same length; the two pixmaps must be byte-identical. No OCR. Cited by AT-11, AT-23, AT-176, AT-184, AT-200, and AT-204.

**F-10.** Fixed: unknown-layout type-64 floors and BLOB/PROVENANCE floors paint metadata only and never the raw payload of withheld fields (including when the captured body is itself an S3A archive). AT-67, AT-107.

**F-11.** Fixed: `fail_lrs_retry_delete` applies to every `lrs-retry/` delete, including startup. New hook point `fail_revoke_pending_remove` for AT-181 / AT-199.

**F-12.** Fixed: AT-24 / AT-25 byte-identity baseline is after bind and a successful `/health`.

**F-13.** Fixed: AT-183 splits an unparseable allowlist entry to a `parse_allowlist_entry` unit test plus a host-spawn startup error. AT-168's first view is "a first view, page, or warehouse".

**F-14.** Fixed: `metadata.google.internal` is denied by name before any DNS query. AT-226 observes zero DNS queries and zero connects for that name.

**F-15.** Fixed: caps are tied to existing flags (no invented values). The inbound request header block and body are each bounded by `--max-request-bytes` (`413` even without a token for header overflow; AT-162, AT-171). The fetch response header block is bounded by `--max-fetch-bytes` (AT-140). The LRS response header block and body are each bounded by `--max-open-bytes` (AT-191). Tightened DD-76, DD-86, DD-108.

**F-16.** Fixed: AT-172 / AT-125 / AT-171 cover `not_archive` / proceed no-op / floor `hit_test` `None` / no-display `DISPLAY` cases. `render_failed` is marked untestable on purpose (no reachable deterministic trigger; no invented hook).

**F-17.** Fixed: host-spawn ATs find a prebuilt `s3a-kite` through a closed environment-variable matrix (`S3A_KITE_BIN`, `S3A_KITE_BIN_EGRESS_TEST`, `S3A_KITE_BIN_TEST_HOOKS`, `S3A_KITE_BIN_EGRESS_TEST_HOOKS`, `S3A_KITE_BIN_NATIVE_WINDOW` with `native-window,test_hooks`, `S3A_KITE_BIN_RELEASE`). `kite-acceptance` looks up the named variable and fails at runtime with "binary not found" naming that variable if it is unset or the path is missing. No `CARGO_BIN_EXE_*` (the host crate is out of the workspace). Tightened DD-121.

**F-18.** Fixed: a missing recorded `{sha}.s3a` (as opposed to present but failing DD-103 reuse checks) is unspecified. New **Q-19**. AT-209 does not cover a missing file. Implementations must not invent re-fetch or permanent `snapshot_invalid` until Ashley answers.

## AT audit (v20 → v21)

- **Defined range:** AT-01–AT-227 (227 tests, no gaps).
- **Changed (27):** AT-11, AT-23, AT-24, AT-25, AT-67, AT-107, AT-124, AT-125, AT-140, AT-162, AT-168, AT-169, AT-171, AT-172, AT-176, AT-181, AT-183, AT-184, AT-191, AT-199, AT-200, AT-203, AT-204, AT-209, AT-211, AT-219, AT-226.
- **Added (1):** AT-227 (publish-time `snapshot_too_large` when the sealed snapshot exceeds `--max-open-bytes`).
- **Removed:** none.

## New and tightened decisions / questions

- **New DD:** none.
- **Tightened:** DD-28, DD-76, DD-86, DD-103, DD-108, DD-114, DD-121, DD-122, DD-140, DD-146, DD-149, DD-151.
- **New Q:** Q-19 (missing recorded snapshot file: re-fetch vs permanent `snapshot_invalid`).
