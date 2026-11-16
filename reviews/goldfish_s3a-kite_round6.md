# Goldfish review: s3a-kite spec v6, round 6

Reviewer: independent, no prior context. Source read: `/workspace/goldfish/s3a-kite.md` only.
Rules applied: DDs, Q-xx, deliberate non-goals, and external requirements are not findings unless the spec contradicts itself about them.

## Summary

| Severity | Count |
| --- | --- |
| Blocker | 3 |
| Major | 13 |
| Minor | 15 |
| **Total** | **31** |

Blockers are acceptance tests that cannot pass under the normative text, or two tests that require opposite behavior.

---

## Blockers

### F-01: AT-84 contradicts DD-96 / AT-151 on a missing consent file (Blocker)
- **Ref:** AT-84; §5.2.3; DD-96; AT-151
- **Quote:** AT-84: "With `--lrs-url` set, a missing or malformed `--consent-state-file` is a startup error that names the file … does not bind a port." AT-151: "a `--consent-state-file` path that does not yet exist creates that file as `off\n` … and the host starts."
- **Problem:** With the same flags, AT-84 requires a missing file to fail startup and AT-151 requires it to start. Both can't pass. If AT-84 means the *flag* is missing, that case is already covered by AT-77, and "names the file" doesn't make sense.
- **Fix:** Change AT-84 to "a **malformed** consent file is a startup error that names the file; a missing file is AT-151", or reword it to "missing `--consent-state-file` flag … names the flag."

### F-02: AT-18 expects quarantine for a reordered tile, but the label table and DD-100 don't give it (Blocker)
- **Ref:** AT-18; §5.4 item 2 label table; DD-100; AT-35/AT-155
- **Quote:** AT-18: "A copy with a reordered or mutated tile fails verification, shows quarantine…". Table: "Seal chain broken … | Quarantined if the break is a committed CRC, else as signatures". DD-100: "Mid-chain sig fail with a trusted signer: later tiles invalid, status stays Verified or Self-signed."
- **Problem:** Reordering tiles in MICRO-VEC-03 keeps each tile's CRC valid and breaks the chain. Under the table, the status then comes from the signatures. Those are Self-signed (empty pin list) or Verified (pinned), and neither is Quarantined. AT-18 requires "shows quarantine", so it fails against a correct implementation. A mutated tile whose CRC was recomputed hits the same problem.
- **Fix:** Choose one rule. Either (a) any seal-chain break (`SealedChainBreak`) sets the file status to Quarantined, and the table and DD-100 change to match; or (b) AT-18 asserts `seal_status = seal_failed` with the break index in the banner, not "quarantine".

### F-03: AT-55 / AT-123 expect `fetch_error` for a `file://` source_url, but §5.5 turns that URL into null (Blocker)
- **Ref:** §5.5 candidate list; source decision table; AT-55; AT-123; Glossary "Source URL"
- **Quote:** §5.5: "`source_url` (from the optional YAML key `source_url` if it is a valid `http`/`https` URL, else `null`)". AT-55: "a current candidate whose `source_url` is a `file://` URL … is rejected … `fetch_error` is set".
- **Problem:** A `file://` value is turned into `null` when the candidate is built. The candidate then falls into the "neither" or "on-disk" row: no fetch happens, `download_error: "no_source"` is returned, and `fetch_error` is null. AT-55's `fetch_error` assertion can never hold. AT-54's `http://169.254.169.254/` case is fine.
- **Fix:** Choose one. (a) Keep the normalization and change AT-55/123 to assert `source_url: null` in `/v1/candidates`, no read of the target file, and `no_source` on download. Or (b) keep non-http schemes as a "rejected source" candidate attribute that gives `fetch_error: "scheme_not_allowed"`, and say so in §5.5 and §5.2.4.

---

## Major

### F-04: The phone UniFFI surface leaves out `proceed_to_warehouse` (and any handle release), but phone tests need it (Major)
- **Ref:** §5.1 (para after Hit list); §5.7; DD-45; DD-109; AT-38; AT-43; AT-99
- **Quote:** §5.1: "v1 UniFFI on the phone exports `open`, `open_verified`, the render functions, `hit_test`, and `paper_identity` only." AT-38: "`open` + `render_manifest` + `proceed_to_warehouse` + `render_warehouse` return…".
- **Problem:** `proceed_to_warehouse` isn't a `render_*` function, and "only" excludes it. Without it the phone can't leave the first view: Continue is pure (DD-97), and the matrix gives 409 for `warehouse` before proceed. That breaks AT-38 and AT-43. The pack API also has no `close`/release, so phone handles (and the "last frame per view" they keep) have no defined lifetime.
- **Fix:** Add `proceed_to_warehouse` (and a `close(handle)` or a UniFFI object-drop contract) to the phone export list in §5.1, §5.7, DD-45, DD-109, and AT-99's allowed set.

### F-05: `render_floor_transition` has no warehouse scroll or geometry input, but the animation starts at the bay rectangle "at the given warehouse scroll" (Major)
- **Ref:** §5.1 signature; §5.8 Animation; DD-18; AT-62
- **Quote:** Signature: "`render_floor_transition(handle, floor_index, t_seconds, width, height, instant, device_pixel_ratio)`". §5.8: "The start rectangle is the bay's layout rectangle in frame coordinates at the given warehouse scroll".
- **Problem:** No `scroll_x`/`scroll_y` is passed, and `/v1/frame` defaults them to 0 for this view. The start rectangle (the t=0 frame) is therefore undefined unless the pack implicitly remembers the last `render_warehouse` scroll and viewport. Hidden state like that conflicts with "hit_test is pure" style determinism and isn't specified. AT-62 can't pin t=0 within 1 CSS px.
- **Fix:** Add `scroll_x, scroll_y` to `render_floor_transition` and to the `/v1/frame` floor-transition contract, defined as the warehouse scroll at click time. Alternatively, state normatively that the pack uses the last `render_warehouse` scroll for that handle, and test it.

### F-06: The write mutex scope contradicts itself, and concurrent selects can each fetch; AT-126 can't detect this (Major)
- **Ref:** §5.5 "Numbered write steps"; §5.2.1 "Write mutex (DD-102)"; DD-84; AT-126
- **Quote:** §5.5: "Select and download share this order, under the process-wide write mutex: 1 … 2. If the source table requires a fetch, fetch and write the snapshot". §5.2.1: the mutex "is **not** held across the source fetch".
- **Problem:** (1) The two statements conflict. (2) Under DD-102, select A writes step 1 and releases the mutex to fetch. Select B then sees the index line with `snapshot_sha256: null`. Failed snapshots "may be retried", and an in-flight fetch isn't tracked, so B fetches as well. That is two GETs. If the source returns different bytes, the result is two snapshots and two index lines, which contradicts "at most one snapshot". AT-126 uses a deterministic fixture, so content addressing hides the duplicate fetch, and the test can't fail.
- **Fix:** Define an in-flight-fetch registry per (file_uuid, identity, verb). Concurrent requests either wait for the in-flight result or don't start a second fetch. Reword §5.5 to say which sub-steps run under the mutex. Extend AT-126 so the fixture counts GETs (exactly 1) and returns different bytes per request.

### F-07: A failed consent-file write on revoke has no defined outcome, so tracing can resume after restart (Major, privacy)
- **Ref:** §5.2.1 "Consent revoke and in-flight LRS"; §5.5 Retry queue; error catalogue; AT-124
- **Quote:** "sets an atomic in-memory consent flag to `off` **before** waiting for the write mutex, then under the mutex writes the consent file and deletes `--data-dir/lrs-retry/`." The only revoke error is `revoke_delete_failed`.
- **Problem:** If the temp-write or rename of the consent file fails (disk full, EROFS, permissions), the in-memory flag is `off` but the file still says `on`. The error code, the response, and the order relative to queue deletion are all unspecified. On restart the host reads `on` and resumes LRS sends, including any queue that wasn't deleted. That breaks "consent-only xAPI." The same gap exists for a failed queue delete at startup when consent is `off`.
- **Fix:** Add `500 consent_write_failed`. Specify that the host refuses LRS sends for the life of the process. On the next start, if the consent file's state can't be confirmed as matching the last requested revoke (for example via a tombstone or a `revoke-pending` marker in `--data-dir`), the host starts with consent `off`. Specify the behavior when queue deletion fails at startup. Add an AT.

### F-08: Nothing prevents two host processes from sharing a working archive or `--data-dir` (Major)
- **Ref:** §1; DD-48; §5.2.1 Write mutex; §5.5 Inclusion index; AT-46; AT-73
- **Quote:** "The working-archive exclusive lock is held only for the tile-10 write (step 1)". "Each index mutation copies the current file …, appends the new line, `fsync`s, and publishes with an atomic rename".
- **Problem:** "One process serves one archive" is an assumption, not an enforced rule. Two hosts that point at the same `--data-dir` both do copy+append+rename, so one host's index line is silently lost. Idempotency breaks, duplicate tile-10 records follow, and `lrs-retry/` and consent writes race. AT-46 itself starts a second host process, and the spec doesn't say whether that host may reuse the data-dir.
- **Fix:** Require a startup lock: a `flock` on `--data-dir/.lock`, and optionally a long-lived shared or advisory lock on the working archive. A second process gets a startup error that names the flag. Add an AT.

### F-09: The rule for a mid-chain signature failure is internally inconsistent and can label tampered files "Verified" (Major, security)
- **Ref:** §5.4 "Per-tile outcomes"; DD-100; AT-35; AT-155
- **Quote:** "A **mid-chain signature failure** when the signer **is** trusted (pinned, or empty list yielding Self-signed) … The file-level **status** stays Verified or Self-signed".
- **Problem:** (1) An empty pin list is defined as *not* a trust anchor ("at best Self-signed"), yet this rule calls it "trusted". (2) A committed CRC failure, which can be an accident, makes the file Quarantined. A failed Ed25519 signature, which is evidence of tampering or a forgery attempt, leaves the file "Verified". The user-facing label is weakest exactly where the evidence is strongest.
- **Fix:** Rename the condition to "signer key known (pinned, or embedded when the pin list is empty)". Either make a failed signature Quarantined at file level (keeping the earlier valid floors), or add a distinct status or label such as "Verified (partial)". Update AT-35/155 to match.

### F-10: Ownership of verification is contradictory, and no test keeps pack and reader-core results consistent (Major)
- **Ref:** DD-19; §5.4 item 2; §5.1 `open` row; DD-80; AT-137
- **Quote:** DD-19: "Verification stays in `s3a-reader-core`." §5.4: "Verification is performed only by `s3a-engine`." §5.1: "The host and the phone app prefer `open_verified` when a report already exists." §5.7: "If the app calls `open` instead … kite runs the §5.4 algorithm."
- **Problem:** The pack implements the full §5.4 label algorithm for `open`/`open_path` (the host always takes this path, since it has no reader-core). The phone uses reader-core's algorithm. Two implementations of the trust labels exist, and AT-137 explicitly tolerates disagreement. Nothing requires the pack's `open` to agree with reader-core `verify` on the same vectors, so the hub and the phone can show different trust labels for the same file. The "host prefers `open_verified`" clause has no source for a report.
- **Fix:** Fix DD-19 (verification exists in both places, and the pack is normative for the host). Delete "host … prefers `open_verified`". Add a conformance AT: for MICRO-VEC-01/02/03, the AT-133/134/154/155 fixtures, and empty and non-empty pin lists, pack `open` and the mapped reader-core `verify` give identical `status`, `seal_status`, and `invalid_tile_indices`.

### F-11: `KiteVerificationReport` has no `unverified` value, but `open_verified` must reject it and AT-154 tests it (Major)
- **Ref:** §5.1; §5.4 label table; DD-99; AT-154
- **Quote:** "fields `status` (`verified` / `self_signed` / `unsigned` / `quarantined`) … Reader-core `Unverified` … is not a legal `status` on this type." "A report whose `status` is `unverified` is a `KiteError`."
- **Problem:** If the type can't represent `unverified`, the error path can't be reached and AT-154's assertion can't be written in Rust, Kotlin, or Swift. If the type can represent it, the type definition is wrong.
- **Fix:** Pick one. Either add `Unverified` to the enum, with `open_verified` rejecting it (AT-154 as written), or drop the rejection clause and AT-154's second sentence and rely on the type system, noting that the app's mapping function returns an error for reader-core `Unverified`.

### F-12: Building candidates from corpus `.s3a` files ignores consent filtering and trust labels (Major, privacy)
- **Ref:** §5.5 candidate list item 2; §5.3 item 4; §5.4 item 2
- **Quote:** "every live (non-tombstone) `AcademicPaperRecord` in every regular `*.s3a` file found by a recursive walk of `--corpus-root` … Files that fail to open are skipped."
- **Problem:** The viewer withholds records under research-consent rules and drops invalid or quarantined tiles. The candidate walk doesn't say it applies either rule. A withheld or withdrawn-consent `AcademicPaperRecord`, or one in a CRC-failed or seal-failed tile, would still be listed, could be selected, and its `paper_id` would be written to the index and sent to the LRS.
- **Fix:** State that candidate extraction uses the same §5.3 item 4 filter and valid-floor rules (withheld or invalid records are not candidates). Specify whether Quarantined corpus files contribute candidates. Add an AT.

### F-13: Local subresources in HTML and Markdown (relative `<img>`, `<link>`, `@font-face`) aren't specified (Major, security)
- **Ref:** §5.2 item 5; §5.5 first bullet; DD-77; AT-121; §5.2.2 Path rule
- **Quote:** "no subresource fetches (no CSS, images, or fonts **over the network**; DD-77)".
- **Problem:** Only network subresources are banned. Whether a page under `--view-root` may load `<img src="../../secret.png">`, `<link href="/etc/...">`, or `file://` stylesheets from disk isn't stated. The path rule (DD-89) covers request paths, `Link` follow-ups, candidates, and `drive_path`, but not subresources. This is a local-file disclosure surface, for example rendering another file's content into a frame.
- **Fix:** Normatively state either "no subresources of any kind (network or local) are loaded in v1" or "local subresources must canonicalize inside an allowed request root, else are dropped". Extend AT-121 with a relative-escape `<img>` and `<link>` case.

### F-14: The error catalogue is called normative but leaves out common failure paths (Major)
- **Ref:** §5.2.1 Error catalogue; DD-83
- **Quote:** "The status/code table in §5.2.1 is normative … ATs assert the listed codes."
- **Problem:** No code is defined for: `/v1/open` of a directory, a FIFO or device, or an unreadable (EACCES) file under an allowed root; a wrong HTTP method on a known path (`GET /v1/select`); a wrong or missing `Content-Type`; a `KiteError` during rendering (for example missing fonts, which AT-42 makes reachable, or an internal render failure); `/v1/hit-test` with an unknown `view`; non-integer or negative `floor`, `w`, or `t` query values (as opposed to absent ones); or JSON that is valid but has the wrong field types. Implementers will invent codes, and ATs can't assert them.
- **Fix:** Add rows: `400 invalid_param` (type or range), `405 method_not_allowed`, `415 unsupported_media_type`, `409 not_regular_file` / `403 unreadable`, and `500 render_failed` (with `KiteError.code`). Also state the catch-all `500 internal` body.

### F-15: The numbered write steps have no branch for an idempotent repeat, and DD-34 leaves out `file_uuid` (Major)
- **Ref:** §5.5 Numbered write steps; Idempotency; DD-34; DD-61; AT-60; AT-83; AT-89
- **Quote:** Step 1: "Write the inclusion record (tile 10) and append the inclusion-index line". Idempotency: "Selecting the same paper twice … writes one record". DD-34: "Idempotent per identity string + verb".
- **Problem:** The step list says every select writes tile 10. The idempotency rule says a repeat doesn't. Which steps a repeat runs isn't specified: step 1 is skipped, but does step 2 retry when `snapshot_sha256` is null? Does step 3 emit a second LRS statement? Does download re-deliver? The retry behavior for LRS on a repeat is especially unclear, because a repeat could re-send or duplicate xAPI. DD-34's key also differs from §5.5's (`file_uuid` + identity + verb).
- **Fix:** Add step 0: "Look up the last matching index line. If one is present, skip step 1, run step 2 only if `snapshot_sha256` is null and a fetch is required, run 2b for download, and do not emit a new LRS statement (or emit with the same id)." Align DD-34's key with DD-61.

### F-16: The fail-first sequencing breaks the workspace build, and the "listed later" edit exceptions don't exist (Major)
- **Ref:** §7 preamble; §11 step 1; §5.1 Test locations
- **Quote:** "Implementation must make these pass **without editing the tests** except as listed later with reasons." §11.1: "Land the fail-first acceptance tests in s3a-storage so they fail for the documented reason".
- **Problem:** (1) No list of allowed test edits appears anywhere. (2) Tests that `use s3a_pack_kite::…` or spawn `s3a-kite host` can't compile before the crate exists. Landing them in the s3a-storage workspace breaks `cargo test` for every crate (a compile error, not a "documented" failure), and step 1 doesn't say how they are fenced: a separate crate, `#[ignore]`, a feature gate, or a stub crate (which the no-stubs rule forbids).
- **Fix:** Specify the mechanism, for example a `kite-acceptance` test crate excluded from default members, or tests that shell out and fail at runtime. Either add the promised exception list or delete the clause.

---

## Minor

### F-17: Runtime consent requests are called "unreachable" while being reachable and tested; AT-116 is vacuous (Minor)
- **Ref:** §5.2.3; §5.2.1 consent row; AT-116; DD-12
- **Quote:** "`POST /v1/consent` `{ "state": "on" }` without `--lrs-url` is unreachable if the flag pairing is enforced at startup; if reached it returns `409`". AT-116: "leaves the consent file not `on`".
- **Problem:** Any client can send that POST, so it is reachable. Without `--lrs-url` a consent file is forbidden, so "leaves the consent file not on" asserts on a file that can't exist and can't fail. A POST of `off` without a consent file returns `lrs_not_configured`, which is an odd code for revoking.
- **Fix:** Say "reachable at runtime; returns 409". Make AT-116 assert the 409 body and that no file is created at any path. Consider making `off` without LRS a `200` no-op.

### F-18: "Request paths are opened only under `--view-root`" contradicts the Scholar path rule (Minor)
- **Ref:** §5.2.2 (after the flag table); DD-89
- **Quote:** "Request paths are opened only under `--view-root`."
- **Problem:** In Scholar mode, the allowed roots are wiki, corpus, view, snapshot, and download directories, plus the working archive. `--view-root` is optional there.
- **Fix:** Prefix the sentence with "In viewer-only mode,".

### F-19: Some ATs restate the auth and flag rules incorrectly (Minor)
- **Ref:** AT-75; AT-76
- **Quote:** AT-75: "still requires a token except loopback `/health`". AT-76: "`--bind 127.0.0.1` (or `--headless`)".
- **Problem:** AT-75 leaves out the Unix-socket case and the `--behind-proxy` exception. AT-76 presents `--headless` as an alternative to `--bind`, but `--bind` is always required.
- **Fix:** Reference the "tokenless health case" term in AT-75. In AT-76, write "`--bind 127.0.0.1` (`--headless` optional on loopback)".

### F-20: Coordinate spaces for `SemanticNode` and hit-testing at DPR ≠ 1 are underspecified (Minor)
- **Ref:** §5.8 RenderFrame; DD-90; §5.1 `hit_test`
- **Quote:** `TextRun` "# pixmap coordinates"; `SemanticNode { … x: i32, y: i32 … }` (no unit); `hit_test` x/y "are CSS pixels".
- **Problem:** The unit of `SemanticNode` geometry isn't stated. Clients mixing CSS-px hit tests with pixmap-px semantics at DPR 2 will be off by 2×, and accessibility focus rectangles will be wrong.
- **Fix:** State the unit of `SemanticNode` geometry explicitly, and add a DPR-2 hit-test AT.

### F-21: Hyper-tile CRC-failure floor validity, `seal_status`, and consent records inside invalid tiles are undefined (Minor)
- **Ref:** §5.3 items 4 and 6; §5.4 label table row "Committed tile CRC failure | … | (as otherwise computed)"; AT-35
- **Problem:** For Edge, the CRC-failed tile and all later tiles are invalid. For hyper-tile, §5.3 doesn't say whether the later tiles stay valid, yet AT-35 asserts on hyper-tile too. "(as otherwise computed)" leaves `seal_status` undefined. It also isn't stated whether `ConsentRecord`s or marks inside a CRC-failed or invalid tile take part in §5.3 item 4. Edge item 6 says "CRC-valid type-71".
- **Fix:** State the hyper-tile rule (only the failed tile, or that tile and all later ones). Define `seal_status` for the row. Say that consent and mark records are read only from valid tiles, and state whether a withdrawal inside an invalid tile still withholds, which is the privacy-safe choice.

### F-22: The font-injection mechanism isn't in the API, and AT-42's host variant is ambiguous (Minor)
- **Ref:** §5.7 Fonts; DD-94; DD-101; AT-42; §5.1 API table
- **Problem:** `open`/`render_*` take no font set. `register_ua_fonts` sounds like global state. How a test passes "an empty set" to "the pack linked into the host binary" without a test feature, given the binary always embeds the fonts, isn't defined, and global registration conflicts with parallel tests.
- **Fix:** Add a font-set parameter or a `KiteContext` object to the API table (passed to `open*`), and define AT-42 as a library-level test of the same crate configuration the host links.

### F-23: Download delivery isn't atomic; snapshot temp files have no cleanup or visibility rule (Minor)
- **Ref:** §5.6 Download names; §5.2.1 Write mutex
- **Problem:** A crash during a download copy leaves a partial `{sha}` file. Every later download then fails with `409 download_conflict` and can't recover. Snapshot temp files in `--snapshot-dir` (an allowed open root) have no naming, startup-cleanup, or "not openable" rule.
- **Fix:** Deliver downloads via temp+fsync+rename. Name temp files with a fixed prefix, exclude them from `/v1/open`, and delete stale ones at startup.

### F-24: Withheld record content isn't explicitly excluded from `text_runs`, `semantics`, and hits (Minor, privacy)
- **Ref:** §5.3 item 4; §5.4 item 6; §5.8
- **Problem:** The spec defines `withheld_count` but never says that withheld payloads must not appear in the text layer, the semantics tree, the rasterized pixels, or hit results. Accessibility output is a likely place for a leak.
- **Fix:** Add a normative sentence and extend AT-11/AT-23 to search `text_runs` and `semantics` for a withheld record's unique string.

### F-25: Partial-success responses are ambiguous (Minor)
- **Ref:** §5.2.1 `/v1/download`; §5.6 Download names; §5.5 Retry queue
- **Problem:** `409 download_conflict` is returned after the inclusion record was kept, but the error body can't tell the client that inclusion succeeded. When a download's fetch fails, the value of `download_error` isn't stated. The required "visible error after a failed [LRS] attempt" has no field in any response.
- **Fix:** Return 200 with `download_error: "download_conflict"`, or add `included: true` to the error body. Define `download_error: "fetch_failed"`. Add an `lrs_status` field (or a status endpoint) for LRS failures.

### F-26: There is no input-size or record-count cap (Minor, robustness)
- **Ref:** §5.3 item 7; §5.2 open path; §5.7 SAF
- **Quote:** "There is no extra size cap beyond the engine and the OS".
- **Problem:** `open_path` and phone `open` read whole files into memory. A multi-GB corpus file or a huge `tile_count` warehouse can exhaust RSS on the phone or the hub. The candidate walk opens every corpus `.s3a` on every request. A fuzz pass only proves no crash, not bounded memory.
- **Fix:** Add a required `--max-open-bytes` (host) and `max_open_bytes` (FFI), or record the risk explicitly in §10 or a Q.

### F-27: The precedence of multiple startup errors is unspecified, and Scholar-mode activation is implicit (Minor)
- **Ref:** §5.2.2; AT-51; AT-125; Glossary "Scholar mode"
- **Problem:** AT-125 ("without `--headless` when `--bind` is `0.0.0.0` … names `--headless`") doesn't say whether TLS flags are present, so the host could legitimately name the TLS flags first. "The Scholar flag set" isn't a defined switch: Scholar mode is just "not `--viewer`".
- **Fix:** State that Scholar mode is the absence of `--viewer`, and either fix the validation order or require ATs to supply every other flag valid.

### F-28: Some ATs are untestable or don't exercise the rule they cite (Minor)
- **Ref:** AT-57; AT-156; AT-145
- **Problem:** AT-57: "a request that presents a second token file" can't be done, since an HTTP request doesn't carry files. AT-156 claims to show "in-flight abort", but its scenario revokes during the *source fetch*, before any LRS POST exists, so only the pre-send check is exercised. AT-145 leaves out `--view-root` from the disjointness set that DD-93 requires.
- **Fix:** Reword AT-57 (for example "a Bearer value read from a different token file"). Add an AT-156 case with a slow LRS fixture and a revoke during the POST. Add `--view-root` cases to AT-145.

### F-29: The create-time research `ConsentRecord` grants consent nobody gave, and it changes the filter mode (Minor)
- **Ref:** §5.6 (create-time ConsentRecord); DD-28; DD-105; preamble "explicit contract exceptions"
- **Problem:** The host writes a research-purpose grant attributed to the researcher (`granted_by`) without any human consent action. That sits uneasily with "humans make the judgments" and with consent being opt-in. The spec's own rationale says it isn't needed for visibility (DD-67). Its presence also switches the archive from "empty map, unrestricted" to "map non-empty", so any later payload with *another* `subject_id` becomes withheld. It is effectively an invented default that isn't in the preamble's exception list.
- **Fix:** Add it to the exception list with its rationale, or make it conditional on an explicit researcher action. Document the filter-mode side effect, and add an AT that covers a foreign-subject record in the working archive.

### F-30: Hit-test on a view that isn't valid for the handle's state has no defined code (Minor)
- **Ref:** §5.1 Hit rules; §5.2.6 matrix; error catalogue
- **Problem:** The matrix covers `/v1/frame` only. For `/v1/hit-test` with `view=warehouse` before proceed, or `view=page` on an archive, it's unclear whether the result is `no_frame`, `view_mismatch`, or `not_archive`. The last frame for `floor-transition` is keyed by view only, not by floor.
- **Fix:** State that the §5.2.6 matrix applies to `/v1/hit-test`, checked before `no_frame`, and that the last-frame cache is keyed by (view, floor).

### F-31: The tokenless-health and Unix-socket rules don't address filesystem permissions on the socket's parent directory (Minor)
- **Ref:** §5.2.2 Unix-socket bind; DD-108
- **Problem:** The socket is mode 0600, but the spec doesn't require the parent directory to be non-world-writable. On a shared host, if the parent directory is world-writable, another user can replace or race the path before bind. The existence check and the bind aren't atomic.
- **Fix:** Require the parent directory to be owned by euid and not group/world-writable (a startup error otherwise), or document the assumption.
