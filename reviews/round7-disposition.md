# s3a-kite goldfish round 7 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round7.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round7.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v8 (self-contained copy at `/workspace/goldfish/s3a-kite.md`). v7 is archived as `reviews/SPEC-v7.md`.
*   **Totals:** 22 findings (1 Blocker, 5 Major, 16 Minor). 22 fixed in the spec; 0 rejected; 0 deferred.
*   **Grounding:** unchanged — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`). The F-06 mapping is grounded in `s3a-reader-core/src/verify.rs` and `integrity.rs` at that pin (`VerificationReport`, `VerificationStatus`, `SealInfo`, `SealState`, `tiles_crc_failed`, and tile `crc` / `problem`).
*   **v8 counts:** AT-01–AT-183 (1–175 reworded in place where the audit required it, no gaps; AT-176–AT-183 new). DD-01–DD-133 (DD-63 remains retired; DD-126 remains unused). Open questions: Q-01–Q-16 (Q-03 answered; Q-15 and Q-16 remain future and out of scope; no new question).
*   After the edits, a full AT audit was run for AT-01–AT-183 against normative §5 and the DDs (setup possible; expected result matches the rules; can fail on current trees or is marked as a guard). Per-AT results are in the audit section below. The audit changed **9** existing ATs in place and added 8 new ATs.
*   Phone scope is unchanged (DD-128): the phone is an S3A / Edge-tile viewer only. No ATs, exports, or interfaces were added for Scholar Explorer, AI Teacher's Assistant chat, or phone paper search.
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

## Findings

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | Consent records from invalid or untrusted tiles can grant; Edge contradicts DD-111 | Fixed (option a, as Ashley preferred). Two-pass restrict-only rule: latest-wins from valid floors only, then withdrawals or denials from parseable invalid or unauthenticated tiles as extra restrictions; grants in those tiles are ignored. The same rule is stated in §5.3 item 4, §5.4 item 6, and DD-111. New AT-176, AT-177, AT-178. AT-35 and AT-85 reworded. | §5.3 item 4, §5.4 item 6, DD-111, AT-35, AT-85, AT-176–AT-178 | DD-111; AT-176, AT-177, AT-178 |
| F-02 | Major | HTTP `t` must be an integer, but animation needs fractional `t` | Fixed. `t` is a finite decimal. Non-numeric or non-finite `t` is `400` `invalid_param`. Finite out-of-range values are clamped, not errors. AT-171 covers `t=0.225` → 200 and `t=-1` → 200 (clamped). | §5.2.1 catalogue, §5.8, AT-171 | AT-171 |
| F-03 | Major | Secrets and state files can sit inside a request root and be painted | Fixed (option a). At startup, token, TLS cert/key, LRS credentials, pinned keys, consent file, `--data-dir`, and `--log-dir` must not canonicalize inside any request root. `--data-dir` and `--log-dir` are also disjoint from `--snapshot-dir` and `--download-dir`. Viewer-only applies the rule to `--log-dir` / token / TLS vs `--view-root`. AT-179. | §5.2.2, DD-89, DD-93, DD-129, AT-179 | DD-129; AT-179 |
| F-04 | Major | Snapshot reuse trusts whatever file already sits at `{sha}.s3a` | Fixed (option a). Before reuse the host runs `verify_sealed_archive` and checks BLOB reassembly hashes to `sha`. Failure is `409` `snapshot_conflict` (inclusion kept, no overwrite). Download reassembly failure is `download_error: "snapshot_invalid"`. AT-180. | §5.6, DD-103, AT-180 | DD-103; AT-180 |
| F-05 | Major | Which lock guards the working archive; AT-73 vs AT-164 | Fixed. Two locks are named: (1) process-lifetime sidecar `flock`s (host vs host, AT-164); (2) engine `try_lock_exclusive` on the archive file per write. An external holder of (2) is `500` `lock_failed`. AT-73 now uses an external `TileWriter`, not a second host. `/v1/open` of the working archive copies under the write mutex. | §5.2.1, §5.6, DD-114, AT-73 | DD-114; AT-73 |
| F-06 | Major | AT-165 cannot live in kite-acceptance; mapping undefined | Fixed (option a). AT-165 moves to s3a-reader, feature-gated with AT-38/39/43/63. The §5.1 table maps reader-core `VerificationReport` / `SealInfo` (`792742c`) onto `KiteVerificationReport`, including torn tiles in `invalid_tile_indices` (DD-130). | §5.1, §5.7, §7, AT-165 | DD-115, DD-130; AT-165 |
| F-07 | Minor | FFI `hit_test` has no `floor` | Fixed (option a). Pack `hit_test` takes `floor: Option<u32>`. Omitted `floor` on floor-scoped views is `400` `missing_param`. HTTP already had `floor`. | §5.1, §5.2.1, DD-125 | DD-125 |
| F-08 | Minor | Select/download share one fetch vs registry keyed by verb | Fixed. In-flight registry and snapshot-exists lookup are keyed by `(file_uuid, identity)`. The verb stays only on the inclusion-record idempotency key. | §5.2.1, §5.5, DD-112 | DD-112 |
| F-09 | Minor | Step 0, step 1, and registry insert were not atomic | Fixed. They run in one mutex hold. | §5.5, DD-102 | DD-102 |
| F-10 | Minor | "Update the index line" vs append-only | Fixed. Step 2 appends a new index line with the snapshot SHA-256. | §5.5, DD-104 | DD-104 |
| F-11 | Minor | SemanticNode CSS space vs pixmap frame width/height | Fixed. Coordinates share a space with `hit_test` x/y and the request's CSS `w`/`h`, not the pixmap fields in the frame response. | §5.8, DD-124 | DD-124 |
| F-12 | Minor | DD-113 said "remove the queue file" | Fixed. DD-113 now says "then remove `revoke-pending`". | DD-113 | DD-113 |
| F-13 | Minor | Startup consent-file write failure with `revoke-pending` undefined | Fixed. That startup failure names `--consent-state-file`, binds nothing, and leaves `revoke-pending`. AT-169 updated. | §5.2.1, AT-169 | DD-113; AT-169 |
| F-14 | Minor | POST `on` after `consent_write_failed` undefined | Fixed. Later `POST on` in the same process returns `409` `revoke_pending`; GET and in-memory state stay `off` (AT-181). | §5.2.1, AT-181 | DD-113; AT-181 |
| F-15 | Minor | AT-111 allowed "not sendable" vs startup error | Fixed. AT-111 now says the queue is deleted, or startup fails naming `lrs-retry` (AT-124). | AT-111 | AT-111, AT-124 |
| F-16 | Minor | When `lrs_status` is `failed` vs `queued` | Fixed. `queued` = attempt failed and a retry file was written; `failed` = attempt failed and no retry file could be written, or an in-flight POST was aborted by revoke (AT-156 returns `failed`). `sent` / `skipped` / `not_configured` are also defined (DD-131). | §5.2.1, AT-156 | DD-131; AT-156 |
| F-17 | Minor | Missing JSON body fields have no error code | Fixed. `missing_param` covers required JSON body fields. An unknown consent `state` is `invalid_param`. AT-171 covers both. | §5.2.1, AT-171 | AT-171 |
| F-18 | Minor | AT-175 missing from the test-location list | Fixed. The list is AT-64–AT-183 (except the reader-only tests, which now include AT-165). | §5.1, §7 | — |
| F-19 | Minor | AT-76 omits `--headless` | Fixed. AT-76 always passes `--headless` so it runs on display-less CI and on builds without `native-window`. The flag remains optional on loopback for a `native-window` build with a display. | AT-76 | AT-76 |
| F-20 | Minor | `drive_path` root set vs DD-89 | Fixed. A `drive_path` that canonicalizes inside `--snapshot-dir` or `--download-dir` is not a usable on-disk source. Those dirs remain Scholar `/v1/open` roots. DD-89 aligned. | §5.5, DD-89 | DD-89 |
| F-21 | Minor | Content validation of pinned-keys and LRS credentials undefined | Fixed. Invalid key line, or a credentials file with no line / no `:`, is a startup error naming the file. Zero pinned-key lines is a valid empty list. Credential passwords may contain `:` (split at the first colon). AT-182. | §5.4, §5.5, AT-182 | DD-132; AT-182 |
| F-22 | Minor | `--egress-allowlist` port rule ambiguous | Fixed. Matching uses the effective port (explicit, or scheme default 80/443). An entry without a port matches only the scheme default. `host:443` matches `https://host/`. AT-183. | §5.2.4, AT-183 | DD-133; AT-183 |

## Blocker and major resolutions

*   **F-01 (consent from invalid tiles can grant):** Option (a), two-pass restrict-only, as preferred in the tasking. Valid-floor records build the latest-wins map. Withdrawals or denials in parseable invalid or unauthenticated tiles (CRC-failed, post-break, unpinned signer) are extra restrictions only. A grant in those tiles is ignored. Hyper-tile §5.3, Edge §5.4 item 6, DD-111, AT-35, and AT-85 now say the same thing. AT-176 / AT-177 / AT-178 cover the three attack cases in the report.
*   **F-02 (HTTP `t`):** `t` is a finite decimal; out-of-range finite values are clamped; AT-171 asserts `t=0.225` and `t=-1` return 200.
*   **F-03 (secrets in a request root):** Option (a). Startup refuses secret/state paths inside any request root, and keeps `--data-dir` / `--log-dir` disjoint from snapshot and download dirs, including in viewer mode (AT-179).
*   **F-04 (unverified snapshot reuse):** Option (a). Reuse only after seal verification and a BLOB reassembly that hashes to the name; otherwise `409` `snapshot_conflict`. Reassembly failure on download is `snapshot_invalid` (AT-180).
*   **F-05 (locks / AT-73):** Sidecar process locks (AT-164) are distinct from engine `try_lock_exclusive` on the archive file. AT-73 uses an external `TileWriter`. Working-archive `/v1/open` copies under the write mutex.
*   **F-06 (AT-165 location and mapping):** Option (a). AT-165 lives in s3a-reader. The §5.1 / DD-130 table defines `status`, `seal_status`, `tiles_crc_failed`, `invalid_tile_indices` (including torn tiles), and `detail`.

## New draft decisions and questions

*   **New DDs:** DD-129 (secret/state vs request roots), DD-130 (reader-core mapping), DD-131 (`lrs_status` values), DD-132 (pinned-keys and credentials validation), DD-133 (egress allowlist effective port). Tightened in place: DD-89, DD-93, DD-102, DD-103, DD-104, DD-111, DD-112, DD-113, DD-114, DD-115, DD-124, DD-125. DD-63 remains retired. DD-126 remains unused. DD-128 is unchanged.
*   **New questions:** none. Q-15 and Q-16 stay future and out of scope.

## Full AT audit (AT-01–AT-183)

For every AT, the audit checked: (1) the setup is possible from the contract and fixtures; (2) the expected result matches normative §5 and the DDs after the v8 edits; (3) the test can fail on today's trees, or is tagged as a guard. **Changed** means the AT text was reworded in place. **New** means the AT was added as AT-176 or later. **Pass** means the existing text already matched and was left as-is.

*   **Pass:** 166
*   **Changed in place:** 9 (AT-35, 73, 76, 85, 111, 156, 165, 169, 171)
*   **New:** 8 (AT-176–AT-183)
*   **Audit-changed existing ATs:** **9**
*   **AT count:** AT-01–AT-183 (183, no gaps)

Guards remain AT-05, AT-22, AT-34, AT-36, AT-37, AT-39, and AT-41 (tagged in the AT line; not claimed to fail first).

| AT | Result | Note |
| --- | --- | --- |
| AT-001 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-002 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-003 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-004 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-005 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-006 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-007 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-008 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-009 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-010 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-011 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-012 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-013 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-014 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-015 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-016 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-017 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-018 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-019 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-020 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-021 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-022 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-023 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-024 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-025 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-026 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-027 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-028 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-029 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-030 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-031 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-032 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-033 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-034 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-035 | changed | Consent from invalid tiles can only restrict, never release (F-01). |
| AT-036 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-037 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-038 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-039 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-040 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-041 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-042 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-043 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-044 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-045 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-046 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-047 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-048 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-049 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-050 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-051 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-052 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-053 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-054 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-055 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-056 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-057 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-058 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-059 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-060 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-061 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-062 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-063 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-064 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-065 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-066 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-067 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-068 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-069 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-070 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-071 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-072 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-073 | changed | Uses an external TileWriter, not a second host (F-05). |
| AT-074 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-075 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-076 | changed | The test always passes --headless so it runs on display-less CI (F-19). |
| AT-077 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-078 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-079 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-080 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-081 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-082 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-083 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-084 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-085 | changed | Unrestricted only when no type-71 record exists in any tile; CRC-failed denial is AT-177 (F-01). |
| AT-086 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-087 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-088 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-089 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-090 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-091 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-092 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-093 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-094 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-095 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-096 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-097 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-098 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-099 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-100 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-101 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-102 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-103 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-104 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-105 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-106 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-107 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-108 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-109 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-110 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-111 | changed | Queue is deleted, or startup fails naming lrs-retry (F-15). |
| AT-112 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-113 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-114 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-115 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-116 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-117 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-118 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-119 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-120 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-121 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-122 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-123 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-124 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-125 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-126 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-127 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-128 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-129 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-130 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-131 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-132 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-133 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-134 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-135 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-136 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-137 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-138 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-139 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-140 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-141 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-142 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-143 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-144 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-145 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-146 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-147 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-148 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-149 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-150 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-151 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-152 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-153 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-154 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-155 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-156 | changed | Revoke during an LRS POST yields lrs_status failed and no new retry file (F-16). |
| AT-157 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-158 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-159 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-160 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-161 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-162 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-163 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-164 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-165 | changed | Moved to s3a-reader; uses the §5.1 mapping table, including torn tiles (F-06). |
| AT-166 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-167 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-168 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-169 | changed | Failed consent-file write at startup names --consent-state-file and leaves revoke-pending (F-13). |
| AT-170 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-171 | changed | t is a finite decimal (t=0.225 and t=-1 clamp to 200); missing JSON fields; unknown state is invalid_param (F-02, F-17). |
| AT-172 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-173 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-174 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-175 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-176 | new | Valid withdrawal plus later CRC-failed grant stays withheld (F-01). |
| AT-177 | new | Edge CRC-failed type-71 denial withholds; not unrestricted (F-01). |
| AT-178 | new | Post-break or unpinned grant cannot override a valid-floor denial (F-01). |
| AT-179 | new | Secret/state files and data/log dirs must not sit in request roots (F-03). |
| AT-180 | new | Planted snapshot fails reuse checks: 409 snapshot_conflict; reassembly failure is snapshot_invalid (F-04). |
| AT-181 | new | POST on after consent_write_failed is 409 revoke_pending; GET stays off (F-14). |
| AT-182 | new | Pinned-keys and LRS credentials file validation at startup (F-21). |
| AT-183 | new | Egress allowlist matches the effective port; entry without a port is scheme default only (F-22). |
