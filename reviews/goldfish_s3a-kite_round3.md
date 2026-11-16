# Goldfish review: s3a-kite spec v3 (round 3)

Reviewer: independent, no prior context. Input: `/workspace/goldfish/s3a-kite.md` only (no other files, repos, prior reviews, or web).
Date: 2026-11-16 (America/New_York).

Rules applied: DD-xx draft values, Q-xx open questions, the deliberate exclusions in §3, and listed external facts are not findings unless the spec contradicts itself about them.

## Summary

| Severity | Count |
| --- | --- |
| Blocker | 2 |
| Major | 13 |
| Minor | 21 |
| **Total** | **36** |

Blockers: F-01 (wiki and HTML pages have no render API or frame view), F-02 (nothing defines how remote bytes are captured, or from which URL).

---

## Blockers

### F-01 — Blocker — §5.1, §5.2.1 `/v1/frame`, AT-30, AT-33, AT-56
**Quote:** `/v1/open` returns `"kind": … "wiki"|"html" …, "first_view": "manifest"|"header"|"page"`; `/v1/frame` `view` (`manifest`|`warehouse`|`floor-transition`|`floor-detail`); §5.1 lists only `render_manifest`, `render_warehouse`, `render_floor_transition`, `render_floor_detail`.
**Problem:** The first product use is a wiki browser, but the pack API has no `render_page`/`render_document` function and `/v1/frame` has no `page` view. `first_view: "page"` points at nothing. AT-30, AT-33 and AT-56 need a text layer for a wiki or HTML page and have no API to get one. `hit_test` can return `Select`, but no view where a Select control could appear is defined. The page's scroll, links and navigation are also undefined.
**Fix:** Add `render_page(handle, width, height, scroll_y) -> Result<RenderFrame, KiteError>` to §5.1, add `view=page` to `/v1/frame` and `/v1/hit-test`, and define which views `hit_test` may return `Select` for and which paper identity a `Select` hit refers to. Also define link-following behavior: allowed or not, and if allowed through which route.

### F-02 — Blocker — §5.2 item 4, §5.5, §5.6, DD-26, DD-44, AT-26/27/31/48/54/55/82/88
**Quote:** "For `http(s)` resource fetch (a snapshot or download of a remote source): the host itself performs the HTTP request…"; "writes a snapshot when bytes are captured"; `/v1/open` takes only `{ "path": "<path>" }`; `/v1/select` takes only `paper_id`.
**Problem:** No route accepts a URL, and the candidate list returns only `id` and `title`, with no URL. The spec never says what triggers a fetch, which URL is fetched for an `SRC-NNNN` stem or an `AcademicPaperRecord`, or whether select fetches and download fetches again. It doesn't say what "captured bytes" are for an on-disk paper (§5.6 says on-disk sources are not snapshotted) or what happens to the inclusion record when the fetch fails or times out. As a result, every egress test (AT-54, AT-55, AT-88) and every snapshot test (AT-31, AT-82) has no defined way to cause a fetch. Neither does any HTTP page view (DD-26).
**Fix:** Define the source model. That means (a) where a paper's source URL comes from (a dossier front-matter field, or an `AcademicPaperRecord` field), (b) which call fetches it (select, download, or a new `POST /v1/open` with `url`), (c) the order of fetch versus the inclusion record when the fetch fails or times out, and (d) what download delivers for on-disk-only papers. Then point each egress and snapshot AT at that route.

---

## Major

### F-03 — Major — §5.3 item 2, §5.4 items 3–5, §5.5 bullets 1 and 5, §5.7, §5.8, §5.2.2 "As in v2"
**Quote:** "Tile-78 rules are unchanged from v2"; "derived header sheet as in v2"; "category labels, and DD-07 colors as in v2. Unknown layouts as in v2"; "Animation endpoints and bezier are unchanged from v2 (AT-62)"; "`RenderFrame` is unchanged from v2".
**Problem:** v2 is not part of this document, yet normative content lives only there. AT-62 asserts the "§5.8 start, bezier mid, and end values", but §5.8 doesn't contain them. AT-19 asserts the "category label from §5.4", but §5.4 only says "as in v2". §4.3 claims "§5.4 states the override and adds a non-color category cue", which it does not. The derived header sheet's field list, the `RenderFrame` field types, the `text_runs`/`semantics` schemas and the LRS fail-closed rules are all missing. A tests-first contract can't be built from this text alone.
**Fix:** Inline every "as in v2" / "unchanged from v2" rule into v3, or make v2 a normative appendix. At minimum include the tile-78 rules, the header-sheet fields, the category-label table, the animation endpoints in frame-pixel terms, the `RenderFrame`/`text_runs`/`semantics` schema and the LRS fail-closed behavior.

### F-04 — Major — §1, §5.2, DD-32, DD-42, §5.1
**Quote:** "The human UI is a native window in this process that calls the pack API directly (DD-42)"; "Only a UI-token `POST /v1/select` or `POST /v1/download` creates an inclusion record (DD-32)… custody of that token file is the human control"; the §5.1 pack API has no select, download, skip or consent function.
**Problem:** This has two contradictions. (1) The native window can't select or download through the pack API, because those functions aren't in it. (2) If the window does write, it does so in-process without the token, so token custody is not the human-control point: anyone at the desktop session can write. The spec also requires a native window on an Aura Rack or LRS VM, which may be headless, and requires an HTTP listener even when only the window is used.
**Fix:** Either have the native window use the Host HTTP API with the UI token, or add write functions to a host-only (non-UniFFI) API and restate DD-32's control point, for example as the desktop session plus the token. Say whether the native window is optional on headless hosts.

### F-05 — Major — §5.5 "Inclusion record", §5.6 last paragraphs, DD-46, glossary "Paper identity", AT-60/83/89
**Quote:** "the inclusion record's paper identity points at that existing path"; "`object_hash` is CRC32C of the UTF-8 paper identity"; "Key: working archive + paper identity + verb id".
**Problem:** A tile-10 record stores only a 32-bit CRC of the identity, so it can't "point at" a path. After a restart, idempotency can only be checked through `object_hash`, which means CRC32C collisions between different papers silently drop a real inclusion. The same paper can also have two identities: `SRC-0042` and a decimal `paper_id`, or the same `SRC` stem under both `--wiki-root` and `--corpus-root`. That yields duplicate records. The candidate list reads "the corpus archive" (singular), but `--corpus-root` is a directory. URL identities can never appear in the candidate list.
**Fix:** State how idempotency is evaluated on disk (a scan of tile-10 records by `object_hash` plus verb), and how collisions are handled, for example a secondary check against the snapshot or provenance. Define dedup and precedence between the SRC and `paper_id` identities and across roots. Name which `.s3a` files under `--corpus-root` are scanned. Drop or rephrase "points at that existing path".

### F-06 — Major — §5.2.1 `/v1/select`, `/v1/download`, `/v1/skip`; §5.5
**Quote:** `{ "paper_id": "<paper identity>" }` → `200 { "ok": true, … }`
**Problem:** The spec never says whether `paper_id` must be in the current candidate list. An arbitrary string, including a URL or a path, would write an inclusion record and possibly trigger a fetch. Error codes for an unknown paper, malformed input and fetch failure aren't defined, and no AT covers a non-candidate select.
**Fix:** Require `paper_id` to resolve to a current candidate, or define the accepted forms. Add a `404 unknown_paper` (or similar) error code and an AT showing that an unknown id writes nothing.

### F-07 — Major — §5.5 download bullet, DD-56, AT-94, AT-83
**Quote:** "If the destination name already exists, the download fails and does not overwrite".
**Problem:** The spec never defines how the destination filename is derived (identity, SHA, URL basename, sanitization). AT-94 can't build a "file of the destination name" without that rule. A second download of the same paper always collides and fails, which conflicts with the idempotent, retry-friendly model in §5.5. A filename taken from a remote URL is also a path-traversal vector.
**Fix:** Specify the deterministic name (for example `{paper identity sanitized}.{ext}` or `{sha256}.{ext}`) and its sanitization rules. Say whether re-downloading the same paper with identical bytes is a success no-op or an error.

### F-08 — Major (security/privacy) — §5.6 "API and locking", DD-53
**Quote:** "Snapshots are exempt from that filter when opened as kite-written snapshot files in `--snapshot-dir`".
**Problem:** The spec gives no test for "kite-written". Read literally, any `.s3a` placed in `--snapshot-dir` bypasses research-purpose consent filtering, which makes that directory a consent-bypass drop box. There is also no AT for the exemption or for its negative case.
**Fix:** Define the criterion, for example a filename equal to the SHA-256 of the reassembled BLOB, the exact BLOB plus PROVENANCE tile set and no other record tiles, and a valid seal. Otherwise apply the normal filter. Add a positive AT and a negative AT (a third-party archive copied into `--snapshot-dir` stays filtered).

### F-09 — Major (security) — §5.2.4, DD-31, AT-82, AT-88
**Quote:** "Deny at connect time… loopback…"; "unless `--egress-allowlist` is set, in which case only those hosts are allowed (tests for a local fixture server use the allowlist)".
**Problem:** The precedence is ambiguous. If the allowlist overrides the deny list, which tests need in order to reach a loopback fixture, then an allowlisted host that resolves or redirects to loopback or metadata is reachable. That undercuts AT-55 and AT-88. If the deny list wins, the fixture-server tests can't run. The allowlist syntax (hostnames, IPs, ports, wildcards) and redirect handling are also undefined.
**Fix:** State that the deny list always applies, and add an explicit test-only exception: a separate flag that is rejected in release builds, or tests that use a non-loopback test address. Alternatively, state that the allowlist overrides only named literal IPs. Define the allowlist grammar and say whether redirects must also stay inside it.

### F-10 — Major — §4.4, §5.5, DD-12, DD-33 (LRS client)
**Quote:** "`s3a-lrs-server` is the LRS (`POST /xapi/s3a/batch`)"; "The xAPI statement… uses that same verb IRI and uses the source URL as the object id…"
**Problem:** The spec never says how the host authenticates to the LRS (credentials flag, file, mTLS) or whether `--lrs-url` must be `https`. Most of the statement is undefined: the xAPI actor (how `--researcher-id` is represented, and whether it is hashed), the timestamp, the statement id or idempotency on retry, and the batch format. Without these, AT-26 can't check the statement, and replays from the retry queue can duplicate statements.
**Fix:** Add LRS credential and transport requirements, plus a statement schema covering actor representation (with a privacy rationale), the id used for deduplication on retry, and the timestamp. Add an AT asserting the statement body.

### F-11 — Major (privacy) — §5.5 "Retry queue", DD-51, AT-91
**Quote:** "the host retries at process start and after each later successful LRS emit".
**Problem:** The spec doesn't require checking the current consent state before a startup retry. A queue left on disk, for example after a crash between revoke and deletion, or after a failed delete, would be sent at the next start even if the consent file says `off`. The case of a failing revoke-time deletion isn't defined either.
**Fix:** Require every LRS send, including a retry, to check that the in-memory consent state is `on`. On startup with consent `off`, delete or ignore any existing queue. A failed deletion on revoke is a visible error and must not leave the queue sendable. Add an AT: queue present plus consent file `off` at start means zero LRS requests.

### F-12 — Major — §4.2, §4.7, §5.6, DD-53, AT-79 (hyper-tile consent semantics)
**Quote:** §4.2: "records without active `purpose=research` are withheld"; §4.7: `evaluate_consent_summary` returns "open / unrestricted (no consent records)" when there are no consent records; §5.6: on create kite writes a `ConsentRecord` "so the owner's records are visible".
**Problem:** These are inconsistent. If no consent records means unrestricted (§4.7), the `ConsentRecord` written on create isn't needed for visibility, and AT-79's rationale is wrong. If no records means withhold everything (§4.2), then a hyper-tile file with no consent records behaves differently from the Edge rule in AT-85, and the spec never says so. DD-53 states the unrestricted rule only for Edge files. Hyper-tile files with zero `ConsentRecord`s have no specified behavior and no AT.
**Fix:** State the hyper-tile rule explicitly: no core consent records means either unrestricted or fully withheld. Add an AT for it, and then justify or drop the self-consent `ConsentRecord`.

### F-13 — Major (sequencing / contradiction) — §11 step 3 vs §4.2, §5.3, Risk 3
**Quote:** "Implement hyper-tile manifest + warehouse paint … against `tour_file` data for hyper-tiles"; §4.2 says `tour_file`'s consent state includes the type-82 branch, and the unsealed fallback ignores `tile_stride`.
**Problem:** Building on `tour_file` output imports the type-82 `withheld_count` bug that AT-11 forbids. It also imports the stride-blind scanner that §5.3 and AT-97 forbid, because a 64 KiB-stride file would be scanned as 128 KiB. The implementation order contradicts the architecture.
**Fix:** In step 3, use `MmapReader` for all file types and reuse only the parts of `tour_file` that are safe, naming which (for example manifest extraction and floor-list fields). Compute `withheld_count` in kite.

### F-14 — Major (AT can't pass) — §5.2.2 token rules, AT-45, AT-81
**Quote:** "Token comparison is constant-time on the raw file bytes. The file is not trimmed… Encoding is not required to be UTF-8"; AT-81: a 33-byte token "is accepted only when the `Authorization: Bearer` value is exactly those 33 bytes".
**Problem:** An HTTP header value can't contain a trailing LF, because the LF ends the header line. Arbitrary non-UTF-8 or control bytes aren't valid header octets either. A token file ending in a newline, which is the common case from `echo`, can therefore never authenticate, and AT-81's "accepted" half can't be satisfied over HTTP. The requirement and the test contradict each other.
**Fix:** Restrict token files to a header-safe charset (for example base64url or hex, at least 32 characters, no whitespace), and reject non-conforming files at startup. Alternatively, define one exact normalization (strip a single trailing `\n`). Rewrite AT-81 to match.

### F-15 — Major — AT-31 vs §5.6 / §5.5
**Quote:** AT-31: "After an inclusion (consent on or off), a sealed snapshot exists under `--snapshot-dir`"; §5.6: "On-disk wiki or corpus sources are not re-snapshotted"; §5.5: "writes a snapshot when bytes are captured".
**Problem:** AT-31 states unconditionally that a snapshot exists, but an inclusion of an on-disk `SRC` dossier produces none. The test either fails on valid behavior or depends on an unspecified fixture. AT-27 and AT-48 hedge with "when bytes were captured", which makes their snapshot clause untestable unless the fixture is pinned.
**Fix:** Pin the fixture in AT-31, AT-27 and AT-48 to a remote-fetched paper (per F-02). Add an AT showing that an on-disk paper inclusion creates no snapshot.

---

## Minor

### F-16 — Minor — §4.1 bullet on Boa vs DD-26/DD-44, §5.2 item 4
**Quote:** "Boa remains relevant only for ordinary `http(s)` pages the Scholar browser may open, under the model in §5.2 and DD-26."
**Problem:** This contradicts DD-26 and DD-44 as amended (no Boa in v1) and §5.2 item 4 ("does not start Boa").
**Fix:** Change it to say Boa is not used in v1.

### F-17 — Minor — Cross-reference errors
**Quotes / problems:**
- §5.6 cites "(DD-53, AT-86)" for the create-time `ConsentRecord`, but AT-86 covers Edge identity exemption. The right reference is AT-79.
- AT-44 cites "the fork change required in §5.7", but §5.7 says nothing about getrandom. Risk 2 or §11 step 2 covers it.
- AT-23 says "(AT-86)" for identity visibility. That is correct, but AT-86 then says "(AT-23)", which is circular and leaves no independent oracle.
- §4.3 claims §5.4 states the DD-07 override (see F-03).

**Fix:** Correct the references.

### F-18 — Minor — §5.8, DD-37 vs §5.1, AT-96, `/v1/frame`; preamble "no invented runtime defaults"
**Quote:** "A `device_pixel_ratio` parameter is accepted and defaults to `1.0`".
**Problem:** None of the §5.1 signatures, the AT-96 export list or `/v1/frame` includes the parameter. A silent default also conflicts with the stated rule "No… invented runtime defaults". `scroll_x`/`scroll_y` defaulting to 0 and `instant` defaulting to false have the same issue.
**Fix:** Add `device_pixel_ratio` to the signatures and the query, or remove it from v1. Document the defaults as an explicit exception to the no-defaults rule.

### F-19 — Minor — DD-04, §5.8, AT-16, AT-92
**Quote:** "bays are square and area is linear in stride … 8 KiB bay's side is exactly 45" ; AT-16: "Bay area is therefore linear in stride".
**Problem:** 45² = 2025, which is not 2 × 32² = 2048, so area is only approximately linear. The "therefore" in AT-16 is false. The 128 KiB hyper-tile bay size is never specified, and neither is the layout for mixed strides. AT-16 also asserts a 4 KiB bay side on an 8 KiB-only fixture.
**Fix:** Say "area approximately linear (sides rounded to the nearest integer)". Specify the hyper-tile bay side (for example round(32·√32) = 181) and a fixture for each stride.

### F-20 — Minor — §5.8, DD-18, AT-62
**Quote:** "to 6vw/8vh/88vw/84vh".
**Problem:** The spec never maps viewport units to the `width`/`height` render arguments. The start rectangle (the bay's rect at the current scroll) and the behavior when the bay is off-screen are also undefined.
**Fix:** Define 1vw = width/100 and 1vh = height/100 of the frame, and the start rect as the bay rect at the given scroll, with a rule for off-screen bays.

### F-21 — Minor — AT-06 vs §5.2.1 "Any other path returns 404 … and still requires a token"
**Problem:** An unauthenticated request to `/json` can't be both "rejected for missing token" and "404". The required status is ambiguous, and so is whether 404 leaks route existence.
**Fix:** State the order: authentication first (401 without a token, 404 with a valid token on an unknown path). Align AT-06, AT-57 and AT-75.

### F-22 — Minor — §5.2.1 `POST /v1/consent` vs AT-76
**Quote:** The table says `POST /v1/consent` returns "`200` with the same body as GET"; there is no 409 note, unlike the other Scholar routes. AT-76 expects 409.
**Problem:** The table and the AT are inconsistent. The spec also doesn't define what happens when `POST {state:on}` is sent while `--lrs-url` is unset (§5.2.3 says tracing is off in that case).
**Fix:** Add "Scholar mode; 409 in viewer-only" to the row. Define a `409 lrs_not_configured` (or similar) error for turning consent on with no LRS.

### F-23 — Minor — §5.2.3 consent file without LRS; DD-12
**Quote:** "if `--lrs-url` is absent … the consent file, if the flag is still passed, must start as `off` or be absent"; DD-12: "If consent is on and the URL is missing, the LRS statement fails closed".
**Problem:** The spec doesn't say what happens when the file says `on` and LRS is unset: a startup error, or ignored. The phrase "forbidden to be the only consent mechanism" is unclear. DD-12 describes a state that §5.2.3 makes unreachable.
**Fix:** Make `on` without `--lrs-url` a startup error naming the file, and add an AT. Remove the unreachable DD-12 clause or note that it is unreachable.

### F-24 — Minor — §5.2.2, DD-52, AT-87 (Unix-socket bind)
**Quote:** "`--behind-proxy` is accepted only with a loopback bind or a Unix-socket bind".
**Problem:** The syntax for a Unix-socket bind is undefined, and `--port` is mandatory in every mode. Behind a loopback proxy, every remote request arrives from loopback, so remote clients can reach `GET /health` unauthenticated. The token rule for `--behind-proxy` (is a token still required?) is also unstated.
**Fix:** Define the Unix-socket syntax and the `--port` rule for it. State that the token is still required behind a proxy, and that `/health` stays tokenless on loopback (or not).

### F-25 — Minor (security) — §5.2.2 token permissions
**Quote:** "Unix permissions must be `0600`."
**Problem:** No AT checks the permission rule. The ownership requirement is unstated, and so is behavior on a non-Unix lab PC (§1 allows lab PCs). The spec also never says tokens must be kept out of `--log-dir` and error messages.
**Fix:** Add an AT for a `0644` token file being refused at startup. Specify the owner check and the Windows ACL rule, or limit v1 to Unix. Add "never log token bytes or Authorization headers", with an AT that greps `--log-dir`.

### F-26 — Minor — §5.2.1 handles
**Quote:** "`<handle>` is an opaque string minted by `/v1/open`."
**Problem:** There's no close route, lifetime, cap or eviction rule, and nothing on concurrency for the "last frame of `view`" state used by `hit_test`. A client can keep opening files and exhaust memory. Concurrent `/v1/frame` calls on one handle make `hit_test` ambiguous.
**Fix:** Add `POST /v1/close`, a maximum number of open handles with an error code, and a rule that `hit_test` uses the last frame rendered for that handle and view, with defined concurrency behavior.

### F-27 — Minor — §5.3 item 1 vs AT-97
**Quote:** "fail or quarantine as an unsupported stride (AT-97)".
**Problem:** "fail or quarantine" lets two different results pass. AT-97 says "opens as unsupported / inconsistent", which implies quarantine. That is ambiguous for `/v1/open`, which could return 200 or an error.
**Fix:** Pick one outcome and give the `KiteError` code or the `kind`/banner value.

### F-28 — Minor — §5.4 item 2, DD-57, AT-35
**Quote:** "the tiles whose `seal_hash` the signature was meant to cover".
**Problem:** "Meant to cover" isn't operational. The spec doesn't say how the covered range is determined from the type-70 record, or what happens when there are several type-70 tiles.
**Fix:** Define the covered range in terms of the type-70 record's fields (for example its chain index), and the rule for multiple signatures.

### F-29 — Minor (security) — §5.2.4 deny list gaps; §5.3 item 7
**Problem:** The deny list names only IPv4-mapped loopback. It leaves out mapped RFC1918 and link-local addresses, `0.0.0.0/8`, `::`, multicast/broadcast, NAT64 and 6to4 forms, and IPv6 site-local. It also doesn't say whether `HTTP(S)_PROXY` environment variables are ignored (a proxy would defeat connect-time IP checks). There's no response-size cap for fetches, and "no extra size cap" applies to opened files, which allows disk and memory exhaustion of `--snapshot-dir` and the phone.
**Fix:** Deny all non-global unicast addresses, including mapped and translated forms. Ignore proxy environment variables, or validate the final peer. Add a maximum fetch body size (a DD) and an AT.

### F-30 — Minor — §5.2 item 4, DD-13 vs DD-26 (remote HTML subresources)
**Problem:** Wiki pages are explicitly "no remote subresources", but the rule for host-fetched `http(s)` HTML being laid out isn't stated: are CSS, images and fonts fetched, and through which egress policy? AT-56 covers only wiki Markdown.
**Fix:** State that remote HTML gets no subresource fetches in v1, or that any it gets go through §5.2.4. Add an AT.

### F-31 — Minor — AT-29, DD-08 (Pass 1 timer)
**Problem:** The 15-second timer exists only in the native UI. No API exposes it, so AT-29 has no test surface and can't fail unless the native UI is instrumented.
**Fix:** Name the test surface (a native-UI test harness or a host clock injection), or reclassify AT-29 as a guard that checks for no write calls on timer expiry.

### F-32 — Minor — AT-12, AT-34, AT-33 "no Boa/JS context"
**Problem:** Boa is wasm32-only (§4.1) and Wasmtime isn't a dependency, so on a native host these assertions hold trivially and can't fail. The spec also doesn't say how "no JS context is created" is observed.
**Fix:** Make them assertions on the dependency graph (for example `cargo tree` with no `boa_engine` on the host or Android targets), and label them guards.

### F-33 — Minor — AT-55
**Problem:** The token file, the working archive and corpus files are only reachable over HTTP through a loopback or local server, which AT-54 and AT-88 already deny. It's also unclear what "a host fetch URL that would retrieve the token file" is: a `file://` URL? Through which route (F-02)? As written, the test can't fail independently.
**Fix:** Specify the attempted vector (for example `file://` passed to the fetch path, or an allowlisted fixture server serving the file) and the expected error.

### F-34 — Minor — Preamble on negative write tests vs AT-46, AT-76; DD-48
**Quote:** "Negative write tests… first run a control select that succeeds… in the same harness"; AT-46: "select against a second working archive"; AT-76 is listed with "control-then-negative".
**Problem:** One host serves one working archive, so AT-46 needs a second host process, which the spec doesn't say. A viewer-only host can't run a successful control select.
**Fix:** In AT-46, specify a second host instance with `--working-archive` pointed at the bad file. For AT-76, run the control select on a separate Scholar host, or drop the control requirement.

### F-35 — Minor — AT-39 and AT-98 fail-first labeling
**Problem:** AT-39 (no WebView as the tour surface) probably holds today, since §4.5 says the current visualization is a Kotlin `MapLayout`, so it is a guard rather than fail-first. AT-98 is conditional ("if the optional path-mmap path is used"), so an implementation without mmap passes it trivially. Truncating a file under an mmap normally raises SIGBUS, and the spec doesn't say whether this requirement is meant to force signal handling.
**Fix:** Relabel AT-39 as a guard, or give its fail-first reason. Make the mmap path either forbidden or required in v1, and state the SIGBUS strategy.

### F-36 — Minor — §5.2.2 viewer-only `--data-dir`; DD-55; §1 "requires durable local disk"
**Quote:** "Optional: `--data-dir` (if omitted, the host uses no durable data dir and keeps only ephemeral temp)".
**Problem:** The spec never says what viewer mode stores in `--data-dir` when it is given. "Requires durable local disk" in §1 and DD-48 reads as universal, yet viewer mode works without it. Opening the working archive through `/v1/open` while the host holds its exclusive write lock (Scholar mode) is also undefined: whether it reads a snapshot, blocks, or errors.
**Fix:** State what viewer mode uses `--data-dir` for, or drop the flag from viewer mode. Scope "durable disk" to Scholar mode. Define reads of the locked working archive (for example a `from_bytes` copy taken under a shared read).

---

## Notes (not findings)
- The scope exclusions (no CDP, no page JS/Wasmtime, no phone writes, no Cloud Run), DD draft values and Q-01–Q-11 are accepted as written.
- `actor_hash` wording differs slightly between §5.5 ("same hashing already used… when one exists, otherwise CRC32C") and DD-46 ("CRC32C"). Align them when Q-07/Q-08 are closed. This is minor and folded under F-17's cleanup.
