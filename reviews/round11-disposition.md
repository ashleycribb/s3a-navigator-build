# Round 11 disposition — s3a-kite SPEC v12

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round11.md` (copied to `reviews/goldfish_s3a-kite_round11.md`).
22 findings (0 blockers, 6 majors, 16 minors). Archived v11: `reviews/SPEC-v11.md`.

**Counts:** 22 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-142, and DD-143 are unchanged (byte-identical to v11). No new open questions. No invented numeric defaults.

## How majors were resolved

**F-01** (revoke must block LRS immediately, but the only consent flag is mutex-guarded). Fixed by a simple two-piece state split, recorded as **DD-144**. Immediate inhibit is a lock-free `lrs_inhibit` latch (an atomic revoke epoch), set on receipt of `POST {"state":"off"}` before the mutex is taken. Setting it cancels every in-flight LRS POST through a shared cancellation token (or equivalent); those requests report `lrs_status: "skipped"`. The step-3 pre-send check, the retry-file write check, and startup replay all check the latch (lock-free) before any LRS send. The persisted / in-memory consent flag stays mutex-only (DD-142 unchanged). DD-113 now reads: set `lrs_inhibit` on receipt; under the mutex set the flag `off` and persist. AT-198: an `on` whose mutex acquisition happens after a revoke was received (epoch changed since that `on` was received) returns `off` and writes nothing. The latch is cleared only when a qualifying `on` persists.

**F-02** (a tampered seal or corrupted consent tile can release records the intact file withholds). Fixed by applying restrict-only uniformly: **when grants are untrusted, or the file is unauthenticated as a whole, subjects without a trusted grant are withheld**. Filter mode is on in those cases; the file never becomes unrestricted via tampering. A lone invalid-tile withdrawal still withholds only that subject and does not by itself put others in filter mode (AT-184). Added AT-200 (AT-185 fixture plus un-consented subject Y, who stays withheld after the seal is tampered). Tightened §5.3 item 4, §5.4 item 6, DD-67, and DD-111.

**F-03** (Edge type-70 signer attribution is undefined). Fixed by a normative signer-attribution algorithm, recorded as **DD-145**. Candidate keys are the pinned list if non-empty, else every embedded `DeviceIdentityRecord` key. A record is authentic if it verifies under any candidate key. Otherwise the algorithm distinguishes `signer_not_pinned`, `signature_known_signer`, and `missing_signature`, and quarantines the file whenever attribution is impossible. `FLAG_ED25519_SIGNED` set with no type-70 or no usable key is `missing_signature` (disjunction). Flag clear with type-70 present applies the signed-file rules. Added AT-201 and included those fixtures in AT-165.

**F-04** (startup mutates shared state before sidecar locks). Fixed: all four sidecar `flock`s are taken **first** at Scholar start, before any read or modification of `--data-dir`, the consent file, `--snapshot-dir`, `--download-dir`, the working archive, or ephemeral temps. Startup cleanup, bootstrap, and replay run only after the locks are held. AT-164 now starts a colliding host B (with and without `--lrs-url`) while A has `revoke-pending` / `lrs-retry/*.json` and asserts those files are byte-identical after B exits. Tightened DD-114.

**F-05** (AT-180 says step 3 still ran on a later download, but step 0 skips it). Fixed by aligning AT-180 with step 0: the later `snapshot_invalid` download has `lrs_status: "skipped"` (step 0 matched; no new statement). §5.5 step 3 and DD-120 now say step 3 still runs after `snapshot_conflict` / `download_conflict` only on the request that first wrote the inclusion line; `snapshot_invalid` on a later request is skipped.

**F-06** (`drive_path` / `/v1/open` canonicalize-then-open TOCTOU). Fixed by open-then-validate via fd, recorded as **DD-146**. Every request path and every `drive_path` used as an on-disk source is opened first (walk each component with `O_NOFOLLOW`; final component `O_NOFOLLOW|O_NONBLOCK`), then `fstat` requires a regular file, and at most `max_open_bytes` + 1 bytes are read. Step 2b and `/v1/open` use that fd and do not reopen the path by name. Added AT-202.

## Minors

**F-07.** Fixed: §5.3 item 1 now says `open_path` passes `name_hint` (the `/v1/open` `name`, else the basename).

**F-08.** Fixed: §8 preamble is DD-01–DD-146 (was a stale DD-01–DD-141).

**F-09.** Fixed: `/v1/frame` and `/v1/hit-test` use one ordered list: auth → 413/415 → malformed/missing/invalid param (including floor-on-non-floor-view) → `invalid_view` → `unknown_handle` → matrix → `invalid_floor` → `invalid_frame_size` → `no_frame`. Optional fields a view does not use are ignored. Tightened DD-125; AT-196 already matches (`page`+`floor` is `400` `invalid_param`).

**F-10.** Fixed: a request inserts a registry entry only after step 1 has succeeded or been skipped, and only if it will start the GET. The entry is removed in the same mutex hold as the snapshot publish and index append, or the fetch-failure / `snapshot_conflict` outcome. A failed step 1 leaves no entry (AT-89). Tightened DD-112.

**F-11.** Fixed: a waiter receives the starter's outcome (snapshot path; `fetch_error` / `download_error: "fetch_failed"`; or `409` `snapshot_conflict` with the waiter's own `included` / `lrs_status`) and never starts a GET in the same request.

**F-12.** Fixed: after a successful startup retry, the host removes `revoke-pending`; failure to remove it is a startup error naming `revoke-pending`. AT-181 asserts the path is absent.

**F-13.** Fixed: under `open_verified`, valid floors are the complement of `invalid_tile_indices`; the file is unauthenticated as a whole iff (hyper-tile ∧ `seal_status == seal_failed`) or `quarantine_reason ∈ {signer_not_pinned, missing_signature}`; for Edge, tiles at or after `break_index` are unauthenticated.

**F-14.** Fixed: when a file has no valid floors, `withheld_count` on the banner still counts parseable subject-bearing / event-log payloads that the filter withholds. AT-134 and AT-195 now assert that count.

**F-15.** Fixed: IPv6 allowlist entries that include a port must be bracketed (`[2001:db8::1]:443`). An unbracketed IPv6 literal with a port is a startup error naming `--egress-allowlist`. Extended AT-183 and DD-133.

**F-16.** Fixed: PROVENANCE `url=` and `EXT_SOURCE_URL` hold the candidate's listed `source_url` (pre-redirect). No `final_url=` line in v1.

**F-17.** Fixed: startup temp cleanup deletes only when the flock'd marker exists and can be taken. Names whose pid cannot be parsed are skipped. Cleanup does not delete on a "pid not running" check alone. Tightened DD-88, DD-122, and DD-136.

**F-18.** Fixed: `invalid_tile_indices` uses one range, `0 .. tiles_total-1`, where `tiles_total` is every tile index the reader enumerates, including torn ones (and may differ from the header `tile_count`). AT-165 and AT-195 use that name.

**F-19.** Fixed: at startup the host `lstat`s the `--working-archive` final component; if it exists it must be a regular file (not a symlink), or startup fails naming `--working-archive`. Create and open use `O_NOFOLLOW`. Added AT-203.

**F-20.** Fixed: `included: true` whenever an inclusion record exists for the key after step 0/1 (written now or earlier).

**F-21.** Fixed: the LRS host is resolved at each fetch's connect check and on each redirect hop; the deny set is the union of that resolution and the startup resolution.

**F-22.** Fixed: the write mutex is held only for the local disk I/O and consent-state access listed in §5.2.1, never across network I/O. Tightened DD-102 and DD-84.

## Acceptance-test audit (test-by-test)

### Changed

| AT | Finding / change |
| --- | --- |
| AT-85 | Unrestricted only when the Edge file is not unauthenticated as a whole (F-02). |
| AT-90 | Subject-bearing payloads on seal-failed floors are withheld in filter mode; fixture may have none (F-02). |
| AT-112 | Unrestricted only when the file is not unauthenticated as a whole (F-02). |
| AT-134 | Asserts `withheld_count` on the banner (F-14). |
| AT-135 | Unrestricted only when the file is not unauthenticated as a whole (F-02). |
| AT-164 | Colliding host B (with and without `--lrs-url`) mutates nothing; `revoke-pending` / retry files stay byte-identical (F-04). |
| AT-165 | Uses `tiles_total`; includes AT-201 fixtures (F-03, F-18). |
| AT-180 | Later `snapshot_invalid` download is `lrs_status: "skipped"` (F-05). |
| AT-181 | Asserts `revoke-pending` is absent after the startup retry (F-12). |
| AT-183 | Bracketed IPv6 allowlist entry; unbracketed IPv6-with-port is a startup error (F-15). |
| AT-185 | Points at AT-200 for un-consented subject Y (F-02). |
| AT-195 | Uses `tiles_total`; asserts `withheld_count` without requiring a denial (F-02, F-14, F-18). |
| AT-198 | Latch set on receipt; `on` after epoch change returns `off` and writes nothing (F-01). |

### Added

| AT | Finding |
| --- | --- |
| AT-200 | AT-185 fixture plus un-consented Y stays withheld after seal tamper (F-02). |
| AT-201 | Edge signer-attribution cases; also in the AT-165 set (F-03). |
| AT-202 | `drive_path` replaced with a symlink after candidates GET; no secret bytes in `--download-dir` (F-06). |
| AT-203 | `--working-archive` final component must not be a symlink (F-19). |

### Removed

None.

Passed unchanged: AT-01–AT-84, AT-86–AT-89, AT-91–AT-111, AT-113–AT-133, AT-136–AT-163, AT-166–AT-179, AT-182, AT-184, AT-186–AT-194, AT-196, AT-197, AT-199.

## Contract range

Acceptance tests: AT-01–AT-203.
Draft decisions: DD-01–DD-146 (DD-63 retired; DD-126 unused). New: DD-144 (`lrs_inhibit`), DD-145 (Edge signer attribution), DD-146 (open-then-validate via fd).
Open questions: Q-01–Q-17 (Q-03 answered; no new question).
