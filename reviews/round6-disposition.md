# s3a-kite goldfish round 6 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round6.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round6.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v7 (self-contained copy at `/workspace/goldfish/s3a-kite.md`). v6 is archived as `reviews/SPEC-v6.md`.
*   **Totals:** 31 findings (3 Blocker, 13 Major, 15 Minor). 31 fixed in the spec; 0 rejected; 0 deferred.
*   **Grounding:** unchanged — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`).
*   **v7 counts:** AT-01–AT-175 (1–163 reworded in place where the audit required it, no gaps; AT-164–AT-175 new). DD-01–DD-128 (DD-63 remains retired; DD-126 is unused). Open questions: Q-01–Q-16 (Q-03 answered; Q-15 and Q-16 are future, out of scope for this spec).
*   After the edits, a full AT audit was run for AT-01–AT-175 against normative §5 and the DDs (setup possible; expected result matches the rules; can fail on current trees or is marked as a guard). Per-AT results are in the audit section below. The audit changed **23** existing ATs in place and added 12 new ATs.
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

## Ashley's phone-scope decisions (2026-11-16)

Recorded as DD-128, decided by Ashley in chat the same day, including later clarifications:

1. The phone is not a literature-review surface. There is **no phone version of Scholar Explorer** and no Scholar 3-pass / literature-review flow on the phone (no candidate list, select, download, inclusion, snapshots, consent, LRS, or `paper_identity`).
2. In v7 the phone's **in-scope role is the S3A / Edge-tile viewer only**. F-04 therefore adds only `proceed_to_warehouse` and handle `close` (UniFFI drop also releases) for that viewer.
3. The **AI Teacher's Assistant agent has not been built**. AI Teacher's Assistant chat and phone paper search are **future, unbuilt, and out of scope for this spec**. This spec adds no ATs, exports, or interfaces for either, and invents no defaults.
4. Q-15 (assistant backend or model, and whether it would run locally or via the hub) and Q-16 (whether phone paper search would be read-only, and how results would hand off to the desktop or hub for Pass 2/3) are kept as **future questions**. Q-03 (phone Scholar writes / LRS) is answered: Scholar 3-pass stays off the phone.

## Findings

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | AT-84 vs AT-151 / DD-96 on a missing consent file | Fixed (option: split the cases). AT-84 is only an existing malformed consent file (startup error naming the file). A missing file is AT-151 (created as `off`). A missing `--consent-state-file` flag is AT-77. | §5.2.3, AT-84, AT-151, AT-77 | DD-96; AT-84 |
| F-02 | Blocker | AT-18 vs the label table / DD-100 (reordered tiles) | Fixed (option a). Any seal-chain break (`SealedChainBreak`, reordered tiles, mid-chain signature failure, or a committed CRC that breaks the chain) sets file `status` Quarantined and `seal_status` `seal_failed`; earlier valid floors may remain. Torn/uncommitted tiles still do not quarantine (AT-17). AT-18's expected quarantine now matches the table; AT-35 / AT-155 no longer say the file stays Verified or Self-signed. | §5.4 label table and per-tile outcomes, DD-100, DD-111, AT-18, AT-35, AT-155 | DD-111; AT-18, AT-35, AT-155 |
| F-03 | Blocker | AT-55 / AT-123 `file://` `source_url` vs §5.5 null → `no_source` | Fixed (option a). Normalization stands: `source_url` is kept only if it is a valid `http`/`https` URL, else `null`. AT-55 / AT-123 now expect `source_url: null`, no HTTP and no filesystem read of the `file://` target, and `download_error: "no_source"` when there is no usable `drive_path`. Those files open only via `/v1/open`. AT-54 (link-local HTTP) is unchanged. | §5.5 candidates and source table, AT-55, AT-123 | AT-55, AT-123 |
| F-04 | Major | Phone UniFFI lacks `proceed_to_warehouse` and handle close | Fixed, narrowed to the viewer. Phone UniFFI exports `KiteContext::new`, `open`, `open_verified`, `close`, `proceed_to_warehouse`, render, and `hit_test`. Drop of a UniFFI handle also releases it. It does not export `open_path`, `paper_identity`, or any Scholar write. F-04 does not add Scholar exports: Ashley decided the phone is an S3A / Edge-tile viewer only (DD-128). | §5.1, §5.7, AT-96, AT-99 | DD-45, DD-109, DD-128; AT-99 |
| F-05 | Major | `render_floor_transition` needs warehouse scroll | Fixed. `render_floor_transition` and `/v1/frame?view=floor-transition` take `scroll_x` and `scroll_y` (warehouse scroll at click time; default 0 only if omitted). | §5.1, §5.2.1, §5.8 | DD-69 |
| F-06 | Major | Lock scope §5.5 vs DD-102; concurrent double fetch | Fixed. §5.5 now names the disk sub-steps that take the mutex; the fetch is not in the mutex. An in-flight fetch registry keyed by `(file_uuid, identity, verb)` makes concurrent requests wait for one GET. AT-126 uses a source that counts GETs (exactly 1) and returns different bytes per request. | §5.2 write mutex, §5.5 steps, AT-126 | DD-102, DD-112; AT-126 |
| F-07 | Major | Consent-file write failure on revoke must fail closed | Fixed. On revoke: in-memory `off` first, write `--data-dir/revoke-pending` (0600), then the consent file, then delete `lrs-retry`. A consent-file write failure returns `500` `consent_write_failed`, leaves `revoke-pending`, and sends no LRS. Startup with `revoke-pending` treats consent as `off` and retries; a failed `lrs-retry` delete at that startup is a startup error naming `lrs-retry` (no bind). | §5.2 revoke, error catalogue, AT-124, AT-169 | DD-113; AT-124, AT-169 |
| F-08 | Major | Multi-process exclusive lock on data dir / working archive | Fixed. Scholar mode takes a process-lifetime exclusive `flock` on `--data-dir/.lock` and `--working-archive.lock`. A second host on the same path is a startup error naming the flag (AT-164). AT-46 uses distinct paths. | §5.2, §5.6, AT-46, AT-164 | DD-114; AT-46, AT-164 |
| F-09 | Major | Failed signature from a known signer must not stay Verified | Fixed together with F-02. "Trusted signer" is renamed "known signer" (pinned, or embedded when the pin list is empty). A failed signature from a known signer sets file-level Quarantined; earlier floors may remain. An unpinned signer on a non-empty list is still whole-file Quarantined with no valid floors. | §5.4, AT-35, AT-155 | DD-100, DD-111; AT-35, AT-155 |
| F-10 | Major | DD-19 vs pack vs reader-core verification | Fixed. Verification exists in both places; the pack algorithm is normative for the host. The host always runs pack `open` and does not prefer `open_verified`. AT-165 requires pack `open` and a mapped reader-core `verify` to agree on `status`, `seal_status`, and `invalid_tile_indices` for the listed fixtures and both pin-list cases. | §5.1, §5.7, DD-19, AT-165 | DD-19, DD-115; AT-165 |
| F-11 | Major | `unverified` missing from the report enum | Fixed. `KiteVerificationReport.status` includes `unverified`. `open_verified` returns `KiteError` for that status (AT-154). It is never painted as a UI label. | §5.1, §5.4, AT-154 | DD-116; AT-154 |
| F-12 | Major | Candidate list must apply consent filtering | Fixed. Candidate extraction applies the §5.3 item 4 / §5.4 filter and valid-floor rules. Withheld or invalid records are not candidates. A Quarantined corpus file contributes no `AcademicPaperRecord` candidates. | §5.5, AT-166 | DD-117; AT-166 |
| F-13 | Major | HTML local subresource loads under path rules | Fixed, narrower than allowing local-with-path-rules: v1 page view loads no subresources of any kind (network or local). AT-121 covers remote tags; AT-167 covers a relative escape and a local `<link>`. | §5.1, §5.2 item 5, §5.5, AT-121, AT-167 | DD-77, DD-118; AT-121, AT-167 |
| F-14 | Major | Error table gaps | Fixed. Catalogue adds `400` `invalid_param`, `405` `method_not_allowed`, `415` `unsupported_media_type`, `409` `not_regular_file`, `403` `unreadable`, `409` `file_too_large`, `500` `consent_write_failed`, `500` `render_failed`, and `500` `internal`. AT-171 covers the HTTP gaps; AT-169 / AT-170 cover the write and size codes. | §5.2.1 error catalogue, AT-171 | DD-119; AT-171, AT-169, AT-170 |
| F-15 | Major | Idempotent select still emitted a new LRS statement | Fixed. Step 0 looks up the last matching index line (`file_uuid` + identity + verb). If present, skip step 1 and step 3 (no new LRS statement); run step 2 only if `snapshot_sha256` is null and a fetch is required; run 2b for download. DD-34 is aligned with that key. | §5.5 numbered steps, DD-34 | DD-34, DD-120 |
| F-16 | Major | `kite-acceptance` / dangling "except as listed later" | Fixed. `kite-acceptance` is excluded from workspace `default-members`; host-spawn tests fail at runtime if the binary is missing. The dangling "except as listed later with reasons" clause is deleted from §7. | §5.1 test locations, §7 | DD-121 |
| F-17 | Minor | POST `on` without `--lrs-url`; POST `off` without a file | Fixed. POST `on` without `--lrs-url` is reachable and returns `409` `lrs_not_configured`; no file is created at any path (AT-116). POST `off` without a configured consent file is `200` `{off, null}` (AT-175). | §5.2.1, §5.2.3, AT-116, AT-175 | AT-116, AT-175 |
| F-18 | Minor | "Request paths opened only under `--view-root`" was unscoped | Fixed. Prefixed with "In viewer-only mode,". Scholar allowed roots stay as in DD-89. | §5.2 | — |
| F-19 | Minor | Health and bind wording in AT-75 / AT-76 | Fixed. AT-75 uses "tokenless health case." AT-76 starts with `--bind 127.0.0.1`; `--headless` is optional on loopback. | AT-75, AT-76 | AT-75, AT-76 |
| F-20 | Minor | SemanticNode coordinates vs hit-test | Fixed. `SemanticNode` `x`/`y`/`width`/`height` are CSS pixels, the same space as `/v1/hit-test`. AT-168 covers a `device_pixel_ratio=2` hit-test. | §5.8, AT-168 | DD-124; AT-168 |
| F-21 | Minor | Hyper-tile committed CRC vs later tiles and consent | Fixed. A committed CRC failure invalidates that tile and all later tiles (same as Edge, so AT-35 holds). `seal_status` is `seal_failed` if a trailer is present, else `unsealed`; `status` is still Quarantined. Consent and mark records are still collected from parseable invalid tiles. | §5.3 item 4, §5.4, AT-35 | DD-111; AT-35 |
| F-22 | Minor | Font injection and AT-42 | Fixed. No process-global font set. `KiteContext::new(ua_fonts)` takes the bytes; an empty set is `KiteError`. AT-42 is a library-level test on the crate the host links. | §5.1, §5.7, AT-42 | DD-101, DD-121; AT-42 |
| F-23 | Minor | Download / snapshot publish and temp cleanup | Fixed. Snapshot and download use temp + fsync + rename. Temp names are `s3a-kite-tmp-*` and are not openable via `/v1/open`. Stale same-euid temps are deleted at startup. | §5.6 | DD-122 |
| F-24 | Minor | Withheld payloads still visible in the text layer | Fixed. A withheld payload must not appear in `text_runs`, `semantics`, pixels, or hit-test targets. AT-11 and AT-23 search for a withheld unique string. | §5.1, §5.3 item 4, AT-11, AT-23 | AT-11, AT-23 |
| F-25 | Minor | `download_conflict` vs included, and LRS status on 200 | Fixed. Keep `409` `download_conflict`, and add `included: true` in the error body when step 1 succeeded (AT-94). A 200 fetch-fail path sets `download_error: "fetch_failed"`. 200 select/download responses include `lrs_status`. | §5.2.1, §5.6, AT-94 | DD-119; AT-94 |
| F-26 | Minor | Open size cap | Fixed. `--max-open-bytes` is required in both host modes, and FFI `open` / `open_path` take `max_open_bytes`. Exceeding it is `409` `file_too_large` / `KiteError` (AT-170). ATs 04 / 76 / 77 build from the inventory and pick the flag up. | §5.2.2, §5.3 item 7, AT-170 | DD-123; AT-170 |
| F-27 | Minor | Startup errors must supply every other valid flag | Fixed. Scholar mode is the absence of `--viewer`. ATs that assert a named startup error must supply every other flag valid for that mode. The host may name any one of multiple errors (no prescribed order) but exits non-zero and binds nothing. AT-125 with `--headless` on `0.0.0.0` also passes TLS flags. | §5.2, AT-51, AT-125 | DD-127; AT-125 |
| F-28 | Minor | AT-57 token, AT-156 revoke during LRS, AT-145 disjointness | Fixed. AT-57 uses a Bearer value read from a different token file. AT-156 adds a slow LRS fixture and a revoke during the LRS POST. AT-145 adds the `--view-root` / write-root disjointness converses. | AT-57, AT-156, AT-145 | AT-57, AT-145, AT-156 |
| F-29 | Minor | Create-time ConsentRecord filter-mode side effect | Fixed. The create-time record stays (preamble exception). Because it makes the subject map non-empty, a payload with a different `subject_id` in the working archive is withheld (AT-173). The grant is for the researcher's subject only. | §5.6, preamble, AT-173 | DD-105; AT-173 |
| F-30 | Minor | Kind/view matrix not applied to hit-test | Fixed. The §5.2.6 matrix applies to `/v1/frame` and `/v1/hit-test` and is checked before `no_frame`. The last-frame cache is keyed by `(view, floor)`. AT-172 covers warehouse hit-test before proceed. | §5.1, §5.2.6, AT-172 | DD-125; AT-172 |
| F-31 | Minor | Unix-socket parent directory permissions | Fixed. The parent directory must exist, be euid-owned, and must not be group- or world-writable; otherwise startup is an error naming the path (AT-174). | §5.2 Unix-socket bind, AT-174 | DD-108, DD-127; AT-174 |

## Blocker resolutions

*   **F-01 (AT-84 vs AT-151 / DD-96):** The three missing-or-bad consent cases are now distinct tests and distinct rules. AT-84 = existing malformed file, startup error naming the file. AT-151 = missing file, created as `off`. AT-77 = missing `--consent-state-file` flag when `--lrs-url` is set.
*   **F-02 (AT-18 vs label table / DD-100):** Option (a). Any seal-chain break, including reordered tiles, is file-level Quarantined with `seal_status` `seal_failed`; earlier valid floors may remain. That also resolves F-09 (a failed signature from a known signer must not stay Verified). AT-18 did not need a result change once the table matched it.
*   **F-03 (AT-55 / AT-123 `file://` vs null):** Option (a). Keep normalization (`source_url` only if valid `http`/`https`, else `null`). Reworded AT-55 and AT-123 to expect `null`, no HTTP, no filesystem read of the `file://` target, and `no_source` when there is no usable `drive_path`.

## New draft decisions and questions

*   **New DDs:** DD-111 (seal-chain break and known-signer failure), DD-112 (in-flight fetch registry), DD-113 (revoke-pending / `consent_write_failed`), DD-114 (process exclusivity), DD-115 (pack verification normative for the host), DD-116 (`unverified` in the report enum), DD-117 (candidate consent and trust filter), DD-118 (no local or network subresources), DD-119 (expanded error catalogue), DD-120 (idempotent step 0), DD-121 (`KiteContext` fonts and `kite-acceptance`), DD-122 (atomic download and temp cleanup), DD-123 (`--max-open-bytes`), DD-124 (SemanticNode CSS pixels), DD-125 (hit-test view matrix), DD-127 (startup error precedence), DD-128 (phone in-scope role, Ashley). **DD-126 is unused** (hit-test matrix lives on DD-125). DD-19, DD-34, DD-45, DD-77, DD-100, DD-102, and DD-109 were tightened in place. DD-63 remains retired.
*   **New questions:** Q-15 and Q-16, both marked future and out of scope for this spec (see Ashley's phone-scope decisions). Q-03 is answered. No defaults were invented for Q-15 or Q-16.

## Full AT audit (AT-01–AT-175)

For every AT, the audit checked: (1) the setup is possible from the contract and fixtures; (2) the expected result matches normative §5 and the DDs after the v7 edits; (3) the test can fail on today's trees, or is tagged as a guard. **Changed** means the AT text was reworded in place. **New** means the AT was added as AT-164 or later. **Pass** means the existing text already matched and was left as-is.

*   **Pass:** 140
*   **Changed in place:** 23 (AT-11, 23, 35, 42, 43, 46, 55, 57, 75, 76, 84, 94, 96, 99, 116, 121, 123, 124, 125, 126, 145, 155, 156)
*   **New:** 12 (AT-164–AT-175)
*   **Audit-changed existing ATs:** **23**
*   **AT count:** AT-01–AT-175 (175, no gaps)

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
| AT-011 | changed | Withheld unique string must be absent from text_runs, semantics, pixels, and hits (F-24). |
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
| AT-023 | changed | Same withheld-content exclusion for Edge event-log payloads (F-24). |
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
| AT-035 | changed | Known-signer signature failure and committed CRC set file status Quarantined; earlier floors may remain (F-02, F-09, F-21). |
| AT-036 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-037 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-038 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-039 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-040 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-041 | pass | Guard as tagged; expected result matches §5 / DDs; setup possible or skipped until the subject exists. |
| AT-042 | changed | Library-level test is KiteContext::new with an empty font set (F-22). |
| AT-043 | changed | Warehouse after proceed_to_warehouse, not after Continue (DD-97). |
| AT-044 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-045 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-046 | changed | Second host uses a distinct --data-dir and --working-archive (F-08, DD-114). |
| AT-047 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-048 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-049 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-050 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-051 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-052 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-053 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-054 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-055 | changed | file:// source_url is listed as null; no HTTP or filesystem fetch; no_source when no drive_path (F-03). |
| AT-056 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-057 | changed | Bearer value is read from a different token file (F-28). |
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
| AT-073 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-074 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-075 | changed | Uses the tokenless health case wording (F-19). |
| AT-076 | changed | Starts with --bind 127.0.0.1; --headless optional on loopback; inventory includes --max-open-bytes (F-19, F-26). |
| AT-077 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-078 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-079 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-080 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-081 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-082 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-083 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-084 | changed | Malformed existing consent file only; missing file is AT-151; missing flag is AT-77 (F-01). |
| AT-085 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-086 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-087 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-088 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-089 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-090 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-091 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-092 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-093 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-094 | changed | 409 download_conflict body includes included: true (F-25). |
| AT-095 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-096 | changed | Pack exports add KiteContext::new, close, and max_open_bytes; paper_identity is host-only (F-04, F-26, DD-128). |
| AT-097 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-098 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-099 | changed | Phone UniFFI includes proceed_to_warehouse and close, and omits paper_identity and Scholar writes (F-04, DD-128). |
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
| AT-111 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-112 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-113 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-114 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-115 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-116 | changed | POST on without --lrs-url is reachable 409 and creates no file; POST off is 200 no-op (F-17). |
| AT-117 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-118 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-119 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-120 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-121 | changed | Zero network and zero local subresource loads (F-13). |
| AT-122 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-123 | changed | Covered with AT-55: file:// is null, not fetched (F-03). |
| AT-124 | changed | 500 revoke_delete_failed; next start fail-closed naming lrs-retry (F-07). |
| AT-125 | changed | Non-loopback --headless supplies TLS flags; named-flag cases supply every other valid flag (F-27). |
| AT-126 | changed | Fixture counts GETs (exactly 1) and returns different bytes per request (F-06). |
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
| AT-145 | changed | Adds the disjointness converses for write roots vs wiki/corpus roots (F-28). |
| AT-146 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-147 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-148 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-149 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-150 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-151 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-152 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-153 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-154 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-155 | changed | Failed signature from a known signer is Quarantined; earlier floors may remain (F-09). |
| AT-156 | changed | Adds revoke during a slow LRS POST, not only during source fetch (F-28). |
| AT-157 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-158 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-159 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-160 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-161 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-162 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-163 | pass | Matches normative §5 and DDs; setup possible; fail-first on current trees. |
| AT-164 | new | Second host on the same data-dir or working-archive is a startup error (F-08). |
| AT-165 | new | Pack open and mapped reader-core verify agree on status, seal_status, invalid_tile_indices (F-10). |
| AT-166 | new | Candidate list applies consent filtering; Quarantined files contribute no AcademicPaperRecord (F-12). |
| AT-167 | new | Relative/local HTML subresources are not loaded (F-13). |
| AT-168 | new | DPR=2 hit-test uses CSS coordinates, same space as SemanticNode (F-20). |
| AT-169 | new | Consent-file write failure on revoke is 500 consent_write_failed and fail-closed via revoke-pending (F-07). |
| AT-170 | new | Open exceeding max-open-bytes is 409 file_too_large / KiteError (F-26). |
| AT-171 | new | Catalogue gaps: invalid_param, method_not_allowed, unsupported_media_type, not_regular_file, unreadable (F-14). |
| AT-172 | new | Hit-test view matrix is checked before no_frame (F-30). |
| AT-173 | new | Create-time ConsentRecord withholds a foreign subject_id in the working archive (F-29). |
| AT-174 | new | Unix-socket parent must be euid-owned and not group/world-writable (F-31). |
| AT-175 | new | Covered with AT-116: POST off without a consent file is 200 no-op (F-17). |

The next goldfish should re-read this table against the v7 §5 text; stale-AT blockers in earlier rounds came from expected results that no longer matched the rules.
