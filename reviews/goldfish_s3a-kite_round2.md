# Goldfish review — s3a-kite spec (round 2)

Reviewer: an independent reviewer with no prior context. Input: `/workspace/goldfish/s3a-kite.md` only (v2, 2026-11-16). I did not read any other file, repo, earlier review, or web page. DD-xx, Q-xx, and external requirements are counted as findings only where the spec contradicts itself about them. The one numeric spot-check: the §5.8 bezier midpoint (≈0.946 at unit time 0.5) and the §5.8 bay sides (32√2 ≈ 45.25 px, 32√32 ≈ 181.0 px) are arithmetically correct.

## Summary

| Severity | Count |
| --- | --- |
| Blocker | 5 |
| Major | 17 |
| Minor | 20 |
| **Total** | **42** |

---

## Blockers

### F-01 — Blocker — §5.2, §5.3 item 3, AT-06, AT-28, AT-52, AT-74: no host API or UI surface is defined
> "All other HTTP routes and all CDP session upgrades require a valid token." / "the API `proceed_to_warehouse` is called on a UI token" / "A UI-token request to open a path …"

**Problem:** Apart from `GET /health`, the spec names no HTTP route, method, payload, or response. It also never defines how a desktop or hub user sees anything. It does not say whether that happens through a browser, a native client, a CDP screencast, or PNG frames over HTTP. "Open a path", "select", "download", "consent toggle", "Continue", `proceed_to_warehouse`, and "deliver the file to the researcher" have no wire contract. AT-06 ("rejects every HTTP route") cannot list the routes, and AT-28/AT-52/AT-57 cannot be written as concrete requests. On the desktop, the product itself ("s3a-kite is the S3A viewer") has no defined UI.
**Fix:** Add a §5.2.x "Host API" table that gives each route, method, required token kind, request and response schema, and error codes, plus the frame-delivery mechanism to the human UI. Then restate AT-06 as "every route in the table".

### F-02 — Blocker — §4.1, §5.2 item 4, §5.2 "CDP capability", AT-33: the CDP server has to be rebuilt, but nothing scopes that work
> "Upstream CDP lives in TypeScript under `packages/engine/src/cdp/`" … "Cloudflare layer … Replaced, not required at runtime." … "CDP domains enabled in v1: Target, Page, Runtime, DOM, Network (observation), Input."

**Problem:** Upstream CDP lives entirely in the TypeScript layer that the spec throws away. The native host must therefore provide a new CDP implementation of six domains, in Rust or some other language. No requirement says which methods and events of each domain exist, how they map onto the Wasmtime page process, or how a CDP session is tied to a page. AT-33 relies on `Runtime.evaluate` and a "document title surface", and AT-57 relies on CDP `Input`. §11 has no step for building CDP.
**Fix:** Add a CDP subsection that lists the supported methods and events per domain (or a minimum set tied to ATs), the session-to-page binding, and an implementation step in §11.

### F-03 — Blocker — §5.2 item 4, DD-26, §4.1, Risk 2: the plan to run the wasm32 core in Wasmtime has no defined ABI
> "runs page JavaScript by loading the existing `wasm32` `kitesurf-core` module (Boa included) in **Wasmtime 36**" vs §4.1 "Boa pulls `getrandom` 0.2, which on `wasm32-unknown-unknown` needs the `js` feature" and "`fonts.rs` uses `#[wasm_bindgen]`"

**Problem:** By the spec's own facts, the existing wasm32 module expects a JavaScript host: wasm_bindgen imports, the getrandom JS backend, and fonts handed over through wasm_bindgen. Wasmtime provides none of these. The spec never says who supplies those imports, how the module performs network I/O through the "host proxy", how fonts reach it, or how it returns a rendered frame or DOM to the host. It also does not say where the module comes from. It could be built from the pinned source with a reproducible command, or downloaded the way `kitesurf-cli` downloads its rasterizer. Supply chain and license inventory (AT-70) depend on that answer. "Loading the existing module" is not buildable as written.
**Fix:** Specify the host-import ABI (or a WASI target build of the core), the fetch/proxy import, font delivery, the frame and DOM return path, and "the page Wasm is built from `235111c` source by `<command>` in CI, never downloaded". Add an AT that checks the module hash against the CI build.

### F-04 — Blocker — §5.8, §5.1 FFI list, §5.3 items 2–4, AT-13/AT-43/AT-62: the render API cannot express what the tests require
> "`render_manifest(handle) -> RenderFrame`" / "`render_warehouse_frame(handle, t_seconds, viewport_width, viewport_height, instant)`" / FFI functions "`open`, `render_manifest`, `render_warehouse_frame`"

**Problem:**
- `render_manifest` takes no viewport, yet the manifest "wraps to the pixmap width and scrolls".
- Neither function takes a scroll offset, so the "scrollable grid" cannot be reached.
- `render_warehouse_frame` has no floor index, so nothing says which tile's card is animating or where the t = 0 tile rectangle sits.
- No function renders floor-N detail (records, `withheld_count`), takes Continue/`proceed_to_warehouse`, or delivers input such as taps.

AT-43 (Continue on Android), AT-11 (floor detail), and AT-62 (start rectangle) therefore cannot run through the declared FFI.
**Fix:** Extend the API, for example `render_manifest(handle, w, h, scroll_y)`, `proceed_to_warehouse(handle)`, `render_warehouse(handle, w, h, scroll)`, `render_floor_transition(handle, floor_index, t, w, h, instant)`, `render_floor_detail(handle, floor_index, w, h, scroll)`, and a hit-test function. Make §5.1's FFI list match.

### F-05 — Blocker — §5.2 "Configured roots", §5.6, AT-74, AT-31: the working-archive and snapshot path rules contradict each other
> "Each path is canonicalized (`realpath`) at startup; a missing or non-canonicalizable path is a startup error. … must be exactly the `--working-archive` path." vs §5.6 "`create` when the working archive does not yet exist" vs "A snapshot is a sealed `.s3a` written beside the working archive" vs AT-74 "Select with no `--working-archive` configured (host) … fails closed"

**Problem:** There are three contradictions:
1. `realpath` on a working archive that does not exist yet fails at startup, so the `TileWriter::create` branch can never run on the host.
2. Snapshot files live beside the working archive, outside both roots and not equal to the working-archive path, so the host may not open them. AT-31's "re-open verifies" is forbidden.
3. `--working-archive` is a required startup flag, so the host half of AT-74 can never happen. The host refuses to start instead.

**Fix:** Canonicalize the parent directory of `--working-archive` and allow the file to be missing. Add an explicit snapshot directory (for example `--snapshot-dir`, or "the working archive's directory") to the allowed-path set. Restate AT-74's host case as "startup fails naming the flag", or make the flag optional and have select fail closed.

---

## Majors

### F-06 — Major — §5.7 Android, DD-39, AT-74, §5.1: phone Scholar writes are required but cannot be built
> "Scholar **local** writes on the phone use a configured on-device working archive and do not depend on tracing consent (DD-39)."

**Problem:**
- The FFI (`open`, `render_manifest`, `render_warehouse_frame`) has no wiki rendering, candidate list, select, download, or write function.
- The phone has no network, so neither "download" nor capturing a remote `http(s)` body (§5.6) can happen there. It is unclear what a phone "select" captures.
- `actor_hash` needs `--researcher-id`, and a phone has no flags, so the phone's researcher identity is undefined.
- "Configured on-device working archive" has no configuration mechanism. Its relation to SAF (content URIs, not paths), to `try_lock_exclusive`, and to "beside the working archive" snapshot files is also undefined.

**Fix:** Either declare the phone Scholar browser out of v1 (and drop the phone half of DD-39/AT-74), or add the FFI functions, the identity source, the archive location (app-private storage vs SAF), and the phone locking and snapshot rules, each with ATs.

### F-07 — Major — DD-32 vs AT-28, §1 "humans make the judgments": what counts as a "human select" is ambiguous
> AT-28: "An API call that tries to write include/exclude/score without a UI-token human select or download is rejected" vs DD-32: "no 'trusted-gesture' heuristic beyond the token kind"

**Problem:** AT-28 suggests there is a difference between a "UI-token API call" and a "UI-token human select". DD-32 says token kind is the only signal. Under DD-32, any script holding the UI token, including CDP `Input` on a UI session, can create inclusion records. That weakens the "humans make the judgments" rule, and the spec does not acknowledge the gap.
**Fix:** State plainly that "UI token = human by policy; custody of the UI token file is the human control". Reword AT-28 to "agent-token or unauthenticated", or define a real human-gesture mechanism.

### F-08 — Major — §5.2 CDP capability, DD-32, DD-33, §5.5 privacy: agent sessions can change consent and observe browsing
> "Agent tokens are read-only: they cannot invoke inclusion, select, download, snapshot, or any other working-archive write." / "Network (observation)"

**Problem:** Changing the consent state file is not a working-archive write, so nothing stops an agent token from turning tracing **on**. Agent sessions can also use `Network`/`Page`/`DOM` observation to see every URL and paper the researcher opens, Pass 1 included, whether or not consent is on. That defeats "Pass 1 skim is never recorded" for any agent that logs what it sees.
**Fix:** Make consent toggling UI-token only, with an AT. Decide whether agent sessions may attach to the human's pages at all, or only to their own pages, and add an AT that an agent cannot observe a UI session's navigation.

### F-09 — Major — §5.5 "Tracing consent model", DD-33, AT-04, AT-58: consent state semantics are underspecified
> "`--consent-state-file` (required for a host that can talk to an LRS)" / AT-58 "consent file goes `on` → `off`"

**Problem:**
- "Can talk to an LRS" is undefined: is it whenever an LRS URL is configured, or always? §5.5 says consent on without a URL is an error case, which implies a consent file can exist with no URL.
- Nothing says who writes the file (the host on a UI toggle, or an outside editor), or whether the host watches it for outside changes, which is how AT-58 phrases revocation.
- The state at startup when the file is missing or malformed is not given. "No invented defaults" suggests a startup error, but the spec does not say so.
- There is no rule for concurrent writes or atomic replacement.

**Fix:** Define it: the file is required whenever `--lrs-url` is set; the host is the only writer, using atomic rename; a missing or malformed file is a startup error, or `off` if Ashley prefers; outside edits are or are not honored. Each rule gets an AT.

### F-10 — Major — §5.5 DD-34, AT-59: retry-queue persistence is unspecified
> "the statement is retried from a host-local retry queue that exists only while consent is on"

**Problem:** If the queue is in memory, a host restart silently drops statements, which contradicts "the statement is not silently skipped". If it is on disk, it is an artifact that holds paper ids, and its location, retention, and deletion on revoke need definitions (AT-58 "drops any retry queue"). No retry limit or backoff is given either.
**Fix:** Pick one. For on-disk, give the location under a defined data dir, delete on revoke, and set a max age and backoff. For in-memory, state that a restart loses queued statements and show it to the user. Add an AT for restart behavior.

### F-11 — Major — §5.5, §5.6, DD-34, AT-26/AT-60, Q-07: paper identity, the candidate source, and the inclusion record layout are undefined
> "idempotency key: working archive + paper identity + verb" / "a Scholar inclusion record for that paper id"

**Problem:**
- "Paper identity" is never defined. It could be `SRC-xxxx`, a URL, a DOI, or a content hash.
- The source of the Pass 1 candidate list is not stated. It could be a corpus archive, the wiki tree, or `triage_wave.py` output.
- The inclusion record is presumably a `HumanLrsRecord` (tile 10), but the spec never says so outright, and `object_hash` is not mapped to a paper.
- Q-07 allows different verbs for select and download, so with the stated key, select followed by download makes two inclusion records. AT-60 only covers "select twice".
- The xAPI statement's object id (what is sent to the LRS when consent is on) is undefined.

**Fix:** Define the paper identity and its derivation, the candidate source, the exact tile and field mapping for an inclusion (`verb_id`, `object_hash` = H(paper identity), `timestamp_sec`), and the xAPI object. Add an AT for select-then-download.

### F-12 — Major — §5.6 "Snapshots", AT-31/AT-32: snapshot encoding is undefined for real payloads
> "containing the captured bytes in existing core BLOB (68) + PROVENANCE (72) tiles … the payload hash must equal SHA-256 of the captured bytes"

**Problem:** Hyper-tiles have a 128 KiB stride, and most PDFs and HTML responses are larger. The spec does not say how bytes are split across BLOB tiles, what "payload hash" means once there are several tiles, or how a reader reassembles them. Nothing links the inclusion record in the working archive to its snapshot file. "Points at the existing object" for on-disk sources is also undefined (§5.6 "Captured bytes"). It is also unclear whether `seal_archive_with_manifest` adds a tile-78 manifest that counts as "other metadata" (AT-32).
**Fix:** Specify the chunking (ordered BLOB tiles plus a whole-content SHA-256 in PROVENANCE), the reference from inclusion record to snapshot (by content hash = file name), the reference for on-disk objects, and whether a manifest is written. Add an AT with a payload larger than 128 KiB.

### F-13 — Major — AT-24/AT-25/AT-61, §5.5 "Host logs and caches": "host log and data directories" is undefined, and caches conflict with byte-identity
> AT-24 "leaves … the host log and data directories byte-identical" vs §5.5 "Caches used to render wiki or pages are ephemeral and are not Pass 1 records" and §5.2 "per-page temporary directory"

**Problem:** No flag or rule defines the host log or data directory, so the test cannot locate them. If render caches or per-page temp dirs live there, byte-identity fails as soon as a candidate page is rendered. If they live elsewhere, paper URLs could persist there without breaking the test. CDP traces and HTTP caches are named but never located.
**Fix:** Add `--log-dir` / `--data-dir` (required, no default), require caches and page temp dirs to live in an ephemeral location (tmpfs or a deleted-on-exit dir), and extend AT-24 to cover that location plus "no paper id anywhere under the data dir".

### F-14 — Major — §5.4 items 3 and 6, AT-23, §4.2: Edge consent behavior with no consent record, and identity shown regardless of consent
> "All kite views apply research-purpose consent filtering." / header sheet shows "device id, hardware model, firmware version"

**Problem:** Following the hyper-tile rule ("records without active `purpose=research` are withheld"), an Edge file with no type-71 record would withhold every record, so the event log would be empty. The spec does not say whether that is intended. The derived header sheet and type-64 identity floors show `DeviceIdentityRecord` fields unconditionally, which conflicts with "all kite views" filtering. AT-23 only covers the case where a record exists and denies research use.
**Fix:** State the no-type-71 behavior (withhold all, or allow, as a DD). State whether the header sheet and identity fields are exempt from consent filtering. Add ATs for both.

### F-15 — Major — §4.2 "v1 has no unfiltered or staff view", §5.6: the researcher's own working archive may be unviewable
**Problem:** The working archive and snapshots, presumably, hold no core `ConsentRecord` with `purpose=research`. Under the "all kite views filter" rule, opening them in kite withholds every record. The researcher could not view their own inclusion records, despite the standing rule that their own records do not depend on consent. The spec does not address this.
**Fix:** Either have kite write a research-purpose consent marker when it creates the working archive, or exempt pack-owned working archives and snapshots from research filtering, and add an AT.

### F-16 — Major — §11 order of work, §5.1 test locations: sequencing problems
> Step 1: "Land the fail-first acceptance tests in s3a-storage and s3a-reader" / Step 5 "Write path … snapshots" / Step 6 "Wasmtime page-process path for `http(s)`"

**Problem:**
- Snapshots of `http(s)` response bodies (§5.6, AT-31/AT-48) need the page/proxy path from step 6, but they are scheduled in step 5.
- s3a-reader tests that compile against `s3a-pack-kite` cannot build while s3a-reader still pins `ff89564`, which has no pack. Landing them in step 1 breaks the reader build unless they are feature-gated or ignored.
- "Land" conflicts with "Do not push … until she says so" unless it means local-only.
- CI for Android/iOS targets (NDK, Apple toolchain) is assumed but never required.

**Fix:** Move the download/remote-capture parts of step 5 after step 6, or split them out. Say how reader tests are gated until the pin moves. Define "land" as local branches. Add a CI-toolchain requirement.

### F-17 — Major — §1, §2 "Host", §5.2 target list: deployment targets assume durable local disk and one researcher per host
> "a host that runs on an Aura Rack, the LRS VM, or Cloud Run"

**Problem:** The host takes one `--researcher-id`, one `--working-archive`, and a local consent file, and depends on `try_lock_exclusive` file locks. That implicitly means one researcher per host process and a durable local filesystem. The spec never states those assumptions, and they do not obviously hold for a scaled-out or stateless deployment like the listed Cloud Run target. "Listens for users" (plural) also clashes with the single-researcher model.
**Fix:** State "one host process per researcher, durable local disk required". Say how Cloud Run meets that (mounted persistent volume, single instance) or drop Cloud Run from v1.

### F-18 — Major — §5.2 "Page sandbox and egress", DD-31, AT-54/AT-55: the sandbox and egress rules are not concrete enough to build or test fully
**Problem:**
- "Sandboxed OS process" names no mechanism (seccomp, namespaces, macOS sandbox, Windows job objects) and no target OS list.
- Egress checks do not say they run on the resolved IP at connect time, so DNS rebinding is open. They also say nothing about redirects to denied hosts, IPv6 loopback (`::1`), IPv4-mapped IPv6, `0.0.0.0`, or CGNAT 100.64/10.
- It is unclear whether plain `http://` is allowed. The text says "Public HTTPS … may proceed", while CDP allows `http(s)`.
- No resource limits are set (CPU, memory, wall time per page).

**Fix:** Name the sandbox per supported OS. Require denial on the resolved address at connect time and on every redirect hop, and enumerate the address classes. Decide on plain http. Add per-page limits and ATs for a redirect to 169.254.169.254 and a DNS name resolving to 127.0.0.1.

### F-19 — Major — §5.2 TLS vs `--behind-proxy`: the TLS guarantee does not hold behind a proxy
> "`--behind-proxy` flag meaning a reverse proxy in front of the process terminates TLS" … "There is no plaintext bearer token on a lab LAN in a supported bind."

**Problem:** With `--bind 0.0.0.0 --behind-proxy`, the hop from proxy to host carries the bearer token in plaintext over a non-loopback interface. The guarantee cannot be enforced or tested, and the host cannot check that a proxy exists.
**Fix:** Allow `--behind-proxy` only with a loopback or Unix-socket bind, or soften the claim and document the risk. Add an AT.

### F-20 — Major — §1 "s3a-kite **is** the S3A viewer" vs §5.2 required roots: opening an arbitrary file is impossible
**Problem:** The host must have `--wiki-root`, `--corpus-root`, and `--working-archive`, and opens files only inside those roots. Viewing a `.s3a` or Edge file from somewhere else (a device dump on a hub, a file on a PC) means configuring Scholar paths and copying the file under a root. That fits poorly with replacing the general S3A viewer, and no viewer-only mode exists. Separately, the spec never says how a host user picks a file to open (see F-01).
**Fix:** Add a viewer-only mode with a `--view-root` and no Scholar flags, or state that the desktop viewer always needs Scholar config and give the reason.

### F-21 — Major — §5.6 / DD-34 failure ordering, AT-46: what happens after a partial failure is unspecified
> "(1) write the local inclusion record, (2) write the snapshot if bytes were captured, (3) if consent is on, emit or queue the LRS statement. A snapshot failure is reported and may be retried"

**Problem:**
- If step 1 fails (lock error, missing `ADDON_EDU`), nothing says whether steps 2 and 3 are skipped. AT-46 implies the LRS is skipped but says nothing about the snapshot file.
- "Snapshot may be retried" contradicts idempotency, because re-selecting the same paper "does not create a second … snapshot", and nothing says the original attempt failed.
- No user-facing retry mechanism is defined.
- On download, nothing says whether the file is delivered when the local record fails.

**Fix:** Step 1 failure aborts steps 2 and 3 (state whether the download is still delivered). Idempotency applies only to successful artifacts. Specify the retry trigger. Add ATs.

### F-22 — Major — §5.3 item 6, §5.4 item 2, AT-35: rules for hyper-tile seal failure and Ed25519 failure are incomplete
**Problem:** The rule for which floors stay valid after a chain break is Edge-only. For a sealed hyper-tile file whose seal trailer fails `verify_sealed_archive`, nothing says which floors remain valid. Edge Ed25519 signatures live in type 70, so if a signature covers the chain, it is unclear what "the failing tile" in AT-35 is.
**Fix:** Define validity rules for floors after a hyper-tile seal failure, and define what an Ed25519 failure invalidates (the signed range vs the type-70 tile only).

---

## Minors

### F-23 — Minor — §5.8 Scale: no device-pixel-ratio or scale parameter
> "at scale 1.0, 1 CSS px = 1 pixmap px"

At 1 px per CSS px on a high-density phone, 32-px bays and text are tiny, but the API has no scale or DPR input and only scale 1.0 is defined. **Fix:** Add a `scale` argument. Keep the ATs at 1.0.

### F-24 — Minor — AT-19: the color assertions have no values
"Severity 0 is green, 1 yellow, 2 and 3 red (pixel samples)" gives no RGB values or tolerance, so the test is subjective. **Fix:** Give sRGB values, plus a ΔE or channel tolerance and the sample location.

### F-25 — Minor — AT-38 / AT-14: weak text assertions
"the first frame's text layer includes `4096`" also passes when `4096` appears in any other field. **Fix:** Assert on the labeled run, for example a `tile_stride` semantics node with value 4096.

### F-26 — Minor — AT-25, AT-28, AT-29, AT-57: negative tests pass vacuously
"Writes nothing" and "rejected" pass when the feature is missing altogether. **Fix:** Pair each with a positive precondition in the same test, for example "the same session's UI-token select succeeds", and for AT-29 assert that the timer is shown (DD-08 has no AT).

### F-27 — Minor — AT-12: "no script runs" is unobservable
**Fix:** Use a canary payload, for example a script that would set a sentinel the test can read, and assert the sentinel is absent.

### F-28 — Minor — AT-13 / §5.8 `instant` semantics in a pull API
"With `instant`, only the t = 0.45 end frame is produced": the API pulls frames by `t`, so the return value for `render_warehouse_frame(t=0, instant=true)` is undefined. **Fix:** "With `instant`, every t returns the end frame."

### F-29 — Minor — AT-42 vs §5.8: the signature has no error type
AT-42 expects `render_manifest` to "return an error", but the signature returns `RenderFrame`. **Fix:** Use `Result<RenderFrame, KiteError>` and list the error variants.

### F-30 — Minor — AT-37: "new" has no baseline
"No new `TileType` constant is added" needs a baseline list to compare against. **Fix:** Check against a committed list of the constants present at `b7bdcda`.

### F-31 — Minor — §2 glossary "Hyper-tile archive": strides that are neither Edge nor 128 KiB
The glossary defines only 128 KiB and 4/8 KiB strides. A non-Edge file with any other `tile_stride` has no specified behavior. **Fix:** Quarantine it as an "unsupported stride" and add an AT.

### F-32 — Minor — §5.3 item 7: the mmap fault requirement has no approach or platforms
"The process must not abort on that fault": SIGBUS on a truncated mapping is hard to recover from safely. The spec names no approach (copy into memory, signal handling, file lease) and no OS list. **Fix:** Name the mechanism, or require that host opens copy into memory (`from_bytes`). Add an AT that truncates the file mid-render.

### F-33 — Minor — §5.2 token rules: ambiguous edge cases
- "32 bytes of secret material" does not say whether trailing newlines or whitespace are trimmed or how the bytes are counted (raw vs hex).
- The behavior when the UI and agent token files hold the same value is undefined.
- The `0600` check is Unix-only, and Windows PC support is unspecified.

**Fix:** Define trimming and counting. Make identical UI and agent tokens a startup error. State the supported host OSes.

### F-34 — Minor — §5.2 Health: "success" is undefined
**Fix:** Specify 200 and the body. Also state whether `/health` reveals any version or configuration.

### F-35 — Minor — AT-33: depends on the live internet
Using `https://example.com/` makes CI depend on outside network access and on that site's content. **Fix:** Use a local HTTPS fixture server added to the egress allowlist.

### F-36 — Minor — AT-74 location vs its phone half
§5.1 places AT-74 in s3a-storage, but its phone case ("no on-device working archive") needs s3a-reader. **Fix:** Split it into AT-74a (host) and AT-74b (reader).

### F-37 — Minor — §5.6 / AT-32: "No other metadata fields" vs the existing PROVENANCE layout
If core PROVENANCE (72) has fixed fields beyond those three, AT-32 can only pass by leaving them zeroed or empty, and the spec should say so. The spec also says "capture time as RFC3339 seconds UTC" without separating stored form (epoch?) from displayed form. **Fix:** State the field mapping onto the existing record, including unused fields, and the stored representation.

### F-38 — Minor — §6.1 item 6 vs the product name
The spec says the license grants "no use of … Kitesurf marks except as needed to describe origin", yet names the product and fork "s3a-kite" / "s3a-Kite". **Fix:** Note the naming risk as a DD or Q, or rename.

### F-39 — Minor — §5.2 item 4 / DD-26: "Wasmtime 36" is a fixed major version
Pinning a sandbox runtime major version with no update policy conflicts with security patching. **Fix:** Say "Wasmtime ≥ 36, tracked for security releases" or add an update policy.

### F-40 — Minor — AT-04: the list of required flags is incomplete
AT-04 says "plus the required root / working-archive / consent flags" but leaves out `--researcher-id`, whose absence makes writes fail closed. It does not say whether that flag is required at startup. **Fix:** List the exact required flag set in one place and make AT-04 use it.

### F-41 — Minor — §5.5 Download: no destination
"Still delivers the file to the researcher" names no destination path (inside a root?), file naming, or overwrite rule on a host. **Fix:** Specify `--download-dir` (or a path under the corpus root) and the collision behavior.

### F-42 — Minor — §5.8 / AT-16: the tolerance is loose for the claim it tests
Allowing ±1 px on each side of both bays lets the measured area ratio fall anywhere between about 1.80 and 2.23 (4 KiB side 31–33 px, 8 KiB side 44.25–46.25 px). That barely tells a 2× ratio apart from 1.8× or 2.2×. **Fix:** Assert the integer-rounded side lengths (32 and 45) exactly, or tighten the ratio tolerance.
