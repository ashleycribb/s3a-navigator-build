# Goldfish review — s3a-kite spec v4 (round 4)

*Reviewer:* independent spec reviewer with no prior context. Source read: `/workspace/goldfish/s3a-kite.md` only.
*Date:* 2026-11-16 (America/New_York)
*Ground rules applied:* DD-xx draft values, Q-xx open questions, deliberate scope exclusions, and listed external facts are not findings unless the spec contradicts itself about them. Nothing outside the file was checked.

## Summary

| Severity | Count |
| --- | --- |
| Blocker | 1 |
| Major | 17 |
| Minor | 22 |
| **Total** | **40** |

Top themes: (1) the privacy rule for `--data-dir` (AT-61) contradicts the inclusion index design; (2) state needed for idempotency is missing: no identity-to-snapshot map, no working-archive key in the index, and no rule for concurrent writes; (3) the HTTP contract has gaps: no error-status catalogue, no `banner` view, no rule for detecting `kind`, and no bounds on frame size; (4) security trust anchors are missing: the Ed25519 public key source and protection of the LRS credentials; (5) some ATs cannot pass as written (AT-76) or cannot fail (AT-107).

---

## Findings

### F-01 — Blocker — AT-61 vs Inclusion index (§5.5, DD-61, AT-103)
**Quote:** AT-61: "After a consent-off select, `--log-dir` and `--data-dir` contain no paper ids or source URLs." §5.5: "`--data-dir/inclusion-index.jsonl` is append-only. Each successful inclusion appends one JSON object: `{"paper_identity":"<id>",…}`." AT-103: "…and the index has the identity."
**Problem:** Every successful select writes the paper identity to `--data-dir`, whatever the consent state. So AT-61 and AT-103/AT-60/AT-83 cannot all pass. The idempotency design depends on the very thing AT-61 forbids.
**Fix:** Pick one. (a) Narrow AT-61 and the §5.5 privacy bullet to "no paper ids or URLs in `--log-dir`, and none in `--data-dir` other than `inclusion-index.jsonl`". State that the index is local research data, just like the working archive. Or (b) move the index into the working archive or a sidecar next to it (outside `--data-dir`), and update DD-15/DD-61.

### F-02 — Major — No identity→snapshot mapping (§5.5 Source model, DD-59, AT-60, AT-83, AT-102, `/v1/select` response)
**Quote:** "Fetch is triggered only by … a current candidate that has a `source_url` and does not already have a successful snapshot for that identity. … if a snapshot already exists for the identity, download does not fetch again."
**Problem:** Snapshots are named by the SHA-256 of the captured bytes (§5.6). The index line stores `paper_identity, verb_id, object_hash, timestamp_sec`, with no snapshot hash. Before fetching, the host cannot tell whether a snapshot "exists for the identity". Several things are also unspecified: what `snapshot` path an idempotent second select returns, and where the bytes for a later `download` come from when no new fetch is allowed (reassembling the BLOB tiles from the snapshot?).
**Fix:** Add `snapshot_sha256` (nullable) to the index. Alternatively, define a separate append-only `snapshot-index.jsonl` for identity → sha256. Specify that download-after-select reassembles bytes from the snapshot BLOBs. Specify the response fields for idempotent calls.

### F-03 — Major — The index idempotency key names the working archive, but the index never records it (§5.5 Inclusion index, DD-61)
**Quote:** "The idempotency key is **working archive (this process's) + paper identity string + verb id**, looked up in that index."
**Problem:** The index line has no archive path or archive UUID. Suppose `--data-dir` is reused with a different `--working-archive`, or the archive is deleted and recreated. A matching index line then suppresses the write, and a real inclusion is silently dropped. Risk 10 calls that outcome worse than a duplicate.
**Fix:** Store the canonical working-archive path and/or `file_uuid` in each index line, and include it in the lookup. Specify what happens when the index refers to an archive that no longer exists (treat the entry as non-matching).

### F-04 — Major — No rule for concurrent writes (§5.2.1 Handles, §5.5, §5.6)
**Quote:** "`/v1/frame` and `/v1/hit-test` on one handle are serialized by a per-handle lock."
**Problem:** Select, download, and consent requests are not tied to a handle. Two overlapping selects of the same paper (a double-click, or the window plus a script) can both miss the index, both write tile 10, and both fetch. Two identities with identical bytes can race on the same `{sha}.s3a` snapshot path. A consent revoke can race an in-flight LRS send. The spec never says whether the archive lock is held across steps 1 to 3.
**Fix:** Require one process-wide write mutex that serializes select, download, and consent end to end, or at least step 1 plus the index check. Require snapshot writes to use temp file plus atomic rename. Add an AT with N concurrent selects of one identity that expects one record.

### F-05 — Major — The download-file ordering contradicts the side-effect order (§5.5, DD-56, DD-34, AT-89)
**Quote:** "(1) inclusion record and index, (2) fetch + snapshot if applicable, (3) LRS … If step 1 fails, steps 2 and 3 are skipped." Also in the same bullet: "If the inclusion write fails, a delivered download file is kept and the error is shown (DD-56)."
**Problem:** If step 1 runs first and skips everything after it on failure, no download file can have been delivered when the inclusion write fails. Either the DD-56 sentence is unreachable, or the download copy happens before step 1, which contradicts the order. The step that delivers the download file (the on-disk copy or the remote write into `--download-dir`) is never numbered.
**Fix:** Name the download delivery as an explicit step (for example 2b, after the snapshot). Then delete the DD-56 sentence, or rewrite it as "if a *later* step fails, a delivered download file is kept". Extend AT-89 to assert that `--download-dir` is unchanged when step 1 fails.

### F-06 — Major — Select/download behavior for candidates with no bytes source; `on_disk` and `drive_path` are underspecified (§5.5, DD-59, `/v1/download`)
**Quote:** "`on_disk` (true when a usable local file exists; see below)" and "A candidate's on-disk bytes, when present, come from a `drive_path` that canonicalizes inside `--corpus-root` or `--wiki-root`."
**Problem:** (a) `AcademicPaperRecord` candidates have neither `source_url` nor `drive_path`, so it is unclear what `/v1/download` returns: 200 with `path: null`? An error? Does the inclusion still get written? (b) The spec does not say what a relative `drive_path` resolves against (the dossier's directory? the root?). (c) The spec does not say whether the dossier `.md` itself counts as an "on-disk" source. (d) The spec does not say what happens when a candidate has both `source_url` and an on-disk `drive_path`: fetch, copy, or both?
**Fix:** Add a decision table over the inputs {source_url?, on-disk?}. For each case, give the select and download outcomes: inclusion written, snapshot, download source, response fields, and error code. Define how `drive_path` is resolved. Add ATs for the "neither" case and the "both" case.

### F-07 — Major — "Current candidate list" is undefined (§5.5, `/v1/select`, AT-105)
**Quote:** "`paper_id` on select, download, and skip **must be an `id` in the current candidate list**."
**Problem:** The spec does not say whether "current" means the last list served by `GET /v1/candidates`, or a list recomputed when select is called. If no GET has been made yet, it is unclear whether select is 404. The spec also does not say what happens if files change between the GET and the select. Each reading makes AT-105 and AT-24 behave differently, and recomputing means a full recursive walk of `.s3a` files on every write.
**Fix:** State "the candidate set is recomputed at request time (with no persisted state)" or "the last list served, held in memory". Give the expected 404 behavior for the no-prior-GET case.

### F-08 — Major — No rule for detecting `kind` and `first_view`; `kind: "other"` is undefined (§5.1 `open(bytes)`, §5.2.1 `/v1/open`, DD-14, DD-62)
**Quote:** `"kind": "hyper-tile"|"edge"|"wiki"|"html"|"other"|"unsupported"` and "`open(bytes)` … Open an archive or document already loaded as bytes."
**Problem:** Several detection rules are missing. `open(bytes)` takes no filename or MIME, so it cannot tell Markdown from HTML from plain text. The spec does not say whether `open_path` uses extensions or content sniffing. It does not say what `first_view` an `"other"` file (for example a PDF, per DD-14) gets, or what renders it. Download files have no extension (DD-62) but sit in an allowed open root, so reopening them depends on sniffing. A file with bad magic is an error under §5.3, yet `"other"` also exists. The boundary between the two is undefined.
**Fix:** Add a detection table: S3A magic → hyper-tile, edge, or unsupported; `.md` → wiki; `.html`/`.htm` → html; anything else → `other`, with a defined `first_view` and frame, or a defined error. Add an optional name or MIME hint parameter to `open(bytes)`. Add an AT for each row.

### F-09 — Major — `first_view: "banner"` cannot be rendered (§5.2.1 `/v1/frame`, §5.3 item 1, §5.4 item 1, AT-66, AT-97)
**Quote:** `/v1/frame` `view` (`manifest`|`header`|`page`|`warehouse`|`floor-transition`|`floor-detail`) and "returns 200 with `kind: "unsupported"`, `first_view: "banner"`."
**Problem:** `banner` is not a valid `view`, and §5.1 lists no `render_banner`. The client is told to show a view that it cannot request. AT-100 says an unknown view is 400 or 404, so `view=banner` would fail.
**Fix:** Either add `banner` to the view enum and map it to `render_manifest` (and say so in §5.1), or have unsupported files return `first_view: "manifest"`/`"header"` with the banner painted by `render_manifest`. Extend AT-97 and AT-66 to fetch the frame and assert the banner text.

### F-10 — Major — No HTTP error-status catalogue (§5.2.1, AT-30, AT-46, AT-52, AT-89, AT-94, AT-124, `proceed` on a page)
**Quote:** "Errors are `{ "error": "<code>", "message": "<text>" }` with a 4xx or 5xx status."
**Problem:** Many failures have no status or code: a path outside the roots (AT-52/53), a missing file (AT-30), invalid magic or an unsupported version, `not_archive` from `proceed` on a page handle, an unknown handle on `/v1/frame`, a view that doesn't match the handle (for example `warehouse` before proceed, or `page` on an archive), a missing `w`/`h`/`floor`/`t`, malformed JSON, a locked or failing archive (AT-89), a missing `ADDON_EDU` (AT-46), a download name conflict (AT-94), and a failed revoke delete (AT-124). Tests can only assert "some error", and clients cannot branch on the result.
**Fix:** Add an error table that gives each `error` code, its HTTP status, and the route(s) that return it. Have the ATs above assert the exact code.

### F-11 — Major — AT-76 cannot pass as written (AT-76, §5.2.2, DD-74)
**Quote:** "Viewer-only mode (`--viewer`, `--bind`, `--port`, `--auth-token-file`, `--view-root`, `--log-dir`) starts."
**Problem:** §5.2.2 makes `--max-open-handles` required in viewer mode. It also requires either `--headless` or a loopback/Unix bind, and AT-76 gives no bind value. Started with exactly the listed flags, the host must exit with a startup error.
**Fix:** Add `--max-open-handles <n>` and `--bind 127.0.0.1` (or `--headless`) to AT-76. Add a companion negative AT for viewer mode without `--max-open-handles`.

### F-12 — Major — The hyper-tile consent-filtering rule is incomplete (§4.2, §5.3 item 4, DD-53, DD-67, AT-11)
**Quote:** §5.3: "If the file contains **no** core `ConsentRecord`s, the view is unrestricted … If at least one research-purpose `ConsentRecord` exists, records not granted for that purpose are withheld." §4.2: "When at least one applicable consent record exists…"
**Problem:** The spec does not cover a file whose `ConsentRecord`s are all non-research or revoked. Nor does it define "granted for that purpose" per record: matching on subject, scope, or time window, and how withdrawal marks apply. "Applicable" is undefined. AT-11's oracle is "hand-verified", but no stated rule exists to verify against, so the test checks whatever the fixture author assumed.
**Fix:** Write the algorithm out: what counts as a consent record, how a data record is matched to a consent record (scope/subject/time), how marks and revocation apply, and the result when only non-research records exist. Add ATs for "only a non-research ConsentRecord" and "a revoked research ConsentRecord".

### F-13 — Major (security) — The Ed25519 trust anchor is unspecified (§5.4 item 2, DD-57, AT-35)
**Quote:** "Each record's `signature` is verified with `verify_signature_raw` against the device public key over that record's 32-byte `seal_hash`."
**Problem:** The spec never says where "the device public key" comes from. If it comes from the file itself (for example the `DeviceIdentityRecord`), anyone who rewrites the file can re-sign it, and "signed" means nothing. The spec also does not say what happens when `FLAG_ED25519_SIGNED` is set but no type-70 tile or key exists, or when a key exists but the flag is clear.
**Fix:** State the key source (a pinned key store, a provisioning file, or "self-asserted; displayed as *self-signed*, not *verified*"). Specify the UI label for each case. Add ATs for a file re-signed with a different key and for a flag-set file with no signatures.

### F-14 — Major — Mobile `VerificationReport` hand-off is missing from the FFI (§5.4 item 2, §5.7, DD-19, §5.1)
**Quote:** "On mobile, `s3a-reader-core` runs first … kite consumes the `VerificationReport` and does not compute a second result."
**Problem:** The FFI signature is `open(bytes) -> Result<Handle, KiteError>`, with no parameter for the report. The spec does not say whether kite calls `s3a-reader-core` itself (adding a dependency the spec never lists), or whether Kotlin passes the report through. The no-second-computation rule therefore cannot be implemented or tested.
**Fix:** Add `open_verified(bytes, report)` (or an optional report argument) to §5.1 and to the UniFFI surface (AT-99). Add an AT showing that kite's banner and floor validity match a supplied report, including a report that disagrees with a fresh verification.

### F-15 — Major — `floor_index` / `Floor(n)` semantics are ambiguous (§5.1, §5.3, §5.4 item 2, AT-10, AT-17)
**Quote:** "`render_floor_detail(handle, floor_index, …)` | Floor N records" and "Invalid tiles … are omitted from the valid floor list."
**Problem:** Invalid tiles are omitted from the valid floor list, so a floor's ordinal position differs from its tile `index`. The spec does not say whether `floor_index`, `Hit::Floor(n)`, and `/v1/frame?floor=` mean the tile index or the ordinal in the displayed list. It also does not cover a request for an invalid tile's index, or the "clearly marked invalid list".
**Fix:** Define `floor_index` as the tile `index` (matching AT-10's `index` field). Give an error code for indices that are not valid floors. State whether invalid-list entries are hit-testable.

### F-16 — Major — Fetch response handling is unspecified (§5.2.4, §5.5, DD-76)
**Quote:** "the host itself performs the HTTP request, applies the egress policy in §5.2.4, and stores the raw response body."
**Problem:** The spec does not say what happens with a non-2xx response (a 404 or 500 HTML error page would be snapshotted and downloaded as "the paper", with idempotency then blocking a retry). It also does not say how many redirects are allowed, whether `Content-Encoding` is decoded before hashing and before the `--max-fetch-bytes` check, or whether "raw" means the bytes on the wire or decoded bytes. Each choice changes the SHA-256, the snapshot name, and the download name.
**Fix:** Require 2xx (otherwise `fetch_error` and no snapshot), a maximum redirect count, and a precise definition of the body (decoded or identity encoding) with the byte limit applied to it. Add ATs for a 404 body and a gzip body.

### F-17 — Major — No bounds on frame size (§5.2.1 `/v1/frame`, §5.8, DD-69)
**Quote:** "`w` and `h` have no default and are required."
**Problem:** No maximum is set for `w`, `h`, `device_pixel_ratio`, or scroll. A single authenticated request like `w=100000&h=100000` asks for about 40 GB of RGBA plus a PNG encode. The per-handle lock does not stop requests on several handles. On the phone this risks an out-of-memory kill. Negative or NaN DPR values are also undefined.
**Fix:** Add a required `--max-frame-pixels` flag (or a contract constant, recorded as a DD) and validation for DPR and scroll, with a 400 error code. Add an AT.

### F-18 — Major (security/privacy) — Other secrets and personal files lack the token file's protections (§5.2.2, §5.2.3, §5.5 LRS client and Retry queue, DD-73)
**Quote:** "On Unix, the token file must be owned by the process's effective user and have mode `0600`… The host never writes token bytes or `Authorization` header values…"
**Problem:** The ownership, mode, and no-logging rules cover only the UI token. `--lrs-credentials-file` holds a Basic password and has no mode check. Basic `Authorization` headers to the LRS are outgoing, so it is unclear whether the no-logging rule covers them. `--tls-key`, the consent file, the retry-queue files (which hold the researcher id and paper ids), and the inclusion index have no specified modes. The spec also never says how LRS TLS certificates are validated (system roots? pinning?).
**Fix:** Apply the owner and `0600` rule to `--lrs-credentials-file` and `--tls-key`. Require `0600` for files the host creates in `--data-dir`. Extend the no-logging rule to outgoing credentials. State the TLS trust store. Extend AT-117 to cover these.

### F-19 — Minor — AT-27 puts the snapshot in the working archive (AT-27, §5.6, DD-27)
**Quote:** "the working archive still contains the inclusion record and a snapshot of the captured bytes."
**Problem:** Snapshots are separate files in `--snapshot-dir`. They are not part of the working archive.
**Fix:** Change it to "…the working archive contains the inclusion record and `--snapshot-dir` contains `{sha}.s3a`."

### F-20 — Minor — AT-07 and AT-77 require `--port` in every mode (AT-07, AT-77, DD-72, AT-113)
**Quote:** "Omitting `--port` or `--bind` in any mode exits non-zero."
**Problem:** This contradicts the Unix-socket rule, under which `--port` is forbidden. A literal implementation would also reject `unix:` binds.
**Fix:** Change it to "Omitting `--bind`, or omitting `--port` with a non-`unix:` bind, …".

### F-21 — Minor — The tokenless-health wording is inconsistent (§1, §5.2.1 intro, AT-06, AT-75 vs the §5.2.1 table, AT-113)
**Quote:** AT-06: "401 on every route … other than loopback `GET /health`." The table says: "None on a loopback or Unix-socket bind."
**Problem:** Several sentences omit the Unix-socket exemption. A strict reading of AT-06 conflicts with AT-113.
**Fix:** Use one defined term ("tokenless health case: loopback or Unix bind without `--behind-proxy`") everywhere.

### F-22 — Minor — The DD-63 snapshot exemption has no observable effect; AT-107 cannot fail on its own terms (DD-63, DD-67, AT-107, AT-108)
**Quote:** "(3) the only record tiles are BLOB (68) and PROVENANCE (72)."
**Problem:** A file that meets criterion 3 has no `ConsentRecord`, so DD-67 already makes it unrestricted. AT-107 passes whether or not the exemption exists. AT-108 only exercises the ordinary filter.
**Fix:** Delete DD-63 and rely on DD-67, or state the case the exemption actually changes. Rewrite AT-107 so that it fails without the exemption.

### F-23 — Minor — AT-125 can pass for the wrong reason, and needs a display in CI (AT-125, AT-51, §5.2)
**Quote:** "Starting without `--headless` when `--bind` is `0.0.0.0` is a startup error."
**Problem:** Without TLS flags, the same command already fails under AT-51. The test also says "native-window build", but elsewhere the window is the same binary controlled by `--headless`. A host-only CI with no `DISPLAY` must fail startup (§5.2), so the window half of the test has no defined environment.
**Fix:** Pass `--tls-cert/--tls-key` in the negative case and assert the error names `--headless`. Say "same binary, without `--headless`". Specify a virtual display, or a skip-and-report rule like the one used for mobile.

### F-24 — Minor — Weak egress ATs (AT-122, AT-54)
**Quote:** AT-122: "With `HTTP_PROXY` … set …, a denied destination is still not sent through the proxy." AT-54: "the configured LRS private address is blocked."
**Problem:** AT-122 still passes if the host honors the proxy for *allowed* destinations, which §5.2.4 forbids. The AT-54 LRS case is already denied as RFC1918, so it never tests the "LRS host" rule.
**Fix:** Have AT-122 assert that an allowed `egress_test` fixture is reached directly with zero proxy hits. Have AT-54 use an LRS hostname that resolves to an otherwise-allowed address, so that only the LRS rule can deny it.

### F-25 — Minor — AT-71 is not a fuzz criterion (AT-71, §5.9)
**Quote:** "The `open_and_render` cargo-fuzz target exists and, on the AT-08 fixture, MICRO-VEC-01/02/03, and a malformed file, does not abort."
**Problem:** Four fixed inputs are a smoke test. No run budget, corpus, or CI duration is given, so "malformed input is not a crash" goes untested.
**Fix:** Specify a seed corpus and a minimum run (for example N iterations or T seconds in CI) with no crash or OOM, and a corpus-replay regression job.

### F-26 — Minor — AT-100 accepts "400 or 404" (AT-100, DD-71)
**Problem:** An either/or status is not a contract. 404 is also the "unknown path" signal, which blurs the two.
**Fix:** Pick 400 `invalid_view`.

### F-27 — Minor — AT-29 calls `/v1/open` a GET (AT-29)
**Quote:** "waits 15 seconds after only candidate/frame/open GETs."
**Problem:** `/v1/open` is a POST. The test also doesn't say whether `/v1/open` may create ephemeral temp files or handles (which are allowed). The wording invites an incorrect harness.
**Fix:** List the exact requests: `GET /v1/candidates`, `POST /v1/open`, and `GET /v1/frame`.

### F-28 — Minor — The path rule is broader than intended (§5.2.2)
**Quote:** "After start, a path the host opens must canonicalize to a location strictly inside an allowed directory, or be exactly the working-archive path."
**Problem:** Read literally, this forbids the host from opening `--data-dir/inclusion-index.jsonl`, `lrs-retry/`, the consent file, the token or credentials files, and `--log-dir`, none of which are "allowed directories".
**Fix:** Scope the rule to paths that come from requests (`/v1/open`, `Link` follow-ups, `drive_path`, and candidate files). List the host's own config and state files separately.

### F-29 — Minor — Coordinate spaces and DPR semantics are undefined (§5.8, §5.2.1, DD-69)
**Quote:** "height: u32, # pixmap pixels; at scale 1.0, 1 CSS px = 1 pixmap px."
**Problem:** At DPR ≠ 1, the spec does not say whether `w`/`h` and `x`/`y` are CSS px or device px, or whether the response `width` equals `w` or `w*dpr`. It also does not say whether `hit_test` coordinates are CSS or pixmap. `TextRun` and `SemanticNode` coordinates are `u32`, but layout boxes can sit partly off-pixmap (§5.8 animation, scrolled content).
**Fix:** Define `w`/`h`/`x`/`y` as CSS px and the pixmap as `ceil(w*dpr)`. Use `i32` for layout coordinates, or clip them. Add one AT at DPR 2.

### F-30 — Minor — `Select` hits where select cannot be called (§5.1 Hit, §5.7, viewer-only mode)
**Quote:** "`Select { paper_id }` is only defined on `view=page` when `paper_identity(handle)` is `Some`."
**Problem:** A dossier opened in viewer-only mode or on the phone still yields a Select control and hit, and the only follow-up is a 409 (host) or nothing at all (phone).
**Fix:** Pass a mode flag to `render_page` (or the handle) so the Select control is painted and hit-testable only where `/v1/select` is available. Add an AT.

### F-31 — Minor — `--log-dir` contents and the ephemeral temp location are undefined (§5.2.2, §5.5, DD-55, AT-24, AT-29, AT-117)
**Problem:** `--log-dir` is required, but the spec never says what goes in it. AT-24 and AT-29 imply that no request logs are written, even for GETs. The ephemeral temp directory has no location (`$TMPDIR`?), no mode, and no crash cleanup, and it may hold fetched paper bytes after a crash.
**Fix:** Specify the log events, their fields, and an explicit no-identifier rule. Specify the temp directory location (for example a `0700` mkdtemp under `$XDG_RUNTIME_DIR` or `/tmp`) and cleanup of stale directories at startup.

### F-32 — Minor — Consent and flag edge cases (§5.2.2, §5.2.3, DD-50)
**Problem:** Several flag and consent cases have no defined outcome: `POST /v1/consent {off}` when no `--consent-state-file` is configured (with nothing to write), `GET /v1/consent` without `--lrs-url` or a file, and Scholar-only flags such as `--lrs-url`, `--researcher-id`, or `--working-archive` passed in viewer mode (only `--data-dir` is explicitly rejected).
**Fix:** Define these responses. Reject all Scholar-only flags in viewer mode, as `--data-dir` already is.

### F-33 — Minor — The provenance document is not byte-exact (§5.6, DD-47, AT-32)
**Problem:** Line order, the trailing newline, and CRLF vs LF are unstated. The `blob_key` for PROVENANCE chunks (the hash of the provenance document or of the captured bytes?) is unstated. `url=` "or empty" can never occur, because snapshots are made only for `source_url` fetches.
**Fix:** Fix the order and LF endings with a final newline. State the provenance `blob_key`. Drop "or empty", or state when it applies.

### F-34 — Minor — Handles never expire (§5.2.1 Handles, DD-74)
**Problem:** If a client crashes without calling `/v1/close`, its handles stay open until restart. With a small `--max-open-handles`, the host returns 429 for good. Handle data is a full in-memory copy (DD-75), which also raises memory concerns.
**Fix:** Add an idle-timeout flag or a `close-all` route (recorded as a DD), or state that a restart is the documented recovery.

### F-35 — Minor — Candidate list edge cases (§5.5 Candidate list, DD-61)
**Quote:** "whose decimal `paper_id` is not already the same paper as an SRC candidate. v1 does **not** treat an SRC stem and a `paper_id` as the same paper."
**Problem:** The exclusion clause can never fire, so it reads as a hidden join. The same `paper_id` in several `.s3a` files, or repeated within one file, has no dedupe rule. It is unclear whether `papers/SRC-*.md` must be directly under the root or may be found recursively (the grounded layout is `wiki/papers/`).
**Fix:** Delete the dead clause. Dedupe by identity, choosing the first by sorted path. State the glob depth exactly.

### F-36 — Minor — Overlapping roots are not prohibited (§5.2.2, §5.6)
**Problem:** `--working-archive`, `--snapshot-dir`, or `--download-dir` could sit inside `--corpus-root` or `--wiki-root`. The corpus is then not read-only, which contradicts §5.6. The recursive `*.s3a` walk would also pick up snapshots of fetched `.s3a` bodies, so the candidate list would change as a side effect of select.
**Fix:** Require the write locations to be disjoint from the read roots, with a startup error otherwise. Add an AT.

### F-37 — Minor — Host font sourcing is unspecified (§5.7 Fonts, AT-42)
**Problem:** Font requirements are written only for the mobile app. The spec does not say where the Linux host gets UA font bytes (embedded, or a flag pointing at files?), or whether AT-42 applies to the host.
**Fix:** State the host font source (embedded via `include_bytes!`, or a required `--font-dir`) and extend AT-42 to the host.

### F-38 — Minor — The implementation order omits whole features (§11)
**Problem:** The steps cover hyper-tile paint (3), the host (4), writes and LRS (5), and mobile (6). Nothing is sequenced for Edge (§5.4: header sheet, event log, seal chain, Ed25519, type-71 consent), the fuzz target (AT-71), or the license inventory and About screen (AT-70). AT-14–AT-23 and AT-66–AT-68 therefore have no step to turn them green.
**Fix:** Add Edge as an explicit step (or extend step 3), and add fuzz and license packaging to the steps.

### F-39 — Minor — DD-14 promises PDF-skim behavior that no requirement or AT defines (DD-14, §5.5, Risk 6)
**Quote:** "Skim shows title, abstract, and any markdown conversion already in the archive. If that conversion is missing, show 'no text conversion in archive'."
**Problem:** No section says where the title, abstract, or "markdown conversion" live in an archive, which view renders them, or how a PDF candidate opens. No AT covers it.
**Fix:** Add the behavior to §5.5 with its source (tile type or record fields) and an AT. Otherwise, mark it as deferred.

### F-40 — Minor — Edge flag edge cases (§5.4 item 1, §4.3)
**Problem:** Detection is "`FLAG_PROFILE_EDGE` or stride 4096/8192". A file with stride 128 KiB, no `FLAG_PROFILE_EDGE`, but `FLAG_TILE_SIZE_4KB` set is not edge-detected, so it is toured as a hyper-tile despite contradictory flags. The spec also does not say whether `FLAG_SEAL_CHAIN_ACTIVE` clear means "unsealed Edge, show as such" or a quarantine.
**Fix:** Extend the inconsistency rule to any Edge size flag on a non-Edge stride. Define the display for an unsealed Edge file. Add ATs.
