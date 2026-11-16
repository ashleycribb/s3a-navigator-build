# Goldfish review: s3a-kite.md, round 1

Reviewer: independent, no prior context. Source read: `/workspace/goldfish/s3a-kite.md` only. Draft decisions (DD-xx), open questions (Q-xx) and the external requirements the spec lists are not findings unless the spec contradicts itself about them.

## Summary

| Severity | Count |
| --- | --- |
| Blocker | 3 |
| Major | 21 |
| Minor | 14 |
| **Total** | **38** |

---

## Blockers

### F-01 [Blocker] The spec never says how `http(s)` pages run JavaScript on a native host (§4.1, §5.2 item 4, Risk 1, DD-02, AT-33)
- Quote: "Boa (page JavaScript inside WebAssembly…)"; "Much of Kitesurf's DOM and JS is `#[cfg(target_arch = "wasm32")]`"; "For `http(s)` pages: one sandboxed process (or equivalent isolate) per page… This is the replacement for `LOADER.load`."
- Problem: The Scholar browser has to open ordinary web pages (§4.1, AT-33). The spec says page JS runs in Boa *inside Wasm*, and that the DOM and JS code is wasm32-only. It also says the host links `kitesurf-core` natively (DD-02), which only covers the `.s3a` tour ("Wasmtime is not required for the `.s3a` tour"). It never picks a runtime for web pages: native Boa (which needs fork work that isn't scoped), Wasmtime with the Wasm core, or no JS at all. The TS `page-script` globals are also dropped without a replacement. So nobody can build the web-page path, and AT-33 cannot be designed.
- Fix: Add a requirement (or a DD) that names the web-page execution model: Wasm core under Wasmtime in a sandboxed subprocess, native Boa, or JS off in v1. Name what replaces `packages/page-script`. Extend AT-33 to check that model (for example, a page script that sets `document.title` either runs or provably does not).

### F-02 [Blocker] The write target for inclusion records and snapshots is undefined, and it conflicts with the read-only viewer and sealed archives (§5.3 item 1, §5.5, §5.6, AT-24, AT-31, DD-15/16)
- Quote: "Open with `s3a_engine::MmapReader` (or an equivalent read-only engine API)"; "A snapshot… is a sealed write of the captured bytes into existing core BLOB + PROVENANCE tiles"; "leaves the corpus archive and the LRS store byte-identical".
- Problem: The spec never says which `.s3a` file receives Scholar inclusion records and snapshot tiles. It could be the corpus archive (AT-24 implies the corpus is writable), a separate per-researcher archive, or something else. It also never says how a write lands in an already-sealed archive (append and re-seal? new generation?), which write API the pack uses (every API it names is read-only), what happens when `ADDON_EDU` is not set (DD-16 says writes "require" it but gives no behavior when it's missing), or whether concurrent writers (host plus phone) are allowed. §5.5 and §5.6 can't be built without this.
- Fix: Add §5.6a "Write path". Name the target archive, the engine write API, the seal and append semantics, the error when `ADDON_EDU` is missing, and the locking model. Add an AT that fails when the target lacks `ADDON_EDU`, and one that re-verifies the seal after a write.

### F-03 [Blocker] Tracing consent also gates the researcher's own inclusion data, so Pass 2/3 cannot work with consent off (§2 "Inclusion", §5.5, AT-27, DD-09)
- Quote: "That is the recorded Pass 1 event, and only with tracing consent."; "Consent off: zero LRS requests and no inclusion record."; "Snapshots of selected papers… require tracing consent."
- Problem: Consent to *tracing* (xAPI telemetry to an LRS) is a different thing from the researcher's *working record* of which papers they chose for Pass 2/3. As written, a researcher who declines tracing can select and download papers, but nothing records the choice and no snapshot is taken. Pass 2/3 then has nothing to work from. The spec also doesn't say whether "download" with consent off still delivers the file. This is a functional gap that comes from mixing two concepts.
- Fix: Split "tracing consent" (LRS statements) from "local inclusion record or snapshot". State explicitly what is kept locally with consent off: either nothing, plus the user-visible consequence of that, or a local-only record. Reword AT-27, AT-31 and DD-09 to match.

---

## Major

### F-04 [Major] Phone tracing rule contradicts DD-12, AT-27 and the no-network app (§5.7 Android bullet 4, §4.5, DD-12, Q-03)
- Quote: "Scholar tracing on the phone happens only if the user has enabled consent *and* an LRS endpoint is configured; otherwise select records locally or errors closed (DD-12)."
- Problem: "records locally" contradicts DD-12 ("If consent is on and the URL is missing, select fails closed with an error; it does not silently skip"). With consent off, it also contradicts AT-27 ("no inclusion record"). "Or" leaves the choice to the implementer. And §4.5 says the Android shell has no network permission, so "an LRS endpoint is configured" can never lead to a request.
- Fix: Give one rule per state (consent off, on with no URL, on with a URL) for each platform, and say whether v1 phone builds have any Scholar write path at all. Tie it to Q-03.

### F-05 [Major] On-demand font downloads are impossible in a no-network app (DD-22, §5.7 Fonts, §4.5, Q-05)
- Quote: "Noto Color Emoji and Noto Sans SC stay optional on-demand assets"; "The Android shell has no network permission".
- Problem: "On-demand" means fetching the fonts, which the app can't do. The spec also doesn't say what happens when a manifest has CJK or emoji text and those fonts are absent: tofu, an error (AT-42 treats missing fonts as an error), or a label.
- Fix: Define "on-demand" (Play Feature Delivery, sideloaded asset pack, or bundled). Define the missing-glyph behavior. Add an AT with a CJK manifest and no SC font.

### F-06 [Major] AT-10 and AT-11 use `s3a-cli tour` as the oracle, but the spec says `tour.rs` has bugs kite must not copy (§4.2, Risk 3, AT-10, AT-11, AT-23)
- Quote: "Opening floor *N* shows the same records and the same `withheld_count` as `s3a-cli tour --floor N`." vs "A comment in `tour.rs` treats `type_id == 82` as `EdgeConsentRecord`… s3a-kite must not copy the type-82 mistake."
- Problem: If the CLI's consent evaluation is affected by the type-82 confusion, matching its `withheld_count` byte-for-byte reproduces the bug. Using a known-defective tool as the oracle makes the tests ambiguous.
- Fix: Say whether the type-82 issue affects hyper-tile `withheld_count` or is only a comment. If it does, fix the CLI first (a separate change) or use golden fixtures with hand-verified counts.

### F-07 [Major] AT-07's "non-documented production mode" clause is untestable and contradicts §5.2 and DD-03 (AT-07)
- Quote: "Omitting `--port` or `--bind` in a non-documented production mode exits non-zero…" vs §5.2 "Missing values are an error. There is no silent production default."
- Problem: The phrase suggests that some documented mode may default the port or bind, which §5.2 forbids. "Production mode" is never defined, so a tester can't tell which invocation to run.
- Fix: "Omitting `--port` or `--bind` in any mode exits non-zero and names the missing flag." If a dev mode exists, define it explicitly.

### F-08 [Major] Health URL vs fail-closed auth is undefined (AT-04, AT-06, DD-11)
- Quote: "a configured health URL returns success" vs "The same host without a valid token rejects HTTP and CDP".
- Problem: The health URL isn't named or configured anywhere. If it needs no token, AT-06's "rejects HTTP" is false for that path. If it does need a token, Cloud Run and load-balancer probes need the token. The spec also doesn't say how the token is presented for HTTP (header name) or for CDP WebSocket upgrades.
- Fix: Name the health path, state whether it needs auth, and specify the token transport for HTTP and CDP. AT-06 should cover every route except the health path.

### F-09 [Major] Host transport security and token handling are missing (§5.2, DD-11)
- Problem: The host can bind to non-loopback addresses on a hub, Aura Rack or LRS VM, with bearer-token auth, but the spec has no TLS requirement. Nothing covers constant-time comparison, token file permissions, rotation, behavior on an empty or short token, or rate limiting. A plaintext bearer token on a lab LAN fails the "fail-closed" intent.
- Fix: Require TLS (or loopback-only plus a reverse proxy) for any non-loopback bind. Add token file rules (permissions, minimum entropy, an error when the file is empty). Add an AT that rejects `--bind 0.0.0.0` without TLS or the proxy flag.

### F-10 [Major] Authenticated CDP and agents have no defined capability scope; filesystem access isn't bounded (§2 "Host", §5.2, §5.5 "wiki tree that the host is configured to open")
- Problem: The host serves "users or agents" and keeps CDP open to them. Nothing limits what an authenticated session can do: `file://` reads, opening arbitrary `.s3a` paths, `Runtime.evaluate`, outbound requests. The wiki and corpus roots are "configured" but there's no config key, path canonicalization rule, or traversal protection. On the LRS VM this is an arbitrary-file-read and SSRF risk.
- Fix: Define the config (allowed roots, archive roots), canonicalize paths and reject anything outside the roots, decide whether `file://` is allowed, and list which CDP domains are enabled. Add ATs for `../` traversal and for `file:///etc/passwd`.

### F-11 [Major] "Egress policy" and "sandboxed" are undefined and untested (§5.2 item 4, AT-33)
- Quote: "one sandboxed process (or equivalent isolate) per page, with egress policy."
- Problem: Neither term has a definition (allowlist? deny private ranges? proxy?). AT-33 only checks that the HTML path is used, not isolation or egress. Upstream's warning about outbound requests (§4.1) is the exact risk, and the host sits next to the LRS and the lab network.
- Fix: Specify the sandbox mechanism per platform and a default-deny egress rule (at minimum, block RFC1918, link-local and metadata IPs). Add ATs: a page fetch to `169.254.169.254` or the LRS's private IP is blocked, and a page process can't read the host FS.

### F-12 [Major] Wiki and corpus Markdown is untrusted HTML with no sanitization or remote-resource rule (§5.5, DD-13, AT-12)
- Quote: "The pack parses CommonMark with `pulldown-cmark` to HTML, then `kitesurf-core` / Blitz lays out that HTML."
- Problem: Corpus Markdown includes AI-extracted paper text (§4.4) and can contain raw HTML, `<script>`, `<img src=remote>` and `<link>`. AT-12 covers only `.s3a` text painting. The spec doesn't say whether raw HTML passes through, whether scripts run (Boa) on wiki pages, or whether remote images or styles load. A remote load is both a tracking/privacy leak, bypassing consent, and an egress path.
- Fix: Require raw-HTML stripping or escaping and no JS for wiki pages, with remote subresources blocked by default. Add an AT with Markdown containing `<script>` and `<img src="http://…">` showing no execution and zero network requests.

### F-13 [Major] AT-28's "human select vs API call" distinction is unenforceable when agents drive the host over CDP (§2 "Host", §5.5, AT-28, §1)
- Quote: "An API call that tries to write include/exclude/score without a human select or download is rejected".
- Problem: The host deliberately serves agents through CDP, and an agent can synthesize the same click or download. The spec defines no mechanism (user gesture provenance, separate credentials, UI-only endpoint) that tells human from agent, so the requirement can't be built or tested as written.
- Fix: Define the mechanism: human writes only from an authenticated UI session with a trusted-gesture flag, and agent tokens are read-only. AT-28 then tests that an agent token's synthesized select is rejected.

### F-14 [Major] xAPI actor identity, consent capture, consent storage and revocation are missing (§1, §5.5, DD-12, Q-02)
- Problem: The spec says tracing is consent-only but never says who the actor is (which researcher identity goes into statements), where consent state is stored, how it's granted or revoked, or what happens to queued or already-sent statements on revocation. Q-02 covers only the consent copy text.
- Fix: Add a consent model section: identity source, storage, a toggle in the UI, revocation semantics, and an AT that revoking consent mid-session drops LRS requests to zero.

### F-15 [Major] Failure, idempotency and ordering of the inclusion side effects are unspecified (§5.5, §5.6, AT-26, AT-31)
- Problem: A consented select produces three effects: an LRS statement, a local inclusion record and a snapshot. The spec doesn't say what happens if the LRS is down (retry, queue, roll back the record?), if the snapshot fetch fails, or if a paper is selected and then downloaded, or selected twice (AT-26 says "one inclusion statement").
- Fix: Define atomicity or ordering, retry and queue behavior, and an idempotency key per paper. Add ATs for LRS 5xx and for a duplicate select.

### F-16 [Major] Host logs and caches are outside the "writes nothing" guarantee (§5.5, AT-24, AT-25)
- Problem: AT-24 and AT-25 check only the corpus archive and the LRS store. The host could still log visited URLs, paper ids and skip actions (access logs, CDP traces, an HTTP cache), which records Pass 1 behavior without consent.
- Fix: Add a logging and caching policy: no paper ids or URLs in logs without consent, and caches cleared or excluded. Extend AT-24 and AT-25 to the host's data and log directories.

### F-17 [Major] Linear bay area across 4 KB, 8 KB and 128 KB doesn't come with a layout that can satisfy it, and AT-16 can't be run as written (§5.4 item 4, DD-04, AT-16)
- Quote: "a 128 KB bay is 32 times a 4 KB bay… In a frame that also includes a 4 KB bay (or a comparison fixture)".
- Problem: One file has one stride, so a 4 KB bay and a 128 KB bay never share a frame. Comparing across files requires a fixed absolute pixels-per-KB scale. At a scale where a 4 KB bay is visible, a hyper-tile file with hundreds of floors at 32× area overflows any phone viewport, and the spec has no scrolling, paging or overview rules. A "one-pixel tolerance" on *area* is also ill-defined.
- Fix: Define the scale (absolute px²/KB at a reference DPI), the scrolling and paging behavior for large floor counts, and the tolerance in linear dimensions. Rewrite AT-16 as two renders at the same scale with measured bay sizes.

### F-18 [Major] Vw/vh animation timing on a CPU frame renderer has no frame model, so AT-13 can't be tested as written (§5.3 item 3, DD-18, AT-13)
- Problem: The animation is specified in CSS viewport units and CSS easing. The native renderer produces pixmaps, but the spec never says what the "viewport" is (pixmap size? Compose bounds?), who drives time (does `render_warehouse_frame(t)` take a timestamp?), the target frame rate, or how a test checks "uses 0.45s cubic-bezier". Animation for Edge floors isn't mentioned at all.
- Fix: Define the frame API (time parameter, viewport size), the frame-rate and frame-time budget, and test the card rect at t = 0, 0.225 s and 0.45 s against the bezier within a tolerance. State that the same applies to Edge floors.

### F-19 [Major] Text assertions on pixmaps need a text-layer requirement that isn't specified, and accessibility is missing (AT-08, AT-14, AT-30, AT-38, §5.7)
- Quote: "OCR or a packed-in text layer test finds `THIS IS AN S3A FILE`".
- Problem: Many ATs assert on frame "text", but the renderer outputs RGBA. OCR is flaky, and no requirement defines a text layer. The same gap means Compose shows an opaque bitmap with no TalkBack or VoiceOver semantics, which is a scope gap for a primary viewer that replaces a native Kotlin UI.
- Fix: Require the render API to return a text and semantics tree alongside pixels (for test assertions and for accessibility). Base the ATs on it, and add an accessibility AT (TalkBack reads the manifest).

### F-20 [Major] The claim that every test fails today is false for several ATs, which are regression guards (§7 preamble, AT-22, AT-37, AT-36, AT-41, AT-05)
- Quote: "Each one is a concrete check that **fails on today's code**".
- Problem: AT-22 (s3a-edge doesn't depend on kite) and AT-37 (no new core TileType) pass today. AT-36, AT-41 and AT-05 pass vacuously until something exists. That's fine for guards, but it contradicts the tests-first premise and §11 step 1 ("so they fail").
- Fix: Split the ATs into "must fail first" and "regression guards", and reword the preamble.

### F-21 [Major] Tests-first sequencing breaks on names "as implemented" and on two repos (§11, AT-01, AT-38, AT-40, §5.1, §5.7)
- Quote: "(name as built, DD-01)"; "`open` + `render_manifest` + `render_warehouse_frame` (names as implemented)".
- Problem: Tests can't be landed first (§11 step 1) when their target names aren't fixed. The work also spans `s3a-storage` (the pack) and `s3a-reader` (UniFFI and Compose), with no rule for how the reader pins the pack version or where the AT-38/39/43 tests live. Step 6 also does the UniFFI surface last, but AT-38 needs it and step 1 is meant to land it.
- Fix: Fix the crate, binary and FFI function names in the spec. State the repo and test location for each AT and the cross-repo pin mechanism (git rev or path dep).

### F-22 [Major] "Not forking or publishing on GitHub" conflicts with vendoring into s3a-storage and shipping an Android app (§3, Status, §5.1, §5.7, §6)
- Quote: Non-goal "Forking or publishing Kitesurf on GitHub." vs "It vendors the forked Kitesurf Rust core" in `s3a-storage` (a GitHub repo) and shipping in the Play build (Q-05).
- Problem: §3 mixes document-level non-goals ("in this commit") with v1 product non-goals. Vendoring the fork into a public repo and distributing an APK *is* publishing a derivative, so the non-goal contradicts the plan.
- Fix: Separate "this document" non-goals from v1 product scope. State that v1 does distribute a derivative (in s3a-storage and the app) under §6.

### F-23 [Major] Who verifies on mobile is ambiguous: reader-core and kite both verify (§5.7, §5.4 item 2, AT-18, DD-19)
- Quote: "`s3a-reader-core` verification still runs first" vs §5.4 "Verify header CRC, per-tile… and the seal chain" (kite) vs AT-18 "matches `s3a-engine`".
- Problem: There are three verifiers (reader-core, kite, engine) with no rule for which is authoritative or what happens when they disagree. Hyper-tile seal verification in kite isn't specified at all (only CRC and Ed25519 in AT-35), and neither is unsealed hyper-tile handling (§4.2 mentions an unsealed fallback).
- Fix: Name one verification source per platform (for example, kite consumes the `s3a-engine` verification result, and reader-core is that result on mobile). Specify hyper-tile seal and unsealed behavior, with ATs.

### F-24 [Major] Edge event colors contradict the grounded phone-reader spec (§4.3 §10 bullet vs §5.4 item 5, DD-07, AT-19)
- Quote: §4.3 "event timeline (faults in red, maintenance in yellow, normal operations in green)" vs DD-07 "Color by severity: 0 green, 1 yellow, 2 and 3 red."
- Problem: The grounded profile colors by event *category*. The spec colors by *severity*. Kite replaces that phone reader, so the two rules collide (for example, a maintenance event with severity 0). The spec doesn't acknowledge the deviation.
- Fix: Either align with §10 or state explicitly that DD-07 overrides it, and give the event_type-to-category mapping. Add a non-color cue (icon or label) so color isn't the only channel.

---

## Minor

### F-25 [Minor] The manifest-to-tour transition trigger is unspecified (§5.3 items 2–3, AT-08, AT-09)
- Quote: "**Then** the warehouse tour"; "The tour may follow."
- Problem: The spec doesn't say whether the user taps, a timer fires, or the tour shows below the manifest. "May follow" in AT-09 is optional, so it can't be tested. Long-manifest wrapping and scrolling are also unspecified.
- Fix: Define the trigger and the long-text layout, and make AT-09 assert the tour is reachable.

### F-26 [Minor] Tile 78 edge cases are missing (§5.3 item 2)
- Problem: The spec doesn't cover tile 78 that is present but not valid UTF-8, multiple tile-78 tiles, or a quarantined tile 78.
- Fix: Specify the behavior (for example, a labeled "manifest not UTF-8" state, and use the first committed valid one), with ATs.

### F-27 [Minor] Torn and failed tiles are treated inconsistently (§5.4 item 2, AT-17, AT-18, AT-35, DD-20)
- Problem: Torn tiles are "omitted… counted as ignored", while CRC-failed tiles are "not shown as a valid floor", which could mean shown-as-invalid or omitted. For a broken seal chain, the spec doesn't say whether all later tiles become invalid.
- Fix: Use one rule for torn, CRC-failed and chain-broken tiles (omit, or show marked invalid), and state how far a chain break propagates.

### F-28 [Minor] Behavior for a missing `DeviceIdentityRecord` and for record-size discrimination is undefined (§4.3, §5.3 item 3, §5.4 item 5)
- Problem: Every Edge archive MUST have a `DeviceIdentityRecord`, but the spec doesn't say what the viewer does when one is missing or there are several. Type 64 with `record_count` 0, or a size that is neither 32 nor 96, is also unhandled.
- Fix: Define the outcomes (quarantine banner, "unknown layout" floor) and add fixtures.

### F-29 [Minor] Inconsistent Edge detection flags are unhandled (§2 "Edge archive", §4.3, §5.4 item 1)
- Problem: "flag and/or stride": the spec doesn't cover `FLAG_PROFILE_EDGE` with a 128 KB stride, or `FLAG_TILE_SIZE_4KB` set with stride 8192.
- Fix: Specify which field wins, or quarantine, with an AT.

### F-30 [Minor] AT-21's condition is always true (AT-21, §4.3)
- Quote: "`P = max(256, tile_stride)`, not at 512 if *P* is not 512."
- Problem: Edge strides are only 4096 or 8192, so P is never 256 or 512. The test reads as if it has a branch it doesn't have.
- Fix: "tile 0 at offset 4096 (4 KB) or 8192 (8 KB)".

### F-31 [Minor] "Research-purpose views" isn't defined (AT-23, §4.2)
- Problem: The spec has no view modes. It doesn't say whether the default viewer is a research-purpose view or who can see withheld records.
- Fix: Say "all kite views apply research-purpose consent filtering" (or define the modes).

### F-32 [Minor] Snapshot "captured bytes" and metadata are ambiguous, and AT-32 contradicts §5.6 (§5.6, AT-32)
- Quote: §5.6 "content SHA-256, source URL if any, and capture time" vs AT-32 "carries content SHA-256 and… URL. No other metadata fields are invented."
- Problem: AT-32 omits capture time, and "no other fields" could forbid it. "Captured bytes" could mean raw HTTP body, downloaded PDF, rendered HTML or PNG.
- Fix: Define the captured artifact per source type and list the exact fields, including capture time and its format and timezone.

### F-33 [Minor] AT-02 misses MPL source availability and in-app notices (AT-02, §6.2, §6.1 item 4)
- Problem: AT-02 checks a "distribution directory". It doesn't check the MPL source offer (§6.2 item 1), the APK or IPA artifact, or an in-app license screen. It also doesn't cover transitive crates (UniFFI, pulldown-cmark, etc.) beyond Kitesurf's NOTICE.
- Fix: Extend AT-02 to the APK contents and an About/licenses screen. Add a generated third-party license inventory (for example, cargo-about) and an MPL source-offer check.

### F-34 [Minor] AT-03 and AT-36 have no mechanical check (AT-03, AT-36)
- Problem: There's no defined way to enumerate the "files the pack modifies", or to decide that the source "contains no… code paths", across a vendored tree with many dependencies.
- Fix: AT-03 diffs against the pin and greps each changed file for the notice. AT-36 uses a dependency denylist (`cargo deny` bans) plus a grep on pack-owned source only.

### F-35 [Minor] Weak test wording lets tests pass while the requirement fails (AT-43, AT-41, AT-40)
- Problem: AT-43 says "not only `MapLayout` as the main surface", which passes with both views as primary and is weaker than DD-24 ("primary… not the default"). AT-41 says "used by the tour path", which leaves a loophole. AT-40 uses `cargo check` while §5.7 says "links".
- Fix: AT-43: "the default screen after open is the kite surface". AT-41: no GPU backend in the dependency tree at all. AT-40: build or link a staticlib.

### F-36 [Minor] No performance or memory budget, so DD-06's trigger can't be tested (Risk 5, DD-06, §5.7)
- Quote: "No second renderer until a measured memory budget on a named device fails."
- Problem: No budget or device is named, and there's no time-to-first-frame or APK size target. The trigger can never fire.
- Fix: Name the device and add numeric budgets (peak RSS, first-frame latency, APK delta) with an AT.

### F-37 [Minor] Robustness against malformed or malicious input is unspecified (§5.3, §5.4)
- Problem: The viewer mmaps untrusted files from SAF or share-sheet and the host. There's no fuzzing requirement, no size or tile-count limits, no handling of a file that changes under mmap (SIGBUS), and no note on how a SAF content URI becomes an mmappable fd.
- Fix: Add input limits, a fuzz target requirement (cargo-fuzz on open/render), and the SAF fd strategy.

### F-38 [Minor] Cross-references and units are ambiguous (§5.4 item 5 "§5.2.2", §4.2 "128 KiB" vs §2 "128 KB", the "timestamps" field)
- Problem: "§5.2.2" refers to the Edge profile but reads as this document's §5.2 (Native host). KB vs KiB is used inconsistently. The timestamp display format and timezone for header sheets and events aren't defined. File version compatibility (unknown versions) isn't covered.
- Fix: Prefix external refs ("EDGE §5.2.2"), standardize on KiB, define the timestamp rendering, and state the supported version range plus the error for unknown versions.
