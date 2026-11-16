# Round 23 disposition — s3a-kite SPEC v24

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round23.md` (copied to `reviews/goldfish_s3a-kite_round23.md`).
18 findings (0 blockers, 2 majors, 16 minors). Archived v23: `reviews/SPEC-v23.md`.

**Counts:** 18 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-142, DD-143, DD-144, DD-145, DD-146, DD-147, DD-150, and DD-152 are unchanged (byte-identical to v23). DD-28, DD-57, DD-93, DD-108, DD-122, DD-130, DD-131, DD-134, DD-148, DD-149, and DD-151 keep their prior intent and are tightened. New **DD-153** (Q-20: a fixed cap of 8 concurrent connections on a non-loopback bind; connection 9 is accept-then-close). Q-19 remains answered (DD-152). Q-20 is answered (DD-153). No new Q.

This v24 is not frozen (DD-149): round 23 still had majors.

## How the majors were resolved

**F-01** (AT-229 case 1 could not be set up: hooks are fixed at startup, and `fail_with_errno` fails every matching operation, so a later qualifying `on` in the same process cannot succeed at the unlink; restarting hits the startup path and forces consent `off`. AT-212 and AT-213 had the same problem in weaker form.) Added a one-shot hook action, `fail_once_with_errno=N`, that fails only the first matching operation at that point; later firings at that point are no-ops. Hook actions otherwise stay active for the whole process lifetime; hooks are still read only at startup. AT-229 case 1 uses `fail_once_with_errno=N` so the runtime step-4 unlink fails and a later qualifying `on` in the same process succeeds. Case 2 still uses process-lifetime `fail_with_errno=N`. AT-212 and AT-213 also use `fail_once_with_errno=N` so a later select can retry the fetch or reuse the left file. Tightened DD-151.

**F-02** (AT-187's consent-temp case needed a pause the closed hook list did not have). Added hook point `pause_before_consent_rename` with `pause_until_signal`, covering consent-file and `revoke-pending` publishes, after the locked temp exists and before the rename. AT-187's third case now parks host A at that point; B's startup must not delete A's locked consent temp, and a lockable stale sibling in the same parent is deleted. Tightened DD-151.

## Q-20 (Ashley, 2026-11-16)

Ashley: "Let's just use 8." Resolved as **DD-153**. When `--bind` is not a loopback IP and not a Unix socket, concurrent connections are capped at a **fixed 8**. This is not a flag (no invented default). A connection occupies a slot from TCP accept until close, including during the TLS handshake, header-read, request handling, and keep-alive idle. When 8 slots are occupied, connection 9 is accepted at the TCP layer and then closed immediately, without a TLS handshake and without an HTTP response (consistent with `--handle-idle-secs` close-without-response). It is not left in the listen backlog waiting for a slot. When a slot frees, a later accept may occupy it. Loopback IP binds and Unix-socket binds have no such cap. AT-231 covers this. Tightened DD-108.

## Minors

**F-03.** Fixed: the §5.5 source-decision paragraph now says usability looks at the last index line for the key that has a non-null `snapshot_sha256` (step 0, DD-152), not "any" line.

**F-04.** Fixed: DD-57, DD-148, and AT-35 now say every tile at or after `break_index` = (the highest preceding authentic `tile_index`) + 1, or 0, is unauthenticated / invalidated, and that gap tiles in that range are untrusted. Intent of DD-148 is kept.

**F-05.** Fixed: `torn_tile_indices` contains only indices that satisfy the §5.4 / DD-100 torn predicate. Tiles uncommitted solely because they follow a torn or CRC stop, but that do not themselves satisfy the predicate, go in `invalid_tile_indices` only. The §5.4 derivation-table torn rows and AT-165 match.

**F-06.** Fixed: the host does not consume a request-path fd; it `fstat`s (regular file; `st_size` greater than `--max-open-bytes` is `file_too_large`) and passes the fd to the pack. The pack performs the single bounded `pread` from offset 0 (at most cap + 1). `paper_identity` uses those pack-read bytes. The working-archive `/v1/open` copy still has the host read and call pack `open` on those bytes.

**F-07.** Fixed: a hyper-tile report may not use `quarantine_reason` `signer_not_pinned`. Added to the §5.1 `invalid_report` checks and to AT-211.

**F-08.** Fixed: if a complete LRS status line has been read, an oversized remainder follows the DD-134 status class (2xx/409 is `sent`; 408/429/5xx are queued; other 4xx is permanent). An oversized header block before a status line is a permanent failure (`.permanent` written, `lrs_status: "failed"`). A retry-file write, fsync, or rename failure is logged, `lrs_status: "failed"`, and the process stays up (not fail-closed). Hook `fail_lrs_retry_write` and AT-191 cover this. Tightened DD-131 and DD-134.

**F-09.** Fixed: `--handle-idle-secs` runs from TCP accept and includes the TLS handshake. AT-230 adds a peer that never sends a ClientHello, or stalls mid-handshake. Tightened DD-108.

**F-10.** Fixed: every Edge floor type other than 64 (32-byte and 96-byte layouts), 70, and 71 shows metadata only (type, size, `record_count`) and never raw or decoded payload fields, under a research grant or a research denial. AT-232 covers an Edge type-75 (or other non-64/70/71) floor with the §5.8 render-model pixmap oracle.

**F-11.** Fixed: AT-172's `floor-detail` request is written as `view=floor-detail&floor=0&w=…&h=…`. The omitted-`floor` variant is `400` `missing_param`.

**F-12.** Fixed: added hook `pause_after_consent_receipt` (after receipt-time latch actions, before the write mutex). AT-198 uses it to park requests in a determined order, including the disconnect case (client closes after READY, test writes CONTINUE). AT-198 is assigned to `S3A_KITE_BIN_TEST_HOOKS`. Tightened DD-151.

**F-13.** Fixed: snapshot and download temps are written and sealed through an fd-taking engine API on the same file description that holds `LOCK_EX`. `/proc/self/fd/<fd>` remains allowed only for the working-archive hold, which has no host `LOCK_EX` on that fd. Noted in §5.2.2, §5.6, Snapshot encoding, DD-28, and DD-122.

**F-14.** Fixed: the out-of-workspace reader test crate builds `s3a-reader-core` against the same `s3a-engine` path or revision as the pack (storage `b7bdcda` or later). Stated in §5.1 Test locations and in AT-165.

**F-15.** Fixed: sidecar `.lock` opens use `openat(dirfd, name, O_CREAT|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC, 0600)` and `fstat` must show a regular file; otherwise startup errors naming the path. These opens are on the shared safe-open site list. AT-208 adds FIFO/symlink/non-regular cases.

**F-16.** Fixed: the `--working-archive` and `--consent-state-file` basenames must not match `s3a-kite-tmp-*` (startup error naming the flag). Startup temp cleanup never deletes a configured working-archive or consent-state-file path. AT-208 covers the reserved-name case. Tightened DD-122.

**F-17.** Fixed: AT-171 covers a missing `Content-Type` on a JSON-body route (`415`) and `floor-transition` with `instant=false` (or omitted) and `t` omitted (`400` `missing_param`). AT-88 covers a redirect to `ftp://` or `file://` (`fetch_error: "scheme_not_allowed"`). AT-183 covers an allowed fixture port with nothing listening (`fetch_error: "connect_failed"`). AT-191 covers LRS `408` and `429` as queued. AT-151 covers a otherwise-valid consent file with no trailing newline as accepted.

**F-18.** Fixed: DD-93's viewer-only clause now names the token file and TLS files versus `--view-root`. DD-130's `invalid_tile_indices` union includes the unsigned tail (verifying container exempt) when any authentic type-70 record exists.

## AT audit

- Inventory: AT-01–AT-232, all unique, none missing, none removed. DD-01–DD-153 (DD-63 still retired; DD-126 still unused). Q-01–Q-20 (Q-03, Q-19, and Q-20 answered).
- 16 existing ATs changed in place: AT-35, AT-88, AT-151, AT-165, AT-171, AT-172, AT-183, AT-187, AT-191, AT-198, AT-208, AT-211, AT-212, AT-213, AT-229, AT-230.
- Added: AT-231 (non-loopback connection cap of 8; accept-then-close on connection 9), AT-232 (Edge types other than 64/70/71 are metadata-only).
- Every new hook point is on the closed §5.2.7 list: `fail_once_with_errno=N` (action), `pause_before_consent_rename`, `pause_after_consent_receipt`, `fail_lrs_retry_write`.
- Host-spawn binary variables: AT-231 on `S3A_KITE_BIN`; AT-198 on `S3A_KITE_BIN_TEST_HOOKS`; AT-191 fixture cases on `S3A_KITE_BIN_EGRESS_TEST` and hook case on `S3A_KITE_BIN_EGRESS_TEST_HOOKS`. AT-232 lives in `kite-pack-tests`.
- Stale "any index line", "hook cleared", "Q-20 invents no cap", and "breaking tile and later tiles" restatements were checked and are gone from the normative text.
- Identical copies: `/workspace/s3a-kite/SPEC.md` and `/workspace/goldfish/s3a-kite.md`.
