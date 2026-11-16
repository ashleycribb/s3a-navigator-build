# Round 13 disposition — s3a-kite SPEC v14

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round13.md` (copied to `reviews/goldfish_s3a-kite_round13.md`).
16 findings (0 blockers, 4 majors, 12 minors). Archived v13: `reviews/SPEC-v13.md`.

**Counts:** 16 fixed, 0 rejected, 0 deferred.

DD-128, DD-142, DD-143, DD-144, and DD-146 are unchanged (byte-identical to v13). DD-138, DD-145, and DD-147 keep their v13 intent and are tightened (in-flight fail-closed writes; total attribution; pointer to DD-148). No new open questions. No invented numeric defaults.

## How majors were resolved

**F-01** (Edge attribution left "empty pin list + embedded keys + first/all-fail" undefined; "unauthenticated as a whole" disagreed between `open` and `open_verified`). The §5.4 item 2 algorithm is now **total**: every type-70 record maps to exactly one of authentic, `signature_known_signer`, `signer_not_pinned`, or `missing_signature`. With an empty pin list and at least one embedded key, a record that fails every embedded key with no earlier authentic record is `missing_signature` (no valid floors); `signer_not_pinned` is not used when the pin list is empty. **Unauthenticated as a whole** is defined once (DD-148, §5.3 item 4) and used identically by pack `open` / `open_path` (pack-computed report) and `open_verified` (supplied report): iff (hyper-tile and `seal_status == seal_failed`) or `quarantine_reason` is `signer_not_pinned` or `missing_signature`. The extra §5.3 clause "or otherwise Quarantined with no verified signer" is deleted. An Unsigned Edge file with a committed CRC failure is explicitly not unauthenticated as a whole. AT-201 adds fixtures (6) and (7) for the empty-pin all-fail and earlier-authentic-then-fail cases; those fixtures stay in the AT-165 set.

**F-02** (AT-35 gave Edge CRC failures a trailer-based `seal_status`). AT-35 now follows the Edge label table: hyper-tile `seal_status` is `seal_failed` if a trailer is present, else `unsealed`; Edge is `seal_failed` when `FLAG_SEAL_CHAIN_ACTIVE` is set (the CRC stop breaks the chain), else `unsealed`. The label table states the chain-inactive CRC case (`unsealed`). Tightened DD-100.

**F-03** (AT-183 rejected `2001:db8::1:443`, which is a valid bare IPv6 address). Ports on IPv6 entries require brackets (`[addr]:port`). A bare unbracketed entry is parsed as an IPv6 address first: if it parses, it is a portless exact address (`2001:db8::1:443` is that address, not host `2001:db8::1` at port 443). An unparseable unbracketed form (`2001:db8::1::443`) is a startup error naming `--egress-allowlist`. Aligned §5.2.4, DD-133, and AT-183.

**F-04 / F-05** (`snapshot_conflict` vs `snapshot_invalid` were inconsistent; AT-180's second half could not happen). One split, used everywhere (error table, DD-103, DD-120, step 0, AT-180, AT-209): `409` `snapshot_conflict` is only for a **newly fetched** capture whose `{sha}.s3a` already exists and fails the reuse checks; a sha already **recorded** in the index that fails the checks is HTTP 200 `snapshot_invalid`. AT-180's later download now re-fetches, hits the same planted file, and returns `409` `snapshot_conflict` with `included: true` and `lrs_status: "skipped"` (step 0 matched; no sha was recorded). The 200 `snapshot_invalid` download path is asserted in AT-209 (recorded sha, then the file is corrupted).

## Minors

**F-06.** Fixed: the retry-file write gate, and DD-134 / HTTP outcomes, now require inhibit inactive and the flag `on`. If inhibit is active or the flag is `off` in that hold, write nothing and return `skipped`. AT-156 case 3 matches.

**F-07.** Fixed: `verification_report` is in the §5.7 phone UniFFI export list and in §11 step 6.

**F-08.** Fixed: `included: true` is present whenever a matching inclusion exists after step 0/1, in the error table, Download names, DD-119, AT-94, and AT-180.

**F-09.** Fixed: DD-125 repeats the §5.2.1 order exactly (auth → 405 → 413/415 → `viewer_only` → JSON/param → route-specific). When `view` is unknown, `invalid_view` is returned before any `floor` check. FFI `floor` is `None` on a non-floor view; `Some(_)` is `invalid_param`.

**F-10.** Fixed: the canonical paths of `--working-archive`, `--working-archive.lock`, `--consent-state-file`, `--auth-token-file`, `--lrs-credentials-file`, `--pinned-keys-file`, `--tls-cert`, and `--tls-key` (when set) must be pairwise distinct. A violation names both flags, binds nothing, and mutates nothing. Extended AT-208.

**F-11.** Fixed: the host holds `LOCK_EX` on a snapshot/download temp file's own fd until publish or delete. Startup cleanup deletes a same-euid `s3a-kite-tmp-*` file only if `LOCK_EX|LOCK_NB` succeeds. Ephemeral temp directories keep a marker file (DD-136 distinguishes the two).

**F-12.** Fixed: `paper_identity` is `Some(stem)` iff the opened canonical path would be a member of item 1 of the §5.5 candidate set under the same roots and the same rules (AT-188, AT-207).

**F-13** and the consent consistency sweep. Fixed: AT-11 is hand-verified under the §5.3 item 4 / DD-147 table; `is_active` / pure latest-wins wording is removed from AT-11, AT-136, AT-185, and DD-110. Other consent-restating ATs already matched DD-147 and were left or given an explicit DD-147 citation (AT-108).

**F-14.** Fixed: `/v1/open` `path` is a plain absolute path or a `file://` URL that is percent-decoded and then subject to the same rule (AT-53). A path that equals the canonical working-archive path is opened via the startup parent dirfd with `openat(O_NOFOLLOW)` and copied under the mutex.

**F-15.** Fixed: §5.2.2 uses the full viewer-only list (`--log-dir`, `--pinned-keys-file`, the token file, and TLS files versus `--view-root`). AT-179 adds token and TLS cases.

**F-16.** Fixed: after the failing hold releases the mutex, in-flight requests receive no further disk writes. They may get `500` `internal` if that needs no disk I/O; otherwise their connections are closed without a response. Leftover temps are removed by the next start's DD-122 cleanup. AT-169 asserts no second tile-10 after the failure. Tightened DD-138 (intent kept).

## Acceptance-test audit (test-by-test)

### Changed

| AT | Finding / change |
| --- | --- |
| AT-11 | Hand-verified under the DD-147 table; no `is_active` / latest-wins restatement (F-13). |
| AT-35 | Edge CRC `seal_status` follows the label table (`FLAG_SEAL_CHAIN_ACTIVE` → `seal_failed`, else `unsealed`) (F-02). |
| AT-53 | `file://` is percent-decoded, then the same path rule; working-archive path opens via parent dirfd (F-14). |
| AT-94 | `included: true` whenever a matching inclusion exists after step 0/1 (F-08). |
| AT-108 | Cites the DD-147 table (consistency sweep). |
| AT-136 | Withdrawal withholds under DD-147; no `is_active` (F-13). |
| AT-156 | Retry-write gate checks inhibit as well as the flag (F-06). |
| AT-169 | No second tile-10 after a fail-closed exit (F-16). |
| AT-179 | Viewer-only token and TLS files inside `--view-root` are the same startup error (F-15). |
| AT-180 | Later download re-fetches and returns `409` `snapshot_conflict` with `lrs_status: "skipped"` (F-04). |
| AT-183 | Bare `2001:db8::1:443` is a portless IPv6 address; unparseable `2001:db8::1::443` is the startup error (F-03). |
| AT-185 | Unsealed control applies DD-147, not a latest-wins shorthand (F-13). |
| AT-201 | Adds empty-pin all-fail (`missing_signature`) and earlier-authentic-then-fail (`signature_known_signer`) fixtures (F-01). |
| AT-208 | Pairwise-distinct secret/state file collisions (F-10). |
| AT-209 | Repeat download of a recorded-then-corrupted snapshot is 200 `snapshot_invalid` (F-04, F-05). |

### Added

None.

### Removed

None.

Passed unchanged: AT-01–AT-10, AT-12–AT-34, AT-36–AT-52, AT-54–AT-93, AT-95–AT-107, AT-109–AT-135, AT-137–AT-155, AT-157–AT-168, AT-170–AT-178, AT-181–AT-182, AT-184, AT-186–AT-200, AT-202–AT-207.

Full AT inventory: AT-01–AT-209, 209 tests defined, no gaps, no duplicate definitions, no references to undefined AT numbers.

## Contract range

Acceptance tests: AT-01–AT-209.
Draft decisions: DD-01–DD-148 (DD-63 retired; DD-126 unused). New: DD-148 (unauthenticated-as-a-whole predicate). Tightened with intent kept: DD-138, DD-145, DD-147.
Open questions: Q-01–Q-17 (Q-03 answered; no new question).
