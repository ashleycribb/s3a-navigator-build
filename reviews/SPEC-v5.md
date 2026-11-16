# s3a-kite: S3A Browser Pack and Viewer

*   **Author:** Grok Bot (elephant) for Ashley Cribb
*   **Status:** v5 — spec only. Ashley has forked Kitesurf as [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) (`main` at `235111c6fe10f2d63f3476bf5e3d19b4c7089415`). This document does not authorize implementing the pack, or pushing to that fork or to s3a-storage / s3a-reader, until Ashley says so.
*   **Date:** 2026-11-16 (America/New_York)
*   **Ashley direction (chat, 2026-11-16):** unchanged from v4: s3a-kite is the S3A viewer (hub, PC, or phone, not the microcontroller); first use is Scholar's 3-pass wiki browser; local inclusion records must not depend on tracing consent; xAPI to the LRS is consent-only; no money or crypto in the S3A core. Independent review rounds continue before any push.
*   **Grounded commits (read, not modified):** same as v3 — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`).
*   **Reviews:** round 1–3 dispositions under `/workspace/s3a-kite/reviews/`; round 4, `/workspace/goldfish/goldfish_s3a-kite_round4.md` (40 findings, F-01–F-40), dispositions in `/workspace/s3a-kite/reviews/round4-disposition.md`.
*   **Self-contained:** this document is the whole v5 contract. Earlier spec versions are historical only. No rule is left as "unchanged from v2/v3/v4."

This is a tests-first pack contract. Fail-first tests must fail on today's trees; guard tests are tagged in §7 and are not claimed to fail first. No stubs, fallbacks, or invented runtime defaults, except the query defaults listed as part of the contract in §5.2.1 and §5.8 (DD-69). Anything that needs Ashley's judgment is a **DD** with a draft value, or an open question. Existing DD numbers keep their topic; meaning changes are called out in §8.

v5 keeps v3's v1 scope cuts: **no CDP and no agent tokens**, **no page JavaScript / Wasmtime**, **no Scholar writes on the phone**, and **Cloud Run is not a v1 target**. Those return only if Ashley accepts a later DD.

--------------------------------------------------------------------------------

## 1. Overview

`s3a-kite` is an S3A **add-on pack** that forks Kitesurf's Apache-2.0 Rust core (vendored from [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) at the pin above) and replaces Kitesurf's Cloudflare TypeScript host with a native host and a UniFFI library inside the S3A mobile app.

s3a-kite **is** the S3A viewer. It reads `.s3a` and Edge files with `s3a-engine` / `s3a-edge` and paints the manifest or header sheet and the warehouse tour with Kitesurf's Rust layout and CPU rasterizer. Wiki Markdown and local HTML are painted by `render_page`. On a hub or PC with a display, an optional **native window** is a Host HTTP API client: it sends the UI token on every request except the tokenless health case (DD-60). The Host HTTP API in §5.2.1 is the only write path and the testable wire contract. It is not a Chrome DevTools server. Headless hosts (Aura Rack, LRS VM, or a lab PC without a display) run with `--headless` and no window. The phone app, in v1, is a viewer only.

First product use, on the hub or PC: Scholar's wiki browser for Keshav's 3-pass method. Pass 1 is a human skim of about 15 seconds and is never recorded. Selecting or downloading a paper is the inclusion decision and is written to the researcher's working archive even when tracing consent is off. Tracing is consent-only xAPI to `s3a-lrs-server`. AI and the browser only check that things exist and record what the researcher did; humans make the judgments. Custody of the UI token file is the human-control point for every write, including those issued by the native window (DD-32, DD-60).

S3A core stays database-only. No money and no cryptocurrency. Everything lives in the pack.

A v1 **Scholar-mode** host process serves **one researcher**, requires **durable local disk**, and runs on an Aura Rack, the LRS VM, or a lab PC (DD-48, DD-78). Viewer-only mode does not require a data directory. Cloud Run is not a v1 target. The host process itself is specified for Unix (Linux) only in v1 (DD-73).

--------------------------------------------------------------------------------

## 2. Glossary

| Term | Meaning in this spec |
| --- | --- |
| Pack | A domain crate that extends S3A without changing `s3a-core`. See `docs/pack-contract.md`. |
| Kitesurf core | Rust crates `packages/core` (`kitesurf-core`), `packages/renderer-core`, and `packages/kitesurf-cli`, kept and forked. |
| Cloudflare layer | TypeScript `packages/engine` plus `wrangler.jsonc`, the Worker Loader binding `LOADER`, and Dynamic Workers. Replaced, not required at runtime. Upstream CDP also lives in this layer and is **not** reimplemented in v1 (DD-41). |
| Host | The `s3a-kite` process for one researcher on a hub, Aura Rack, LRS VM, or lab PC. |
| Scholar mode | Host started with the Scholar flag set in §5.2.2. Enables the 3-pass wiki browser and writes. |
| Viewer-only mode | Host started with `--viewer` and `--view-root`. Opens files under that root. No Scholar writes, no LRS, no `--data-dir`. |
| Mobile library | The same Rust renderer, exposed through UniFFI from crate `s3a-pack-kite`, linked into the S3A Android (and later iOS) app. v1 phone builds are viewers only. |
| Hyper-tile archive | A standard S3A file whose `tile_stride` is 128 KiB (the default when the header does not declare an Edge stride). |
| Edge archive | An S3A file with `FLAG_PROFILE_EDGE` and a consistent `tile_stride` of 4096 or 8192. Still a valid S3A file (EDGE profile §1.2). A file whose stride is neither 4096, 8192, nor 128 KiB, or whose Edge flags disagree with the stride, is opened as inconsistent / unsupported (AT-66, AT-97), not as a hyper-tile. |
| Micro-tile | One 4,096-byte or 8,192-byte Edge tile (`MICRO_TILE_4KB_SIZE` / `MICRO_TILE_8KB_SIZE`). |
| Lobby | `TourLobby` in `crates/s3a-cli/src/tour.rs`. |
| Floor | One tile in the tour directory (`TourFloorItem`). |
| Warehouse tour | Ashley's name for the animated walk of those floors (DD-04). |
| Stored manifest | The UTF-8 payload of `TileType::MANIFEST` (78). |
| Derived header sheet | For Edge files (no tile 78 in the Edge profile), a labeled plain-text readout of present `FileHeader` and `DeviceIdentityRecord` fields, as listed in §5.4. |
| Pass 1 | The researcher's own skim, about 15 seconds per paper. Never recorded. The 15-second timer is a client-side UI concern; the host has no write-on-timer endpoint. |
| Inclusion | The researcher selects or downloads a paper for Pass 2/3. Written to the working archive regardless of tracing consent. The inclusion record is a `HumanLrsRecord` (Scholar tile 10). |
| Paper identity | The stable string used as the idempotency key's paper field: the `SRC-NNNN` stem of a `papers/SRC-*.md` file, or the decimal `paper_id` of a live (non-tombstone) `AcademicPaperRecord` (DD-61). There is no join of an SRC stem to a `paper_id`. A source URL is a property of a candidate, not an identity. |
| Source URL | An `http` or `https` URL taken from the optional dossier YAML key `source_url` (DD-59). `AcademicPaperRecord` stores only hashes and has no URL string. |
| Tracing consent | Permission to emit xAPI to the LRS. Gates only LRS traffic. |
| Working archive | A per-researcher live `.s3a` file that receives inclusion records. Not the corpus. |
| Text layer | Structured `text_runs` and `semantics` returned with every frame, per §5.8. Not OCR. |
| Host HTTP API | The v1 wire contract in §5.2.1. Not CDP. The native window is a client of this API. |
| Inclusion index | The Scholar-mode file `--data-dir/inclusion-index.jsonl`. It is local research data, like the working archive: it stores paper identities, the working-archive `file_uuid`, and a nullable snapshot SHA-256 (DD-61). |
| Tokenless health case | `GET /health` on a loopback or Unix-socket bind when `--behind-proxy` is not set. That is the only unauthenticated success case. |

--------------------------------------------------------------------------------

## 3. Non-goals

### 3.1 This document does not authorize

*   Implementing the pack in this revision.
*   Pushing to [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite), s3a-storage, or s3a-reader. "Land tests" in §11 means local branches only.

### 3.2 v1 product non-goals

*   Changing `s3a-core` record layouts or adding kite tile IDs to core (DD-15).
*   Running s3a-kite, Kitesurf, or a GPU renderer on the microcontroller.
*   BLE / USB / SoftAP transport (Edge Phase 2).
*   Hosting or embedding the existing `s3a-wasm-snapshot` / harness WebAssembly viewer.
*   Rasterizing PDF input (DD-14).
*   Money, cryptocurrency, or writing Bitcoin / OpenTimestamps anchors. Reading an existing on-device SHA-256 seal chain is required for Edge verification.
*   Cloudflare Workers Paid, Wrangler, or Dynamic Workers as a runtime dependency.
*   **A CDP server, agent tokens, or any reimplementation of `packages/engine/src/cdp/` (DD-41).**
*   **Page JavaScript, Boa, Wasmtime, or a wasm32 page-isolate ABI (DD-26 as amended, DD-44).**
*   **Scholar writes, candidate lists, select, or download on the phone (DD-39 as amended, DD-45).**
*   **Cloud Run or any other stateless / multi-tenant host as a v1 target (DD-48).**
*   **A general `POST /v1/open` by URL, live remote browsing, or following `http(s)` links from a page view (DD-58, DD-59).** Remote bytes are retrieved only by select/download of a candidate that has a `source_url`.
*   **A Windows host binary (DD-73).** v1 `s3a-kite` is specified for Unix (Linux). Android and later iOS remain the mobile targets.

v1 **does** distribute a Kitesurf derivative, under §6, when Ashley authorizes vendoring the Rust core into s3a-storage and shipping the Android (and later iOS) app.

--------------------------------------------------------------------------------

## 4. Grounded facts

Claims about Kitesurf, S3A, Edge, and the phone reader are taken from the trees named above. File paths below are in those trees.

### 4.1 Kitesurf

*   Experimental browser engine that parses HTML, applies CSS, runs page JavaScript, and renders screenshots without Chrome (`README.md`). Licensed Apache-2.0 (`LICENSE`); bundled fonts and crates keep their licenses (`NOTICE`).
*   Cloudflare deployment requires Dynamic Workers (Workers Paid) and `wrangler dev` locally (`README.md` "IMPORTANT" block). `packages/engine/wrangler.jsonc` declares `"worker_loaders": [{ "binding": "LOADER" }]`. Page isolates are started with `env.LOADER.load({...})` in `packages/engine/src/eval/page-script.ts` at line 368 in `235111c` (line 376 in `4a29038`).
*   Commit `235111c` (`engine: slim what each page isolate receives at startup`) changes only TypeScript. Each PageScript and renderer isolate previously received the 27 MB core Wasm as bytes and recompiled it on every navigation. The parent now passes its already compiled `WebAssembly.Module`, and the generated startup-input module embeds `hasHtml` and `htmlLength` instead of the navigated HTML string. Files: `packages/engine/src/eval/page-renderer.ts`, `page-script.ts`, two engine tests, `packages/page-script/src/__ks_startup_probe_input.ts`, and `startup-probe.ts`. This does not change the Rust core and does not provide a native-host page-JS runtime.
*   The engine has no authentication of its own. A person who has the URL can open DevTools, connect Puppeteer, and send outbound requests (`README.md`, Cloudflare deployment warning).
*   Stack, from `README.md` "Technology stack": Blitz (HTML/DOM/layout/paint, MIT OR Apache-2.0); Stylo (CSS, **MPL-2.0**); Boa (page JavaScript inside WebAssembly, Unlicense OR MIT); Linebender libraries (Parley, Vello, Kurbo, Peniko, Apache-2.0 OR MIT); Krilla (PDF output, MIT OR Apache-2.0).
*   `packages/core/Cargo.toml` describes `kitesurf-core` as "Wasm core for kitesurf: DOM data plane + Stylo + Blitz render." `crate-type` is `["cdylib", "rlib"]`. Feature `native-host` is declared and is used to drop `wasm_bindgen` exports (for example `packages/core/src/paint/scene_replay.rs`, `tty/grid.rs`).
*   Page JavaScript evaluation lives in `packages/core/src/js.rs` and `packages/core/src/js/boa_eval.rs`, both gated `#[cfg(target_arch = "wasm32")]`. There is no native-host Boa entry point in the tree today. `packages/core/Cargo.toml` notes that Boa pulls `getrandom` 0.2, which on `wasm32-unknown-unknown` needs the `js` feature.
*   **Paint backend is CPU, not GPU.** Workspace pins `vello_cpu` 0.3 with features `std`, `text`, `u8_pipeline`, `poly-pnorm`, and `anyrender_vello_cpu` 0.18, both `default-features = false` (`Cargo.toml`). Screenshots use `anyrender_vello_cpu::VelloCpuImageRenderer` (`packages/core/src/paint/screenshot.rs`, `scene_replay.rs`). There is no `wgpu`, Vulkan, Metal, or GLES dependency in those manifests. Raster output is a CPU `vello_cpu::Pixmap` / RGBA buffer, then PNG or JPEG.
*   **Fonts are bundled, not system fonts.** `make_empty_font_context` sets `CollectionOptions { shared: true, system_fonts: false }` (`packages/core/src/resources/fonts.rs`). UA fonts live under `packages/engine/public/fonts/` (the directory is about 19 MB, 55 files, SIL OFL 1.1 per `NOTICE`). Production registration is `register_ua_fonts` from host-supplied bytes; tests `include_bytes!` Noto Sans, Italic, Serif, Mono, and Symbols. Noto Color Emoji (about 1.9 MB) and Noto Sans SC (about 5.9 MB) exist as separate assets. Blitz's `system_fonts` feature is deliberately off in `packages/core/Cargo.toml`.
*   `kitesurf-cli` is a terminal client. It downloads a Wasm scene rasterizer and runs it in **Wasmtime** 36 (`packages/kitesurf-cli/Cargo.toml`, `packages/kitesurf-cli/src/wasm.rs` "Calls the scene-host Wasm ABI through Wasmtime"). That is a client playback path, not the Cloudflare host, and it is not a page-JS runtime.
*   Boa is used to run page `Runtime.evaluate` expressions in the core Wasm (`packages/core/Cargo.toml`). Page `WebAssembly.*` is shimmed in `packages/page-script/src/globals.ts`. **s3a-kite does not use Boa or that shim to run an old S3A viewer.** Opening a `.s3a` file does not instantiate the page JavaScript runtime. **Boa is not used in v1** (DD-26 as amended, DD-44): there is no page JS, no Wasmtime, and no Boa isolate on the host or phone.
*   Upstream CDP lives in TypeScript under `packages/engine/src/cdp/` (modules include `page.ts`, `runtime.ts`, `dom.ts`, `network.ts`, `input.ts`, `target.ts`, `debugger.ts`, `accessibility.ts`, `fetch.ts`, and others).

### 4.2 S3A tour and manifest

*   `s3a-cli tour [--json] [--floor N] [--slice] [--cross] [--history] <archive>` and `s3a-cli tour --html <archive> <out.html>` are implemented in `crates/s3a-cli/src/tour.rs`, `tour_html.rs`, and `tour_html_template.html`.
*   `tour_file` builds a `TourReport` with a `TourLobby`. If a floor has `type_id == TileType::MANIFEST` (`s3a-core`: `MANIFEST = 78`), the UTF-8 payload is `lobby.manifest`. Plain-text output prints `--- PLAIN-TEXT MANIFEST ---` then that payload (`print_tour_report`). The test `test_tour_displays_manifest_in_lobby_and_floor` expects `THIS IS AN S3A FILE` and the seal title/creator.
*   Default mode lists the floor directory. `--floor N` is floor detail (schema fields, records, `withheld_count`). Consent, as described in the `tour.rs` header comment: when consent records exist, records without active `purpose=research` are withheld. Separately, `evaluate_consent_summary` returns `"open / unrestricted (no consent records)"` when there are no consent records, no withdrawal marks, and no Edge consent value. Kite follows that no-records-unrestricted rule for both Edge files (no type 71) and hyper-tile files (no core `ConsentRecord`s). When at least one applicable consent record exists, kite withholds records that are not granted for the research purpose. v1 has no unfiltered or staff view mode.
*   `tour --html` embeds the same JSON, escaped so archive text cannot break out of a `<script type="application/json">` block, and renders with `textContent`, never `innerHTML` (`tour_html.rs`).
*   The HTML animation: clicking a tile opens `#card`. The card is first placed on the tile's bounding rect at opacity 0.2 with `transition: none`, then on a double `requestAnimationFrame` the transition is restored (unless the hash has `instant`) and the card moves to `left: 6vw; top: 8vh; width: 88vw; height: 84vh; opacity: 1`. CSS: `#card { transition: all .45s cubic-bezier(.2,.8,.2,1); }` (`tour_html_template.html`).
*   The unsealed fallback in `tour.rs` walks tiles at `FILE_HEADER_SIZE + i * HYPER_TILE_SIZE` (512 + *i* × 128 KiB). It does **not** consult `tile_stride`. Edge 4 KiB / 8 KiB archives are therefore not toured correctly by today's CLI scanner. s3a-kite must use `s3a_engine::MmapReader` (`is_edge_archive()`, `tile_stride()`, `header_extent()`, edge tile getters) instead of copying that loop.
*   `tour.rs` function `build_consent_state` contains a real branch `else if f.type_id == 82` that parses the payload as `EdgeConsentRecord` and may set `edge_research_allowed`. That is not only a comment. In `s3a-core`, 82 is `VCS_RESOLUTION`. Edge consent is `TILE_TYPE_CONSENT = 71` (`s3a-edge/src/header.rs`), with a different record layout than core consent. `s3a-engine/src/display_layouts.rs` warns that hyper-tile and edge archives reuse type numbers (64, 71, 75) and must be selected with `MmapReader::is_edge_archive`. s3a-kite must not copy the type-82 mistake. On a hyper-tile file that contains a type-82 tile, CLI `withheld_count` can be wrong; it is not a safe oracle for AT-11. Floor-list fields used by AT-10 (`index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`) are not produced by that branch.
*   Timestamps in the tour and in sealed manifests are formatted with `humantime::format_rfc3339_seconds` from Unix epoch seconds (`tour.rs` `format_unix_timestamp`; a zero value prints `genesis`). That is UTC (`…Z`), not America/New_York. Kite uses the same function and timezone for header sheets, events, and snapshot capture time.
*   `FileHeader.version` is a `u16`. The writer sets it to 1. `FileHeader::verify` returns `S3AError::UnsupportedVersion` when it is not 1 (`s3a-core`: display text `Unsupported file version: {n}`). Magic that is not `S3A1` is `InvalidMagic` (`Invalid S3A file magic`). On mobile, `s3a-reader-core` surfaces a non-S3A file as `This is not an S3A file`.

### 4.3 Edge profile

From `docs/spec/S3A-EDGE-PROFILE.md` (cited as **EDGE §…** in this document) and `crates/s3a-edge`:

*   Phase 1 writer is `#![no_std]`, heapless, for MCUs with ≤ 32 KiB SRAM. Micro-tiles are exactly 4,096 or 8,192 bytes. Dual-slot header Variant A is 512 bytes; tile *i* is at `512 + i * tile_stride`. Variant B (`FLAG_COMPACT_HEADER`) uses extent `P = max(256, tile_stride)`. Because Edge strides are only 4096 or 8192, *P* is 4096 or 8192, never 256 or 512.
*   Flags: `FLAG_PROFILE_EDGE`, `FLAG_TILE_SIZE_4KB`, `FLAG_TILE_SIZE_8KB`, `FLAG_COMPACT_HEADER`, `FLAG_SEAL_CHAIN_ACTIVE`, `FLAG_ED25519_SIGNED`. Detection: `FLAG_PROFILE_EDGE` **or** `tile_stride` 4096 or 8192. Torn or unsealed micro-tiles are ignored, never treated as committed history.
*   Equipment event log is type 64, `EquipmentEventRecord` (32 bytes) with `event_type` 0x0001–0x0007 and `severity` 0=Info, 1=Warn, 2=Alarm, 3=Emergency (EDGE §5.2). Type 64 may also hold a 96-byte `DeviceIdentityRecord` (distinguish by `payload_len / record_count`). Every Edge archive MUST contain at least one `DeviceIdentityRecord` (EDGE §6.3) — that is a writer requirement.
*   Canonical `event_type` names (EDGE §5.2.2): `0x0001` `EVENT_MACHINE_START`, `0x0002` `EVENT_MACHINE_STOP`, `0x0003` `EVENT_EMERGENCY_STOP`, `0x0004` `EVENT_FAULT_CODE`, `0x0005` `EVENT_MAINTENANCE`, `0x0006` `EVENT_CALIBRATION`, `0x0007` `EVENT_OPERATOR_CHANGE`.
*   On-device trust is a SHA-256 seal chain (`S3AEDGE1` domain separation) and optional Ed25519 signatures in type 70. There is **no** tile 78 manifest in the Edge tile list (EDGE §12.2). A chain break is `S3AError::SealedChainBreak`: any modified, inserted, or reordered tile breaks the chain immediately (EDGE §6.1).
*   Conformance vectors `MICRO-VEC-01` (`micro_valid_4kb`), `MICRO-VEC-02` (torn tile), `MICRO-VEC-03` (signed seal chain) are generated in `crates/s3a-spec-vectors`.
*   EDGE §10 Phone Reader (Android first): verify CRC32C, payload hash, seal chain, Ed25519; store raw `.s3a`; show an event timeline (faults in red, maintenance in yellow, normal operations in green); optional later LRS sync and in-app consent writes. Phase 2 transports are not implemented. DD-07 colors by *severity* instead of that category coloring; §5.4 states the override and adds a non-color category cue.
*   Primary invariant (EDGE §1.2): every Edge file remains a valid S3A file readable by `s3a-engine` without transcoding.

### 4.4 3-pass, LRS, and writes

From `/workspace/3Pass/README.md` (repo [ashleycribb/3Pass](https://github.com/ashleycribb/3Pass)):

*   Pass 1 is the researcher's own skim. The AI only checks whether a record exists.
*   A paper gets a researcher record only when the researcher selects it for Pass 2/3.
*   `triage_wave.py` lists candidates; it does not score, rank, include/exclude, or record anything. Skipped papers are never recorded. The heading is "Pass 1: Your Skim (~15 s per paper)".
*   AI-extracted text is an AI draft and needs researcher judgment.

`s3a-pack-scholar` already defines the Scholar xAPI profile and tiles 10–15 (`HumanLrsRecord` is tile 10, 64 bytes, including `actor_hash`, `verb_id`, `timestamp_sec`, `object_hash`). `s3a-lrs-server` is the LRS (`POST /xapi/s3a/batch`). A unit test documents the client contract: consent off emits zero requests (`crates/s3a-lrs-server/src/main.rs`, `test_batch_sync_consent_off_client_contract`). The s3a-lrs VM is part of Ashley's lab and is outside the storage repo (`docs/design/research-lab-scenario.md`). Verb IRIs used for inclusion must come from `s3a-pack-scholar` `xapi.rs`; this spec does not invent a new IRI (Q-07).

Write APIs already in `s3a-engine`: `TileWriter::create`, `TileWriter::open_append`, `write_hyper_tile` / `write_hyper_tile_ext` / `write_raw_hyper_tile`, `enable_addon`, `try_lock_exclusive`, and `seal_archive` / `seal_archive_with_manifest` / `verify_sealed_archive` / `verify_sealed_bytes`. Opening a sealed archive with `open_append` does not modify it; the first append unseals it by dropping the seal trailer (`unseal_on_write` in `TileWriter`). Durability docs describe S3A as single-writer (`docs/durability.md`). Tiles 10–15 require `ADDON_EDU` (`0x0001`) per `docs/pack-contract.md`. Core `TileType::BLOB` is 68 and `TileType::PROVENANCE` is 72. Scholar pack `create_human_lrs` writes `HumanLrsRecord`s.

### 4.5 S3A mobile app (`s3a-reader`)

[ashleycribb/s3a-reader](https://github.com/ashleycribb/s3a-reader) at `792742c`, with Ashley's 2026-10-27 decisions already recorded (UniFFI + cargo-ndk, Kotlin, Jetpack Compose, minSdk 26, Android first, later iPhone via UniFFI Swift). The Rust crate `s3a-reader-core` opens bytes (`MmapReader::from_bytes`), lists tiles, verifies CRC and signatures via `s3a-engine` (`verify_sealed_bytes` and related APIs in `integrity.rs`), and can leave a quarantined file viewable with a label. The Android shell has no network permission; files come from SAF. Visualization today is a Kotlin "deck plan" (`MapLayout.kt`), not Kitesurf. The published Cargo pin for `s3a-engine` in that tree is still `ff89564`, which is behind the storage commit grounded here.

s3a-kite **replaces that visualization** and becomes the in-app viewer. Verification APIs in `s3a-reader-core` stay and remain the mobile entry to `s3a-engine`. BLE/USB remain out of kite v1.

### 4.6 Pack contract

`docs/pack-contract.md`: browser and WebAssembly integrations stay out of `s3a-core` and `s3a-engine`. Addon bits already allocated: EDU, FINTECH (reserved), ROBOTICS, WEARABLE, GIS, DA, ONTOLOGY. Future packs that need new tile IDs start at 64+ with a new bit; the Clean pack uses existing core tiles and needs no new bit. v1 kite follows the Clean-pack pattern (DD-15, DD-16).

### 4.7 Facts used in v3, v4, and v5 dispositions

*   `s3a-pack-clean` `archive::chunk_tiles` / `unchunk` prefix each BLOB or PROVENANCE tile payload with a 32-byte key, a little-endian `u32` chunk index, and a little-endian `u32` total, then the chunk bytes (`crates/s3a-pack-clean/src/archive.rs`). The core registry describes the same 40-byte field layout for tiles 68 and 72 (`docs/spec/registry.md`). Hyper-tile payload size is `HYPER_TILE_SIZE - HYPER_TILE_HEADER_SIZE` (131,072 − 512).
*   `AcademicPaperRecord.paper_id` is a `u64` described as a hash of DOI / URI. The record has no URL or DOI string field (only `doi_prefix_hash`). `HumanLrsRecord` is 64 bytes: `session_uuid` ([u64; 2]), `timestamp_sec` (u64), `actor_hash` (**u64**), `verb_id` (u32), `decision_score` (f32), `ai_trace_coord`, `object_hash` (**u32**), `flags` (u32).
*   `s3a-pack-scholar` `xapi.rs` defines `hash_iri(&str) -> u64` as FNV-1a 64-bit with offset basis `0xcbf29ce484222325` and prime `0x100000001b3`. Scholar tests often use literal hashes; this function is the pack's existing string-to-`u64` hash. Verb IRIs and extensions `EXT_PAPER_ID`, `EXT_SOURCE_URL`, and `EXT_CONTENT_SHA256` live in the same file.
*   3Pass dossiers at `wiki/papers/SRC-XXXX.md` use YAML front matter whose keys in the grounded tree are `source_id`, `title`, `author`, `year`, `drive_path`, `provenance`, `wiki_category`, `theories` (and occasionally `PRACTICE`). None of the 287 dossiers in that tree has a `url` or `source_url` key. `drive_path` values are Windows Google Drive paths, not host-local paths.
*   `s3a-lrs-server` authenticates with HTTP Basic (`Authorization: Basic …`) from a credentials file of lines `user:password[:scopes[:archive]]`, or `--auth-user` / `--auth-pass`. It exposes `POST /xapi/statements` and requires `X-Experience-API-Version` of `1.0.x` (tests use `1.0.3`). `POST /xapi/s3a/batch` is a signed binary delta sync, not a JSON statement post.
*   Edge type-70 tiles hold one or more `DeviceSealRecord`s (144 bytes): `tile_index`, `generation`, `seal_hash`, `prev_seal_hash`, `signature` (Ed25519 over the 32-byte `seal_hash`), CRC. `MmapReader::edge_seal_records` returns each record with its `container_tile`. `edge_seal_chain` recomputes `compute_seal_hash` per tile in index order and stops at the first CRC-failed tile.
*   `tour.rs` `evaluate_consent_summary` returns status `"open / unrestricted (no consent records)"` when there are no consent records, no withdrawal marks, and no Edge consent value. That is the no-records behavior kite follows for Edge and hyper-tile files (DD-67).
*   `s3a-reader-core/src/verify.rs` defines `VerificationStatus` {`Unverified`, `Verified`, `SelfSigned`, `Unsigned`, `Quarantined`} and `SignerTrust` {`Embedded`, `Pinned`}. `verify(pinned_keys)` treats a non-empty pinned list as the trust anchor: a signer not in the list fails ("signer key not in pinned list"). An empty pin list yields at best `SelfSigned` (a signature against a `DeviceIdentityRecord` key is not independently trusted). s3a-vault `contacts.json` marks a contact `VERIFIED` only after `vault contact add` of the full 32-byte key; unpinned issuers are `UNKNOWN ISSUER`. S3A-EDGE-PROFILE.md §543 refers to a "cached or provisioned public_key."
*   `kitesurf-cli` downloads a Wasm scene rasterizer for playback; that download path is not used in v1 kite (DD-44). `boa_eval.rs` / `js.rs` remain wasm32-only; v1 does not invent a Wasmtime import ABI for them and does not depend on Boa or Wasmtime.

--------------------------------------------------------------------------------

## 5. Architecture

### 5.1 Pack layout, names, and FFI (draft: DD-01)

These names are fixed for tests-first work:

| Thing | Name |
| --- | --- |
| Pack crate | `s3a-pack-kite` (`crates/s3a-pack-kite` in s3a-storage) |
| Host binary | `s3a-kite` |
| UniFFI surface | exported by `s3a-pack-kite` (`cdylib` / `staticlib`) |
| Crate types | `rlib`, `cdylib`, and `staticlib` |
| Error type | `KiteError` (message string + a stable code) |

FFI / pack API (every `render_*` returns `Result<RenderFrame, KiteError>`). `device_pixel_ratio` is a parameter on every `render_*` (DD-69):

| Function | Role |
| --- | --- |
| `open(bytes, name_hint, max_frame_pixels) -> Result<Handle, KiteError>` | Open an archive or document already loaded as bytes. `name_hint` is an optional filename (used only for kind detection, §5.2.5). `max_frame_pixels` is required and is the cap in §5.8. The host and the phone app prefer `open_verified` when a report already exists. |
| `open_verified(bytes, name_hint, max_frame_pixels, report) -> Result<Handle, KiteError>` | Same as `open`, but kite uses the supplied `KiteVerificationReport` for the quarantine banner and for which tiles are valid floors, and does **not** recompute verification (DD-80, AT-137). |
| `open_path(path, max_frame_pixels) -> Result<Handle, KiteError>` | Open a filesystem path the caller has already authorized. Implementation reads the file into memory and uses `from_bytes` (no path mmap in v1, DD-75). Kind detection uses the path's basename as the name hint. The host fills verification from `--pinned-keys-file` (DD-79). |
| `render_manifest(handle, width, height, scroll_y, device_pixel_ratio)` | First view of a `.s3a` / Edge file (manifest, labeled absent/quarantine/unsupported state, or Edge header sheet). `view=banner` is an alias for this function (DD-81). |
| `render_page(handle, width, height, scroll_y, device_pixel_ratio, select_enabled)` | Wiki Markdown, HTML, or `kind: "other"` documents (the `page` view). `select_enabled` is true only when Scholar-mode `/v1/select` is available (false on the phone and in viewer-only mode; DD-91). |
| `proceed_to_warehouse(handle)` | Leave the first `.s3a` / Edge view. No-op (not an error) if already in the warehouse. Error `not_archive` if the handle is a page. |
| `render_warehouse(handle, width, height, scroll_x, scroll_y, device_pixel_ratio)` | Warehouse floor grid at the given scroll offset. |
| `render_floor_transition(handle, floor_index, t_seconds, width, height, instant, device_pixel_ratio)` | Floor-to-detail animation for `floor_index`. |
| `render_floor_detail(handle, floor_index, width, height, scroll_y, device_pixel_ratio)` | Floor N records, including `withheld_count`. `floor_index` is the tile `index` (the same field as AT-10), not an ordinal among only valid floors (DD-82). |
| `hit_test(handle, view, x, y) -> Result<Hit, KiteError>` | Map a point in the last frame of `view` for this handle to a `Hit`. `x` and `y` are CSS pixels (§5.8). |
| `paper_identity(handle) -> Option<String>` | Paper identity when the handle is a paper dossier or a bound paper page, else none. |

`Hit` is one of: `Continue`, `Floor(n)`, `Select { paper_id }`, `Link { href }`, `None`.

*   `Continue` is only defined on `manifest` or `header` first views.
*   `Floor(n)` is only defined on `warehouse`. `n` is the tile `index`. Invalid or omitted tiles are not hit-testable as `Floor(n)` (they may be listed in the banner text only).
*   `Select { paper_id }` is only defined on `view=page` when `select_enabled` was true for the last `render_page` and `paper_identity(handle)` is `Some`. The returned `paper_id` is that identity. A Select hit does **not** write an inclusion record; the host or native window must call `POST /v1/select` with the UI token (DD-58, DD-60). In viewer-only mode and on the phone, `select_enabled` is false and no Select control is painted (AT-144).
*   `Link { href }` is defined on `view=page` for a link box. The pack and the host do **not** fetch or open `href` automatically. If `href` is a relative or absolute filesystem path that canonicalizes inside an allowed root, the UI may then call `POST /v1/open` with that path. `http(s)` and `file://` hrefs are not opened or fetched from the page view in v1.

v1 UniFFI on the phone exports `open`, `open_verified`, `open_path`, the render functions, `hit_test`, and `paper_identity` only. It does not export select, download, skip, consent, or write. The app runs `s3a-reader-core` `verify(pinned_keys)` first and passes the mapped `KiteVerificationReport` to `open_verified`. The pack does not depend on `s3a-reader-core`; `KiteVerificationReport` is a pack type with fields `status` (`verified` / `self_signed` / `unsigned` / `quarantined`), `tiles_crc_failed`, `invalid_tile_indices: Vec<u32>`, and `detail`. The app maps the reader-core report into that type (DD-80).

The pack vendors the Kitesurf Rust core from [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) at `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. The TypeScript engine, Wrangler, Worker Loader, and TypeScript CDP are not runtime dependencies.

`s3a-core` MUST NOT depend on this pack. The pack depends on `s3a-engine`, `s3a-edge`, `s3a-pack-scholar`, and the forked Kitesurf crates.

**Test locations and pins.** Pack tests (AT-01–AT-37, AT-41, AT-42, AT-44–AT-62, AT-64–AT-150 except the reader-only tests below) live in the s3a-storage workspace. AT-38, AT-39, AT-43, and AT-63 live in s3a-reader and are **feature-gated or in a separate test crate that does not build in the default reader CI job** until the reader pin moves off `ff89564` to a git revision of s3a-storage that contains `s3a-pack-kite`. AT-40 lives in s3a-storage as the iOS link gate. Path dependencies are allowed only in local developer checkouts. "Land" means local branches; nothing is pushed until Ashley says so. Android/iOS ATs run only on a job that has the NDK or Apple toolchain; they are skipped, not failed, on a host-only CI job, and the skip is reported.

### 5.2 Native host (hub, Aura Rack, LRS VM, lab PC)

The host process:

1.  Compiles `kitesurf-core` and `renderer-core` with feature `native-host` and links them as native `rlib`s (DD-02). Wasmtime is not used in v1. Boa is not used in v1.
2.  Requires the flags in §5.2.2 for the chosen mode. Missing required flags are an error in every mode. There is no silent port or bind default (DD-03).
3.  Rejects unauthenticated HTTP as in §5.2.1 (DD-11, DD-29, DD-71). There is no CDP listener in v1 (DD-41).
4.  For a candidate **source fetch** (select or download of a paper that has a `source_url`): the host itself performs the HTTP request, applies the egress policy in §5.2.4, and stores the raw response body as in §5.5. It does not start Boa, Wasmtime, or a page-script isolate. There is no `POST /v1/open` by URL.
5.  For `.s3a` and Edge files: the host calls `render_manifest` / warehouse / floor functions and paints `RenderFrame`s. For wiki Markdown and HTML files: the host calls `render_page`. No Boa, no Wasmtime, no page-script, and no subresource fetches for HTML (DD-77).

**Native window (DD-60).** The window is a Host HTTP API client in this process. It attaches the UI token to every request other than loopback `GET /health`. It does not call select/download/skip/consent through the pack FFI (those functions are not in the pack API). `--headless` starts no window. A native window is allowed only when `--bind` is a loopback address or a Unix socket; `--headless` is required for any other bind. If `--headless` is omitted and no display is available, startup fails and names `DISPLAY`. The HTTP listener is always started.

One Scholar-mode host process serves one `--researcher-id` and one working archive. Durable local disk is required in Scholar mode (DD-48, DD-78).

#### 5.2.1 Host HTTP API (DD-42)

All routes below are the complete v1 HTTP surface. Authentication is checked first (DD-71): a missing or invalid token yields **401** on every route other than the **tokenless health case** (`GET /health` on a loopback or Unix-socket bind without `--behind-proxy`). With `--behind-proxy`, `/health` also requires a token (AT-50, AT-87, AT-113). A valid token on an unknown path yields **404**. AT-06 is "401 without a token on every route other than the tokenless health case; 404 with a valid token on a path not in this table." There is no CDP upgrade.

JSON uses UTF-8 and `Content-Type: application/json`. Errors are `{ "error": "<code>", "message": "<text>" }` with the HTTP status in the table below. `<handle>` is an opaque string minted by `/v1/open`.

Contract defaults for `/v1/frame` query parameters (DD-69), used only when the parameter is omitted: `scroll_x=0`, `scroll_y=0`, `instant=false`, `device_pixel_ratio=1.0`. `w` and `h` have no default and are required. `floor` is required for `floor-transition` and `floor-detail`. `t` is required for `floor-transition` when `instant` is false. `w`, `h`, `scroll_x`, `scroll_y`, `x`, and `y` are **CSS pixels**. The response `width` and `height` are pixmap pixels: `ceil(w * device_pixel_ratio)` by `ceil(h * device_pixel_ratio)` (§5.8, DD-90). A request whose pixmap pixel count exceeds `--max-frame-pixels`, or whose `w` or `h` is 0, or whose `device_pixel_ratio` is not a finite number greater than 0, returns `400` `invalid_frame_size` (AT-141).

| Method | Path | Token | Request | Success |
| --- | --- | --- | --- | --- |
| `GET` | `/health` | None in the tokenless health case; UI token otherwise | (none) | `200`, `Content-Type: text/plain`, body exactly `ok\n` |
| `POST` | `/v1/open` | UI | `{ "path": "<path>", "name": "<optional hint>" }` | `200` `{ "handle": "<id>", "kind": "hyper-tile"\|"edge"\|"wiki"\|"html"\|"other"\|"unsupported", "first_view": "manifest"\|"header"\|"page"\|"banner" }` — `name`, if set, is the kind-detection hint; otherwise the path basename is used (§5.2.5) |
| `POST` | `/v1/close` | UI | `{ "handle": "<id>" }` | `200` `{ "ok": true }` — unknown handle is `404` `unknown_handle` |
| `POST` | `/v1/proceed` | UI | `{ "handle": "<id>" }` | `200` `{ "ok": true }` |
| `GET` | `/v1/frame` | UI | Query: `handle`, `view` (`manifest`\|`header`\|`page`\|`banner`\|`warehouse`\|`floor-transition`\|`floor-detail`), `w`, `h`, plus the defaults above, `floor` and `t` when required | `200` `{ "png_base64": "<png>", "width": <pixmap w>, "height": <pixmap h>, "text_runs": [...], "semantics": [...] }`. `view=banner` calls `render_manifest` (DD-81). An unknown `view` is `400` `invalid_view`. |
| `POST` | `/v1/hit-test` | UI | `{ "handle", "view", "x", "y" }` | `200` `{ "hit": "continue"\|"floor"\|"select"\|"link"\|"none", "floor": <n or null>, "paper_id": <string or null>, "href": <string or null> }` — `x`/`y` are CSS pixels; `floor` is a tile index |
| `GET` | `/v1/candidates` | UI | (none) | `200` `{ "papers": [ { "id": "<paper identity>", "title": "<from the file or record, or empty>", "source_url": <string or null>, "on_disk": <bool> } ] }` — Scholar mode only; `409` `viewer_only` in viewer-only mode. The list is recomputed at request time (§5.5). |
| `POST` | `/v1/select` | UI | `{ "paper_id": "<paper identity>" }` | `200` `{ "ok": true, "snapshot": "<absolute path or null>", "fetch_error": <string or null> }` — Scholar mode; `409` `viewer_only` in viewer-only; `404` `unknown_paper` if `paper_id` is not in the candidate set recomputed at this request. An idempotent repeat returns the same `snapshot` path (or null). |
| `POST` | `/v1/download` | UI | `{ "paper_id": "<paper identity>" }` | `200` `{ "ok": true, "path": "<absolute path in --download-dir or null>", "snapshot": "<absolute path or null>", "fetch_error": <string or null>, "download_error": <string or null> }` — same 409/404 as select. `download_error` is `"no_source"` when the candidate has neither a `source_url` nor on-disk bytes (`path` is then null). Bytes for a later download of a previously snapshotted identity are reassembled from that snapshot's BLOB tiles (DD-61). |
| `POST` | `/v1/skip` | UI | `{ "paper_id": "<paper identity>" }` | `200` `{ "ok": true }` — Scholar mode; writes nothing; `404` `unknown_paper` if not in the request-time candidate set; `409` `viewer_only` in viewer-only |
| `GET` | `/v1/consent` | UI | (none) | `200` `{ "state": "on"\|"off", "timestamp": "<RFC3339 seconds UTC or null>" }` — Scholar mode; `409` `viewer_only` in viewer-only. Without `--lrs-url`, this is `200` `{ "state": "off", "timestamp": null }`. |
| `POST` | `/v1/consent` | UI | `{ "state": "on"\|"off" }` | `200` with the same body as GET after the host writes the consent file — Scholar mode; `409` `viewer_only` in viewer-only; `409` `lrs_not_configured` if `state` is `on` and `--lrs-url` is unset, or if `state` is `off` and `--consent-state-file` is not configured |

Any other path, with a valid token, returns `404` `unknown_path` with the error object. `POST /v1/consent` is UI-token only; v1 has no agent token (DD-41).

**Error catalogue (DD-83).** Every error body is `{ "error": "<code>", "message": "<text>" }`.

| HTTP | `error` code | When |
| --- | --- | --- |
| 400 | `malformed_json` | Request body is not the JSON object required by the route. |
| 400 | `missing_param` | A required query field (`w`, `h`, `handle`, `view`, `floor`, `t`) is absent. |
| 400 | `invalid_view` | `view` is not in the enum above (AT-100). |
| 400 | `invalid_frame_size` | `w`, `h`, or `device_pixel_ratio` fails the cap / positivity rule. |
| 400 | `invalid_magic` | The path is under an allowed root but the bytes have S3A magic and a version or header that cannot be opened (also AT-69's `Unsupported file version: {n}` message). |
| 401 | `unauthenticated` | Missing or invalid token, including on unknown paths (auth first). |
| 404 | `unknown_path` | Valid token, path not in the table. |
| 404 | `unknown_handle` | Handle not open (including after idle expiry or close). |
| 404 | `unknown_paper` | `paper_id` is not in the request-time candidate set. |
| 404 | `not_found` | `/v1/open` of a missing path under an allowed root (AT-30). |
| 409 | `viewer_only` | Scholar route on a viewer-only host. |
| 409 | `lrs_not_configured` | Consent `on` without `--lrs-url`, or consent write without a consent file. |
| 409 | `unsupported` | `proceed` (or a warehouse/floor view) on a `kind: "unsupported"` handle. |
| 409 | `not_archive` | `proceed` or a warehouse/floor view on a page / other handle. |
| 409 | `no_frame` | `hit_test` with no prior frame for that handle and view. |
| 409 | `invalid_floor` | `floor` is not the tile index of a valid floor (DD-82). |
| 409 | `addon_missing` | Working archive lacks `ADDON_EDU` (AT-46). |
| 409 | `download_conflict` | A file exists at the download path whose contents do not hash to the name (AT-94). |
| 409 | `path_escape` | A request path, `Link` follow-up, or `drive_path` canonicalizes outside allowed roots (AT-52, AT-53). |
| 409 | `view_mismatch` | `view` is in the enum but is not valid for this handle's state (for example `warehouse` before proceed, or `page` on an archive). |
| 429 | `too_many_handles` | Open would exceed `--max-open-handles`. |
| 500 | `write_failed` | Inclusion or snapshot write failed after the preconditions were checked (AT-89). |
| 500 | `lock_failed` | Exclusive lock on the working archive could not be taken (AT-73, AT-89). |
| 500 | `revoke_delete_failed` | Consent went `off` but `--data-dir/lrs-retry/` could not be deleted (AT-124). |

**Handles (DD-74, DD-92).** `--max-open-handles` is required in both modes and must be an integer ≥ 1. `--handle-idle-secs` is required in both modes and must be an integer ≥ 1. A handle with no `/v1/frame`, `/v1/hit-test`, `/v1/proceed`, or `/v1/close` for that many seconds is closed; the next use is `404` `unknown_handle` (AT-149). A further `/v1/open` that would exceed the cap returns `429` `too_many_handles` and mints no handle. `POST /v1/close` releases a handle. On process exit, all handles are dropped. `/v1/frame` and `/v1/hit-test` on one handle are serialized by a per-handle lock. `hit_test` uses the last `RenderFrame` produced for that handle and `view`; if no such frame exists, it returns `409` `no_frame`.

**Write mutex (DD-84).** Select, download, skip, and consent (the write routes) are serialized by one process-wide mutex, held from the start of the request through index and archive step 1, snapshot rename, download delivery, and any LRS send begun by that request. Two concurrent selects of the same identity therefore produce one tile-10 record and at most one snapshot (AT-126). Snapshot files are written to a temp file in `--snapshot-dir` and published with an atomic rename. The working-archive exclusive lock is held only for the tile-10 write (step 1), not across the HTTP fetch.

Opening the Scholar working-archive path via `/v1/open` while the host holds its exclusive write lock succeeds: the host takes a `from_bytes` copy of the current file bytes (a short shared read under the lock) and opens that copy (DD-78).

`kind: "unsupported"` and `first_view: "banner"` are used for an unsupported stride (AT-97) or an inconsistent Edge header (AT-66). `GET /v1/frame?view=banner` (or `view=manifest`) returns the banner text in the text layer. `proceed` on those handles returns `409` `unsupported`.

#### 5.2.2 Modes, flags, and paths

**Viewer-only mode** (`--viewer`): required flags are `--bind`, `--auth-token-file`, `--view-root`, `--log-dir`, `--max-open-handles`, `--max-frame-pixels`, `--handle-idle-secs`, and either `--headless` or a loopback/Unix `--bind` (window allowed). `--port` is required unless `--bind` starts with `unix:`. The following Scholar-only flags are not accepted and, if passed, are a startup error that names the flag: `--data-dir`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--researcher-id`, `--fetch-timeout-secs`, `--max-fetch-bytes`, `--max-redirects`, `--lrs-url`, `--consent-state-file`, `--lrs-credentials-file`, `--lrs-actor-homepage` (AT-132). Select, download, skip, and consent routes return `409` `viewer_only`. Request paths are opened only under `--view-root`.

**Scholar mode** (no `--viewer`): required flags are `--bind`, `--auth-token-file`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--researcher-id`, `--fetch-timeout-secs`, `--max-fetch-bytes`, `--max-redirects`, `--max-open-handles`, `--max-frame-pixels`, `--handle-idle-secs`. `--port` as above. If `--lrs-url` is set, `--consent-state-file`, `--lrs-credentials-file`, and `--lrs-actor-homepage` are also required. `--view-root` may be added; if set, it is also an allowed open root. `--headless` as above. `--pinned-keys-file` is optional in both modes (DD-79).

`--working-archive` names a file path. At startup the host canonicalizes its **parent directory** (the parent must exist); the file itself may be missing. The first Scholar write creates it with `TileWriter::create` and `enable_addon(ADDON_EDU)` (AT-79). `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--wiki-root`, `--corpus-root`, and `--view-root` must exist as directories at startup (the host does not create them). Each existing directory is canonicalized with `realpath`. A missing or non-canonicalizable required path is a startup error that names the flag.

**Disjoint write and read locations (DD-93, AT-145).** After canonicalization, `--snapshot-dir`, `--download-dir`, `--data-dir`, `--log-dir`, and the working-archive **parent** must be pairwise-disjoint from `--wiki-root`, `--corpus-root`, and `--view-root` (no directory is equal to or a prefix of another in the other set). A violation is a startup error that names both flags. This keeps the corpus read-only and stops the recursive `*.s3a` walk from picking up snapshots.

**Path rule (DD-89).** This rule applies to paths that come from **requests**: `/v1/open` `path`, a `Link` follow-up, a candidate file, and a resolved `drive_path`. Those paths must canonicalize strictly inside an allowed request root, or be exactly the working-archive path (once created). Allowed request roots are `--wiki-root`, `--corpus-root`, `--view-root`, and, in Scholar mode, `--snapshot-dir` and `--download-dir`. `..` segments, symlink escapes, and `file://` URLs outside those directories are `409` `path_escape`. Separately, the host may open its own config and state files: the token file, `--tls-cert`, `--tls-key`, `--lrs-credentials-file`, `--consent-state-file`, `--pinned-keys-file`, `--log-dir` files, and `--data-dir` files (the inclusion index and `lrs-retry/`). Those paths are not required to sit inside a request root.

**Unix-socket bind (DD-72).** `--bind unix:/absolute/path.sock` binds a Unix domain socket at that path. `--port` must not be passed (startup error if it is). The token is still required on every route other than the tokenless health case.

**Health, tokens, TLS, and other secrets (DD-29, DD-52, DD-68, DD-73, DD-85).**

*   `GET /health` success is exactly HTTP 200, `Content-Type: text/plain`, body `ok\n`.
*   The token file must be a single line of at least 32 characters from the set `A–Z a–z 0–9 _ -` (the unpadded base64url alphabet), optionally ending in one `\n` or `\r\n`. The host strips that single trailing newline at load; the secret is the remaining characters. Any other whitespace, any other byte, an empty file, or a secret shorter than 32 characters is a startup error that names the file (AT-45, AT-81). Comparison is constant-time on the UTF-8 secret. The Bearer value must be exactly that secret (no added newline).
*   On Unix, these existing files must be owned by the process's effective user and have mode `0600`: `--auth-token-file`, `--lrs-credentials-file`, `--tls-key`, and `--pinned-keys-file` when that flag is set. `--consent-state-file`, if it exists at startup, must also be `0600` and so owned. A `0644` (or otherwise group/world-readable) file, or a file owned by another user, is a startup error that names the file (AT-117, AT-142). `--tls-cert` may be world-readable. v1 does not specify a Windows host.
*   Files the host **creates** under `--data-dir` (the inclusion index, `lrs-retry/` entries) and the consent file after a host write are created with mode `0600` and owner euid.
*   The host never writes secrets or identifiers to `--log-dir`, stdout, or stderr: UI token bytes, LRS passwords, incoming or outgoing `Authorization` header values (including Basic to the LRS), TLS private keys, paper identities, source URLs, or `--researcher-id` (AT-117, AT-142).
*   HTTPS to `--lrs-url` (and any other non-loopback https fetch) is validated against the **process platform / webpki system root store**. v1 does not pin LRS certificates (a later DD may add pinning). Loopback `http` LRS remains allowed for tests.
*   A non-loopback, non-Unix `--bind` requires `--tls-cert` and `--tls-key`, and requires `--headless`. `--behind-proxy` is accepted **only** with a loopback bind or a Unix-socket bind (AT-87). Non-loopback plus `--behind-proxy` is a startup error. With `--behind-proxy`, every route including `/health` requires the token.

**Logs and ephemeral temp (DD-88).** `--log-dir` receives append-only lines for startup (flag *names* only, never flag values that are paths to secrets or token contents), shutdown, and error messages that obey the no-identifier rule above. There is no access log of request bodies or paper ids. The ephemeral temp directory is created with `mkdtemp` and mode `0700` under `$XDG_RUNTIME_DIR` if that variable names an existing directory, otherwise under `/tmp`, with a name prefix `s3a-kite-`. It is deleted on a clean exit. At startup the host deletes leftover `s3a-kite-*` directories under those locations that are owned by the same euid.

#### 5.2.3 Consent file (DD-50)

`--consent-state-file` is required when `--lrs-url` is set. When `--lrs-url` is absent, tracing is off: if `--consent-state-file` is still passed and the file exists with `on`, that is a **startup error** that names the file (AT-114). A missing file with `--lrs-url` set, or a malformed file, is a startup error (no invented `on` default). `POST /v1/consent` `{ "state": "on" }` without `--lrs-url` returns `409` `lrs_not_configured` and does not write `on`. `POST /v1/consent` `{ "state": "off" }` without a configured `--consent-state-file` returns `409` `lrs_not_configured`. `GET /v1/consent` without `--lrs-url` returns `200` `{ "state": "off", "timestamp": null }`.

The file format is the exact UTF-8 tokens `on` or `off`, a newline, and an RFC3339-seconds-UTC timestamp of the last change.

The host is the only writer. It writes with a temp file plus atomic rename in the same directory, mode `0600`. `POST /v1/consent` is the only supported toggle. Mid-session outside edits are not honored (the host keeps the last state it wrote or read at startup). AT-58 uses `POST /v1/consent`, not an outside editor.

DD-12's earlier "consent on and URL missing, statement fails closed" state is **unreachable** in v5: that combination is a startup error, and the toggle is rejected at runtime.

#### 5.2.4 Egress for host fetches (DD-31, DD-49, DD-64, DD-76, DD-86)

v1 has no page subprocess. Egress rules apply to the host's own select/download fetches of a `source_url`:

*   Only `http` and `https` URLs are fetched. `file://` and any other scheme are rejected and no file is read through the HTTP client (AT-123).
*   The **deny list always applies**, including to allowlisted names and to every redirect hop (DD-64). Denied: any address that is not global unicast, including loopback (IPv4 and `::1`), IPv4-mapped forms of those ranges, `0.0.0.0/8`, `::`, RFC1918, RFC4193, IPv6 site-local (`fec0::/10`), link-local, multicast, broadcast, CGNAT `100.64.0.0/10`, NAT64 / 6to4 translated forms of the same ranges, metadata (`169.254.169.254`, `metadata.google.internal`), and the configured LRS host plus every IP it resolves to (even if that IP would otherwise be allowed). Denial is at **connect time on the resolved address**, and again on every redirect hop, before the request is sent.
*   `--egress-allowlist` is a comma-separated list of exact hostnames or exact IP literals, each optionally followed by `:port`. No wildcards. If the flag is set, the URL's host (and every redirect hop's host) must also appear on the list. The allowlist never overrides the deny list.
*   Loopback fixture servers used by tests are reached only when the binary is built with the Cargo feature `egress_test` and the flag `--egress-test-loopback-port <port>` is set. That flag is a **startup error** in builds without the feature (including release distributions). In an `egress_test` build it excepts only `127.0.0.1` and `::1` at that exact port. Redirects from that fixture to a denied address are still denied (AT-88, AT-109).
*   `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, and `NO_PROXY` environment variables are **ignored for every host fetch**, including to an allowed `egress_test` fixture (AT-122).
*   Follow at most `--max-redirects` redirects (`--max-redirects` is a required integer ≥ 0). Exceeding it is `fetch_error` and no snapshot.
*   Only a final **2xx** response is stored. A 3xx beyond the redirect budget, or a final 4xx/5xx, is `fetch_error`; the error-page body is not snapshotted or downloaded (AT-139).
*   The stored and hashed body is the **payload after `Content-Encoding` is decoded** (`identity`, `gzip`, and `deflate` are supported). Unknown encoding is `fetch_error` and no snapshot. `--max-fetch-bytes` applies to the decoded body (AT-140). "Raw response body" in earlier wording means this decoded payload.
*   Per-fetch wall-time limit is the required `--fetch-timeout-secs` flag (DD-49). Exceeding it or `--max-fetch-bytes` aborts the fetch, writes no snapshot, and returns `fetch_error` (the inclusion record, if already written, is kept). There is no silent default. There is no page-process CPU/memory limit in v1 because there is no page process.

#### 5.2.5 Kind detection and `first_view` (DD-87)

`open`, `open_verified`, `open_path`, and `/v1/open` classify bytes as follows, in this order. The name hint is the optional `/v1/open` `name`, or the path basename.

1.  If the bytes begin with `S3A_MAGIC` (`S3A1`): apply §5.3 / §5.4. A consistent Edge file is `kind: "edge"`, `first_view: "header"`. An inconsistent Edge file, a contradictory size flag on a non-Edge stride (F-40 / AT-146), or a stride that is not 4096, 8192, or 128 KiB is `kind: "unsupported"`, `first_view: "banner"`. A version-1 hyper-tile is `kind: "hyper-tile"`, `first_view: "manifest"`. A non-1 `FileHeader.version` is `400` `invalid_magic` (AT-69).
2.  Else if the name hint ends with `.md` (ASCII, case-insensitive): `kind: "wiki"`, `first_view: "page"`.
3.  Else if the name hint ends with `.html` or `.htm`: `kind: "html"`, `first_view: "page"`.
4.  Else: `kind: "other"`, `first_view: "page"`. `render_page` shows the exact label `no text conversion in archive` and invents no title, abstract, or raster (DD-14, AT-148). This includes PDFs and extensionless download files.

`open(bytes)` without a name hint, and without S3A magic, is therefore `other`. AT-130 covers each row.

--------------------------------------------------------------------------------

### 5.3 Opening a hyper-tile `.s3a` file

1.  Open with `open` / `open_verified` / `open_path`. `open` and `open_verified` take a name hint and the required `max_frame_pixels`. `open_path` reads the file into memory and uses `MmapReader::from_bytes` (no path mmap in v1; DD-75), passing the path basename as the name hint. If it is not an S3A file, fail with engine `Invalid S3A file magic`, or on mobile the existing `This is not an S3A file`. If `FileHeader.version` is not 1, fail with `Unsupported file version: {n}`. If `tile_stride` is set and is neither 4096, 8192, nor 128 KiB, or a 4 KiB / 8 KiB size flag is set on a 128 KiB (non-Edge) stride, `/v1/open` returns 200 with `kind: "unsupported"`, `first_view: "banner"`, and a labeled unsupported or inconsistent-header banner; `proceed` returns `409` `unsupported`; the file is not toured as a hyper-tile (AT-97, AT-146). The FFI `open` of such a file returns `Ok` with that handle (not `Err`).
2.  **First view** is `render_manifest(handle, width, height, scroll_y, device_pixel_ratio)` (also reached as `view=banner` or `view=manifest`). If a committed, non-quarantined tile 78 is present and valid UTF-8, paint that exact stored manifest text (the same bytes as `lobby.manifest`, trimmed only as `print_tour_report` already trims). If several committed valid tile-78 tiles exist, use the first in tile order as the manifest; later tile-78 tiles still appear as ordinary floors. If tile 78 is absent, or every tile 78 is quarantined, paint a labeled "manifest absent" or "manifest quarantined" state and do not invent title/creator text. If tile 78 is present but not valid UTF-8, paint a labeled "manifest not UTF-8" state; do not invent replacement text. Long manifest text wraps to the pixmap width and scrolls with `scroll_y`; it is not silently truncated.
3.  The first view stays until `proceed_to_warehouse(handle)` or a Continue `hit_test` on that view. There is no timer auto-advance. Then `render_warehouse` is used. Floor-list fields (`index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`) are taken from `MmapReader`, not from a full `tour_file` walk. `withheld_count` is computed by kite as in item 4, never by `tour_file`.
4.  Floor N detail is `render_floor_detail`. **`floor_index`, `Floor(n)`, and `?floor=` are the tile `index` of a valid floor**, not a 0-based position in the visible list (DD-82, AT-10). After invalid tiles are omitted from the valid floor list, the same tile index still identifies that floor. A request whose `floor` is omitted (where required) or is not the tile index of a valid floor returns `409` `invalid_floor`. The invalid-list is banner text only and is not hit-testable (AT-138).

    **Hyper-tile research-consent filter (grounded in `s3a-core` `consent.rs` and `s3a-cli` `tour.rs` `build_consent_state` / `is_record_withheld`; DD-67, AT-11, AT-112, AT-135, AT-136).** Evaluation uses core `ConsentRecord` and marks only. The pack **must not** implement the type-82 branch in `tour.rs` (that branch is a known bug and is not a ConsentRecord). Latest-wins is **per `subject_id` only**, and only `ConsentRecord`s with `purpose == Research` enter the map. `is_active` is `withdrawn_at == 0 && (expires_at == 0 || now <= expires_at)` as in `consent.rs`. `scope` and `study_id` exist on the record and are **not** used by this filter.

    *   If the `subject_research_status` map is empty and no withdrawn marks apply, the tour is **unrestricted** and `withheld_count` is 0. This includes a file with no `ConsentRecord`s at all, and a file whose only `ConsentRecord`s are non-research (AT-112, AT-135).
    *   If the map is non-empty, a payload record that **has** a `subject_id` is withheld unless that subject is in the map and the winning research record is `is_active`. A payload record **without** a `subject_id` is not withheld by `ConsentRecord`; it is withheld only by marks (`MARK_CONSENT_WITHDRAWN`, or `MARK_TARGET_ALL_RECORDS` as implemented in `tour.rs`).
    *   AT-136 covers a later research `ConsentRecord` that withdraws an earlier grant for the same `subject_id`: that subject's records are withheld.

    Golden fixtures are the oracle when type 82 is present. The CLI may be used as a cross-check only on hyper-tile fixtures that contain no type-82 tile.
5.  Archive bytes are painted as text, never executed as HTML or JavaScript (same policy as `tour_html.rs` escaping). No JS context is created (observed as in AT-34: `cargo tree` for the host and Android targets contains no `boa_engine` and no `wasmtime`).
6.  Animation is `render_floor_transition` (§5.8). Seal verification uses `s3a-engine` (`verify_sealed_archive` / `verify_sealed_bytes`). Unsealed hyper-tile files may be opened; the first view shows seal status (unsealed) and still shows the manifest or "manifest absent" state. If `verify_sealed_archive` fails on a file that has a seal trailer, the file opens with a persistent quarantine banner; CRC-valid committed tiles may still be listed as floors, but the banner reports the seal failure and no floor is presented as a verified seal (AT-90). The host must not use the CLI's 128 KiB unsealed scanner on any file.
7.  The only open path is `from_bytes` (a snapshot of the file). Path mmap is **not used in v1** (DD-75). There is no extra size cap beyond the engine and the OS; a failed open is an error, not a hang.

### 5.4 Opening an Edge micro-tile file

1.  Detect with `MmapReader::is_edge_archive()` (flag or stride 4096/8192). If `FLAG_PROFILE_EDGE` is set but `tile_stride` is not 4096 or 8192, or a size flag disagrees with `tile_stride` (for example `FLAG_TILE_SIZE_4KB` with stride 8192), or both 4 KiB and 8 KiB size flags are set, do not tour as if the header were consistent: `/v1/open` returns 200 with `kind: "unsupported"`, `first_view: "banner"`, and a quarantine / "inconsistent Edge header" state (AT-66). `proceed` returns `409` `unsupported`. `tile_stride` is not silently fixed. If `FLAG_SEAL_CHAIN_ACTIVE` is clear, the file is an **unsealed Edge** file: open it, show seal status "unsealed" on the header sheet, and do not auto-quarantine solely for that flag (AT-147).
2.  Verification is performed only by `s3a-engine`. On mobile, `s3a-reader-core` may run first; kite consumes a `KiteVerificationReport` (DD-80) and does not compute a second result when that report is supplied, even if a fresh verify would disagree (AT-137). **Invalid tiles** (torn, CRC-failed, chain-broken) are omitted from the valid floor list, counted on the header sheet / banner, and may appear only in a clearly marked invalid list (banner text, not hit-testable). They are never drawn as valid floors. When the seal chain breaks, tiles before the break that already verified remain valid floors; the breaking tile and every subsequent tile are not valid floors. The banner reports the break index. `floor_index` remains the tile `index`.

    **Ed25519 and device public-key provenance (DD-57, DD-79, F-13).** Type-70 tiles contain one or more `DeviceSealRecord`s. Each record's `signature` is verified with `verify_signature_raw` against a public key over that record's 32-byte `seal_hash`. The record's `tile_index` is the tile whose chain hash the signature covers. Records are considered in increasing `tile_index` order. The first failed signature (or a record whose `seal_hash` does not match `edge_seal_chain` at that `tile_index`) invalidates the tile at that `tile_index` and every subsequent tile; earlier tiles that already verified remain valid floors. A later type-70 record with a higher `tile_index` does not re-validate an earlier failure. The type-70 container tile itself is not a valid data floor when its signatures fail. `FLAG_ED25519_SIGNED` set with no type-70 tile and no usable key is **Quarantined** (AT-134).

    The pack **must not treat a public key embedded in the file** (`DeviceIdentityRecord.public_key`, or any key carried in the same bytes) as a trust anchor. That is the same rule as `s3a-reader-core` `verify(pinned_keys)` and as s3a-vault contact pinning (`vault contact add` of a full 32-byte key; S3A-EDGE-PROFILE.md §543 "cached or provisioned public_key"):

    *   The trust anchor is an optional **pinned-key list**. The host flag `--pinned-keys-file` names a UTF-8 file of hex-encoded 32-byte Ed25519 public keys, one per line, trailing newline stripped the same way as the token file; empty lines and lines starting with `#` are ignored. The file, when the flag is set, must be euid-owned and mode `0600`. Mobile callers pass the same list through the app; the pack does not read a host flag.
    *   If the pinned list is **empty or omitted**, a signature that verifies against a key found in the file is at best **Self-signed**, never **Verified**.
    *   If the pinned list is **non-empty**, the signer must be in it or the check fails and the file is **Quarantined** ("signer key not in pinned list").
    *   UI labels, matching `VerificationStatus` in `s3a-reader-core/src/verify.rs`, are exactly: **Verified**, **Self-signed**, **Unsigned**, **Quarantined**. `SignerTrust` is **Embedded** or **Pinned**; only Pinned plus a successful signature yields Verified.
    *   AT-133: a file re-signed with a different key that is not in the pinned list is Quarantined, even if the new key is embedded in a `DeviceIdentityRecord`.
3.  **First view:** a derived header sheet via `render_manifest` (also `view=banner`), clearly labeled as derived from the file header and `DeviceIdentityRecord`, not as a stored tile 78 manifest. Fields are only those present: magic, flags, `tile_stride`, `tile_count`, generation, timestamps (RFC3339 seconds UTC, `humantime::format_rfc3339_seconds`), `file_uuid`, device id, hardware model, firmware version, and the verification label from item 2. Missing fields are omitted, not invented. If no `DeviceIdentityRecord` is present, omit device fields and show a labeled "device identity record absent" note; still open the file (the EDGE §6.3 MUST is on the writer). If several identity records exist, the header sheet uses the first in tile order and later ones remain floors. Device identity fields on the header sheet and on type-64 identity floors are **exempt** from research-purpose record filtering: they identify the file and the device, not a consent-gated payload record (DD-53, AT-86).
4.  **Warehouse tour scaled to micro-tiles:** each committed valid micro-tile is a floor, via `render_warehouse`. `size_bytes` is 4096 or 8192. Offsets follow Variant A (`512 + i * tile_stride`) or Variant B (tile 0 at `P = tile_stride`, which is 4096 or 8192). Visual footprint follows §5.8. The Continue control from the header sheet reaches the tour, same as §5.3.
5.  Type 64 with 32-byte records renders as an equipment event log: timestamp (RFC3339 seconds UTC), `event_type` name from EDGE §5.2.2, severity, and a non-color category label derived from `event_type`: **fault** for `0x0003` and `0x0004`; **maintenance** for `0x0005` and `0x0006`; **normal** for `0x0001`, `0x0002`, and `0x0007`; **unknown** for any other value. Color is by severity (DD-07), which **overrides** the category coloring in EDGE §10 (a maintenance event with severity 0 uses the severity-0 color, not a category color). Color is never the only channel. Exact RGB is Q-10; AT-19 asserts pairwise-different colors plus labels. Type 64 with 96-byte records renders as `DeviceIdentityRecord` using `edge_display_layouts()`. Type 64 with `record_count` 0, or a record size that is neither 32 nor 96, is an "unknown layout" floor, not event columns and not identity columns.
6.  Edge consent uses type 71, not type 82 and not core `ConsentRecord`. If the file has **no** type-71 record, research-purpose filtering does not withhold records: the tour's "open / unrestricted (no consent records)" behavior applies (AT-85). When a type-71 record exists and does not allow the research purpose, event-log payload records are withheld and `withheld_count` is reported (AT-23); header and identity remain visible (AT-86).
7.  No transcoding into a 128 KiB hyper-tile file is required in order to view (EDGE §1.2). Import via `import_edge_archive` is a separate, existing engine operation, not the viewer path.

The microcontroller continues to run only `s3a-edge`. It does not link kitesurf or the host (AT-22, DD-06).

### 5.5 Scholar wiki browser (3-pass) — hub and PC only

Phone v1 does not implement this section (DD-45).

*   Wiki Markdown from under `--wiki-root` / `--corpus-root` is parsed with `pulldown-cmark` (raw HTML disabled or escaped) and painted with `render_page`. No JS, no remote subresources, no JS context. Missing pages are reported as missing. HTML files opened under an allowed root are also `render_page`, with JavaScript disabled and **no subresource fetches** (no CSS, images, or fonts over the network; DD-77, AT-121).
*   **Current candidate list (DD-61, F-07, F-35).** There is no persisted candidate list. `GET /v1/candidates` and the membership check for select, download, and skip **recompute the set at that request**. A `paper_id` that is in the set at request time is accepted even if no prior GET listed it. A `paper_id` that is not in the set at request time is `404` `unknown_paper`, even if an earlier GET listed it (AT-105). The set is the union of:
    1.  every regular file matching the exact glob `{root}/papers/SRC-*.md` (not recursive) under `--wiki-root` and under `--corpus-root` (identity is the `SRC-NNNN` stem of the filename; if the YAML `source_id` is present and does not equal that stem, the file is omitted from the list and no paper id is written to logs);
    2.  every live (non-tombstone) `AcademicPaperRecord` in every regular `*.s3a` file found by a recursive walk of `--corpus-root`. There is **no join** of an SRC stem to a `paper_id` (v4's "already the same paper" wording is deleted; there is no grounded join key). If the same `paper_id` appears in more than one live record, keep one: sort the containing `.s3a` paths, then take the first record in tile order in the first of those files. The walk skips files that fail to open.
    If the same relative path `papers/SRC-XXXX.md` exists under both roots, **`--wiki-root` wins**; there is one candidate. Building the list twice writes nothing (AT-24). Each candidate includes `source_url` (from the optional YAML key `source_url` if it is a valid `http`/`https` URL, else `null`) and `on_disk` (true when a usable local file exists; see the source decision table).
*   **Source decision table (DD-59, F-06).** A dossier `.md` file is **not** itself the on-disk source. On-disk bytes, when present, come from a `drive_path`:
    *   an absolute `drive_path` must canonicalize inside `--wiki-root`, `--corpus-root`, or `--view-root` if set;
    *   a relative `drive_path` is resolved against the **parent directory of the dossier file**, then must canonicalize inside one of those roots.
    A Windows Google Drive path that does not so canonicalize is not an on-disk source. Remote bytes, when present, come from the optional front-matter key `source_url` (draft name; not present in the grounded 3Pass tree; fixtures and future dossiers may set it). `AcademicPaperRecord` typically has neither a `source_url` nor an on-disk file. `POST /v1/open` does not accept a URL. At select/download:

    | `source_url` | usable `drive_path` | Fetch | Snapshot | Download delivery |
    | --- | --- | --- | --- | --- |
    | yes | no | yes (unless a snapshot already exists for this identity + current archive) | yes, on 2xx decoded body | bytes from the snapshot BLOB tiles |
    | no | yes | no | no | copy of the on-disk file |
    | yes | yes | no (prefer on-disk) | no | copy of the on-disk file (AT-129) |
    | no | no | no | no | `200` with `path: null`, `download_error: "no_source"`; inclusion and index still written (AT-128) |

    Fetch is triggered only by select or download of a request-time candidate in the first row of that table. Select and download share one fetch: if a snapshot already exists for the identity in the current working archive, download does not fetch again and reassembles bytes from that snapshot's BLOB tiles (DD-61, AT-104 for the on-disk-only row).
*   Pass 1 list and skip write nothing. The 15-second timer may be shown in the native UI; it is client-side; expiry does not call a host write endpoint (DD-08). AT-29 waits 15 seconds after only `GET /v1/candidates`, `POST /v1/open`, and `GET /v1/frame` and checks byte-identity of the working archive, `--snapshot-dir`, `--download-dir`, `--data-dir`, and `--log-dir`.
*   **Numbered write steps (F-05, DD-84).** Select and download share this order, under the process-wide write mutex:
    1.  Write the inclusion record (tile 10) and append the inclusion-index line (including a null `snapshot_sha256` if no snapshot exists yet).
    2.  If the source table requires a fetch, fetch and write the snapshot (temp file + atomic rename); then update the index line's `snapshot_sha256`.
    2b.  For a download request only: deliver bytes into `--download-dir` (copy, or reassembly from the snapshot).
    3.  If consent is `on`, emit or queue the LRS statement.

    If step 1 fails, skip 2, 2b, and 3, and leave `--download-dir` unchanged (AT-89, AT-127). The earlier DD-56 sentence that kept a download after a failed inclusion is **deleted**; it is unreachable once 2b follows 1. If the fetch fails or times out after step 1, the inclusion record is kept, `snapshot` is `null`, `fetch_error` is set, 2b is skipped unless an earlier snapshot already exists, and step 3 is still attempted if consent is on (the statement may omit content SHA-256).
*   Tracing consent on/off, and "zero LRS requests when off", are enforced including on retries (DD-66). Consent toggling is only `POST /v1/consent` (DD-50).
*   Only a UI-token `POST /v1/select` or `POST /v1/download` creates an inclusion record (DD-32, DD-60). "Human select" means "a call authenticated with the UI token", including one made by the native window. AT-28 tests an unauthenticated (401) call.
*   Pass 1 list and skip never write paper ids or URLs into `--log-dir`, and none into `--data-dir` other than `inclusion-index.jsonl`. The retry queue under `--data-dir/lrs-retry/` may hold identities only when consent is on and a sendable statement exists. Render caches and per-request temp files live in the ephemeral directory of §5.2.2, not under `--data-dir` or `--log-dir`. (AT-61; F-01: the index is local research data, like the working archive, and stays in `--data-dir`.)

**Inclusion record (DD-46, DD-61).** Tile 10 `HumanLrsRecord`: `timestamp_sec` is the write time (Unix seconds); `actor_hash` is `hash_iri(--researcher-id)` as implemented in `s3a-pack-scholar` `xapi.rs` (FNV-1a 64-bit; the field is `u64`); `verb_id` is the id of the existing `xapi.rs` verb chosen in Q-07; `object_hash` is CRC32C of the UTF-8 paper identity, stored in the `u32` field (CRC32C is already used in S3A); `decision_score` is 0; `ai_trace_coord` is the default / empty coordinate; flags are `FLAG_ACTIVE`. Because a 32-bit CRC is not a unique pointer to a path or identity, **idempotency is not evaluated from `object_hash` alone.**

**Inclusion index (F-01, F-02, F-03).** `--data-dir/inclusion-index.jsonl` is local research data in `--data-dir`. Each successful inclusion appends one JSON object:

```
{"paper_identity":"<id>","verb_id":<n>,"object_hash":<n>,"timestamp_sec":<n>,"working_archive":"<canonical path>","file_uuid":"<hex of FileHeader.file_uuid>","snapshot_sha256":"<lowercase hex or null>"}
```

The idempotency key is **current working-archive `file_uuid` + paper identity string + verb id**, looked up among index lines whose `file_uuid` equals the open archive's `FileHeader.file_uuid`. Lines whose `file_uuid` or canonical `working_archive` path do not match the open archive are **non-matching** and do not suppress a new inclusion (a deleted and recreated archive has a new `file_uuid`). `snapshot_sha256` is null until a successful snapshot exists; a later successful step 2 updates that field (rewrite of that one line, still under the write mutex). An idempotent second select of an identity that already has a snapshot returns `200` with `ok: true` and `snapshot` set to `--snapshot-dir/{snapshot_sha256}.s3a` (AT-83). A later download of that identity reassembles bytes from the snapshot BLOB tiles and does not fetch again.

If the index file is missing at startup, the host creates an empty one (mode `0600`) and does **not** skip writes based on existing tile-10 `object_hash` values (duplicates of older records are possible; dropping a real inclusion is not). The phrase "the inclusion record points at a path" is not used: the index stores the identity string, the archive identity, and the optional snapshot digest.

**Idempotency (DD-34, Q-07).** Selecting the same paper twice with the same verb against the same archive `file_uuid` writes one record and at most one successful snapshot. The v5 draft for Q-07 remains **one existing verb for both select and download**, so select-then-download is also one record (AT-83). If Ashley later picks two verbs, select-then-download writes two records; AT-83 follows the accepted mapping. Idempotency applies only to **successful** artifacts: a failed snapshot may be retried by a later select/download of the same paper.

**LRS client (DD-65).** When `--lrs-url` is set, the host POSTs to `{lrs-url}/xapi/statements` ( `--lrs-url` is the origin, with no trailing `/xapi`; the host appends `/xapi/statements`). `--lrs-url` must use `https`, except `http` is allowed when the host is loopback (for tests). The request uses HTTP Basic from `--lrs-credentials-file` (the s3a-lrs-server format: lines `user:password[:scopes[:archive]]`; `#` comments and blank lines ignored; kite uses the first credential line), plus `X-Experience-API-Version: 1.0.3` and `Content-Type: application/json`. The body is one xAPI 1.0.3 statement:

*   `id`: a UUID generated at first emit and stored in the retry-queue file, reused on retry (so a replay is the same statement).
*   `actor`: `{ "objectType": "Agent", "account": { "homePage": "<--lrs-actor-homepage>", "name": "<--researcher-id>" } }`. The LRS therefore sees the researcher id; `actor_hash` in the local tile does not anonymize the statement (privacy is the researcher's consent).
*   `verb.id`: the Q-07 IRI from `xapi.rs`.
*   `object.id`: the candidate `source_url` if present; otherwise `https://ashecribb.dev/xapi/activities/paper/` plus the percent-encoded paper identity. That host is already used by Scholar verb and extension IRIs in `xapi.rs`; this spec does not mint a new host.
*   `object.definition.extensions` includes `EXT_PAPER_ID` (the identity). It includes `EXT_SOURCE_URL` when a `source_url` exists, and `EXT_CONTENT_SHA256` when bytes were captured.
*   `timestamp`: RFC3339 seconds UTC of the inclusion write.

AT-110 asserts this shape. A batch of several statements may be sent as a JSON array, per xAPI, but v1 retries are one statement per request.

**Retry queue (DD-51, DD-66).** Queued LRS statements live on disk under `--data-dir/lrs-retry/` (one file per statement, including the statement `id` and body, mode `0600`) and survive a host restart. **Every LRS send, including a startup retry, first checks that the in-memory consent state is `on`.** If consent is `off` at process start, the host deletes the queue and issues zero LRS requests (AT-111). Revoke (`POST /v1/consent` to `off`) deletes that directory. If deletion fails, the host returns `500` `revoke_delete_failed`, treats the queue as **not sendable** (a send barrier until the directory is gone), and still records consent as `off` (AT-124). Backoff and a maximum attempt count are Q-09; until Ashley sets them, the host retries at process start (only if consent is `on`) and after each later successful LRS emit, and surfaces a visible error after a failed attempt without dropping a sendable queue.

### 5.6 Write path and sealed snapshots

**Target (DD-27).** Inclusion records go to `--working-archive`. Snapshots go in `--snapshot-dir` as `{lowercase hex SHA-256 of the captured bytes}.s3a`. The corpus and wiki are read-only. The snapshot directory is an allowed open path, so AT-31's re-open is allowed.

**API and locking (DD-28, DD-84).** `TileWriter::create` / `open_append`, exclusive lock held only for the tile-10 write (step 1), `ADDON_EDU` on create, fail closed if an existing file lacks the bit. On `create`, kite also writes one active core `ConsentRecord` with `purpose = Research` and `scope = 0` (all), `granted_by` derived from `--researcher-id`, so that once any consent record exists, the owner's inclusion records remain explicitly granted (DD-53, AT-79). This record is not required for visibility of a file that has **zero** consent records (DD-67). Snapshot files are written to a temp file in `--snapshot-dir` and published with an atomic rename. Concurrent select/download/skip/consent are serialized by the process-wide write mutex (AT-126).

**DD-63 retired (F-22).** v4's "kite-written snapshot exemption" is deleted. A BLOB+PROVENANCE-only snapshot contains no `ConsentRecord`, so DD-67 already leaves it unrestricted. Research-purpose filtering is never skipped merely because a file sits in `--snapshot-dir`. A third-party archive copied into `--snapshot-dir` is filtered by the same §5.3 / §5.4 rules as any other file (AT-107, AT-108).

**Snapshot encoding (DD-47, F-33).** Captured bytes are stored in core BLOB tiles (68) using the 40-byte prefix in `docs/spec/registry.md` / `s3a-pack-clean` `chunk_tiles`: `blob_key` (32-byte SHA-256 of the **complete** captured bytes), little-endian `chunk_index`, little-endian `total_chunks`, then chunk data. Each tile payload is at most `HYPER_TILE_PAYLOAD_SIZE` bytes, so each chunk is at most `HYPER_TILE_PAYLOAD_SIZE - 40` bytes. Reassembly concatenates chunks `0 .. total_chunks-1` of the same key. The "payload hash" in AT-31 is that SHA-256; it must equal `blob_key` and SHA-256 of the reassembled bytes. A payload larger than 128 KiB is required in AT-82.

PROVENANCE tiles (72) use the same chunk layout. The reassembled provenance **document** (after the 40-byte prefix) is exactly the UTF-8 bytes

```
sha256=<lowercase hex>
url=<source_url>
captured=<RFC3339 seconds UTC>
```

with LF line endings and a final newline. `url=` is followed by the `source_url` (this document is written only for a fetched snapshot, which always has a URL). There is no "or empty" alternative. `blob_key` of the PROVENANCE tile is the SHA-256 of that document (not of the captured content). Kite seals with `seal_archive` (no seal-generated tile-78 requirement). If a seal manifest is later added, it is the archive's ordinary stored manifest and is not counted against AT-32.

On-disk wiki or corpus sources are not re-snapshotted. Skip does not snapshot and does not write an inclusion record.

**Download names (DD-62).** The destination path is `--download-dir/{lowercase hex SHA-256 of the delivered bytes}` with no added extension (the bytes are the authority). Sanitization is unnecessary because the name is hex only. If that path already exists and the existing file's SHA-256 equals the name, download is a **success no-op** (the path is returned, the file is not overwritten). If a file exists at that path whose contents do **not** hash to the name (a planted or corrupt file), download fails with `409` `download_conflict`, does not overwrite, and if the inclusion write already succeeded it is kept (AT-94, AT-106). A `no_source` download writes no file in `--download-dir`.

### 5.7 Mobile build (Android, then iOS)

s3a-kite is the **viewer** inside [s3a-reader](https://github.com/ashleycribb/s3a-reader), as a UniFFI library from `s3a-pack-kite`. v1 phone builds do not offer Scholar candidate lists, select, download, LRS, or working-archive writes (DD-45). Those stay on the hub/PC host. v1 UniFFI exports open, `open_verified`, `open_path`, render, hit-test, `paper_identity`, and the `KiteVerificationReport` type (AT-99, DD-80). The pack does not depend on `s3a-reader-core`; the app maps a reader-core `VerificationReport` into `KiteVerificationReport` before calling `open_verified`.

**Why this is feasible without a mobile GPU.** Kitesurf already rasterizes on the CPU through `vello_cpu` / `VelloCpuImageRenderer` to an RGBA pixmap. The mobile library paints the manifest, warehouse, and page into a CPU buffer and hands pixels plus the text layer to the UI toolkit. v1 does not add a wgpu or platform GPU backend (DD-21). `cargo tree` for Android and host targets must not include `wgpu` or a Vulkan / Metal / GLES backend at all, not only "on the tour path" (AT-41).

**Fonts (DD-22, DD-35).** Keep `system_fonts: false`. The app ships UA font bytes (Noto Sans, Italic, Serif, Mono, and Symbols) and registers them with the same `register_ua_fonts` / `populate_ua_fonts` path. Missing those required UA fonts is an error, not a fallback to device or host fonts (AT-42). The **host** binary embeds the same required UA font bytes with `include_bytes!` and registers them through the same path; AT-42 applies to the host as well as to mobile (DD-94, F-37). Noto Color Emoji and Noto Sans SC are optional **bundled asset packs** (Play Feature Delivery, or a sideloaded asset packaged with the APK). They are not fetched over the network. The no-network Android manifest stays in force. When those optional packs are absent, the file still opens: required Latin text is painted with the shipped UA fonts; code points not in those fonts appear as the font's `.notdef` glyph (a visible missing-glyph box), not as an error, and not from system fonts, and with zero network requests (AT-49). Q-05 remains which packs ship in the first Play/App build.

**Android (v1 ship target, already decided for the reader).**

*   UniFFI `cdylib` + Kotlin, cargo-ndk, minSdk 26, ABIs `arm64-v8a` and `x86_64` (DD-23), Jetpack Compose.
*   Compose displays the Rust pixmap (for example `ImageBitmap` / `Bitmap` from RGBA) and exposes the text layer to TalkBack (AT-63). The default screen after open is the kite first view (manifest, header sheet, or page), then the warehouse after Continue for archives (AT-43). The warehouse tour does not use a WebView.
*   SAF / share-sheet open paths already in the app stay the way files arrive. Bytes are passed into `open` via the existing `MmapReader::from_bytes` path (no new scratch-file invention). The Kotlin deck plan is no longer the default (DD-24).
*   When the app has a reader-core `VerificationReport`, it maps that report into `KiteVerificationReport` and calls `open_verified`. Kite uses the supplied report and does not recompute a disagreeing result (DD-80, AT-137). If the app calls `open` instead, kite runs the same pinned-key algorithm as the host (`--pinned-keys-file` / the list the app passes). Quarantined files may be opened with a persistent banner; invalid tiles follow §5.4 item 2 (DD-20).
*   The app's current "no network" manifest remains the default. Q-03 draft: not in v1 phone builds; a later DD would be required to add phone network or phone writes.

**iOS (feasible, not the first ship).** Feasible because the paint path is portable `std` Rust plus CPU raster, not Metal. UniFFI already planned for Swift in the reader repo. Follow-on, required once the pack links a `staticlib` for `aarch64-apple-ios` and `aarch64-apple-ios-sim` (AT-40, DD-25). SwiftUI (draft) shows the same RGBA frames and text layer. Same font and no-GPU rules.

**Native-link work the pack must do (not optional, not a stub).** Kitesurf's workspace today pins `getrandom` 0.2/0.3 with `js` / `wasm_js` features, and `fonts.rs` uses `#[wasm_bindgen]`. The `native-host` feature already gates some exports. The pack's fork must compile `kitesurf-core` for `aarch64-linux-android`, `x86_64-linux-android`, and later Apple iOS targets **without** the Wasm `getrandom` JS backend and without requiring a JS host (AT-44). That is a fork change in the pack, with Apache change notices on touched files.

**ESP32 / MCU.** Not a kite viewer target.

**Budgets (DD-06, Q-06).** The "measure on a named minSdk-26 device before adding a second renderer" trigger stays dormant until Ashley names the device and the numeric budgets (peak RSS, time to first manifest frame, APK size delta versus the current reader). This spec does not invent those numbers.

### 5.8 Render API, warehouse scale, and animation

Every paint function returns a `RenderFrame`, not only pixels (DD-38):

```text
RenderFrame {
  rgba: [u8],           # packed RGBA, length = width * height * 4
  width: u32,
  height: u32,          # pixmap pixels; at scale 1.0, 1 CSS px = 1 pixmap px
  text_runs: [TextRun],
  semantics: [SemanticNode],
}
TextRun { text: String, x: i32, y: i32, width: u32, height: u32 }   # pixmap coordinates; x/y may lie off-pixmap
SemanticNode {
  role: String,         # one of: document, heading, button, label, link,
                        # floor, continue, select, paragraph
  label: String,
  x: i32, y: i32, width: u32, height: u32,
  href: Option<String>,       # set when role is link
  paper_id: Option<String>,   # set when role is select
  floor: Option<u32>,         # set when role is floor
}
```

Acceptance tests that assert on "text" use concatenated `text_runs` as contiguous substrings, never OCR. TalkBack and VoiceOver read `semantics`.

Every `render_*` takes `width`, `height`, and `device_pixel_ratio` (DD-69, DD-90). `width` and `height` are **CSS pixels**. The pixmap is `ceil(width * device_pixel_ratio)` by `ceil(height * device_pixel_ratio)` pixels; `rgba` length is pixmap width × pixmap height × 4. `device_pixel_ratio` must be a finite number greater than 0. The product of the pixmap dimensions must be ≤ the required `max_frame_pixels` passed to `open` / `open_verified` / `open_path`, or, on the host, the required `--max-frame-pixels`. A request that violates this, or whose CSS `width` or `height` is 0, is `400` `invalid_frame_size` (AT-141, AT-143). At `device_pixel_ratio = 1.0`, 1 CSS px = 1 pixmap px (DD-37). `/v1/hit-test` `x`/`y` are CSS pixels in the same space as `width`/`height`. The same defaults as `/v1/frame` apply when an FFI wrapper omits scroll / instant / dpr, and those defaults are part of the contract: `scroll_x=0`, `scroll_y=0`, `instant=false`, `device_pixel_ratio=1.0`. `select` semantics are present only when `select_enabled` is true (`render_page(..., select_enabled)`; false on a viewer-only host and on mobile; AT-144).

**`instant` (AT-13, AT-93).** When `instant` is true, `render_floor_transition` returns only the end frame (the t = 0.45 layout) and ignores `t_seconds`. When `instant` is false, `t_seconds` is used as below.

**Scale (DD-37, DD-70, AT-16, AT-92, AT-120).** At reference scale 1.0, bays are square. Side lengths are the nearest integer to `32 * sqrt(stride / 4096)` CSS px, so area is only **approximately** linear in stride. Specified integer sides: **32** (4 KiB), **45** (8 KiB, nearest integer to 32√2), **181** (128 KiB, nearest integer to 32√32). AT-16 and AT-92 render the 4 KiB and 8 KiB fixtures **separately** and assert those integer sides; they do not assert exact area doubling. AT-120 uses a 128 KiB fixture and asserts side 181. Mixed-stride warehouses are out of v1 (a file is either Edge 4 KiB, Edge 8 KiB, or hyper-tile 128 KiB). The warehouse is a scrollable grid; `scroll_x` / `scroll_y` reach floors that do not fit the viewport. Floors are not automatically shrunk below the reference scale.

**Animation (DD-18, DD-37, AT-62).** Viewport units map to the frame arguments: `1vw = width / 100`, `1vh = height / 100`. The start rectangle is the bay's layout rectangle in frame coordinates at the given warehouse scroll (it may lie partly or wholly outside `[0, width] × [0, height]`; the pixmap still only contains in-viewport pixels). Without `instant`, the floor-to-detail card starts at t = 0 on that start rectangle at opacity 0.2, and at t = 0.45 s is at `left = 0.06 * width`, `top = 0.08 * height`, `width = 0.88 * width`, `height = 0.84 * height`, opacity 1. Progress between those endpoints is the y-value of CSS `cubic-bezier(.2,.8,.2,1)` at unit time `t / 0.45` (at `t = 0.225` s, unit time 0.5, progress is approximately 0.946). Tests sample t = 0, 0.225, and 0.45 (AT-62). An off-screen bay still animates from its (possibly off-pixmap) start rect to the end rect.

### 5.9 Robustness

`cargo-fuzz` target `open_and_render` (AT-71) is required, with a **checked-in seed corpus** and a **corpus-replay** CI job that runs every seed to completion (not only a timed fuzz). A timed fuzz duration for CI is Q-14; this spec does not invent a duration. Malformed input is an error or quarantine, not a crash.

--------------------------------------------------------------------------------

## 6. License obligations

The pack is a derivative of Kitesurf and must comply with every license Kitesurf already lists in `LICENSE` and `NOTICE`. At minimum:

### 6.1 Apache-2.0 (Kitesurf, and many Rust crates)

From `LICENSE` §4 (Redistribution):

1.  Give recipients a copy of the Apache-2.0 license.
2.  Cause modified files to carry prominent notices that they were changed.
3.  Retain all copyright, patent, trademark, and attribution notices from the source.
4.  If a `NOTICE` file is distributed with the Work, include a readable copy of its attribution notices in a NOTICE in the derivative, or in the documentation, or in a display generated by the derivative. The pack MUST ship Kitesurf's `NOTICE` (copyright "kitesurf / Copyright (c) 2026 Cloudflare, Inc.") plus any added notices. Additional notices must not be construed as modifying the License.
5.  The pack may add its own copyright on modifications.
6.  Trademarks (§6): this license does not grant use of Cloudflare or Kitesurf marks except as needed to describe origin and to reproduce NOTICE.

A generated third-party license inventory (for example `cargo-about` or an equivalent check against the resolved tree, including UniFFI, pulldown-cmark, and other transitive crates in the resolved tree (Wasmtime is not a v1 dependency; DD-44)) ships with the distribution and is shown on an in-app About / licenses screen (AT-02, AT-70).

### 6.2 MPL-2.0 (Stylo, and the Public Suffix List if that engine file is kept)

Stylo is MPL-2.0 (`README.md` Technology stack; `NOTICE` lists stylo among patched crates). The Public Suffix List snapshot is MPL-2.0 (`NOTICE`, `licenses/MPL-2.0-PSL.txt`).

MPL is file-level copyleft:

1.  Modified MPL-covered files (Stylo, Stylo patches under `scripts/patches/`, and the PSL if bundled) must remain MPL-2.0, and their source must be provided (a source offer in the distribution and on the About screen).
2.  The larger work (the pack, Apache-2.0) may link them.
3.  Do not remove MPL notices.

### 6.3 Other NOTICE items the pack must retain if the corresponding assets ship

*   SIL OFL 1.1 for Noto, Inter, and related fonts (`packages/engine/public/fonts/OFL.txt`, `licenses/OFL-1.1-*`).
*   jpeg-encoder: MIT OR Apache-2.0, **and** IJG terms (`licenses/IJG-jpeg-encoder-0.7.0.txt`); "This software is based in part on the work of the Independent JPEG Group."
*   WPT / Webref / TodoMVC / wasm-memprof / anti-slop, if those files remain in the fork.

A release artifact (host directory, Android APK, or iOS IPA) that omits NOTICE, LICENSE, the third-party inventory, Stylo/PSL source availability, OFL texts, or the in-app About screen fails AT-02 / AT-70.

--------------------------------------------------------------------------------

## 7. Acceptance tests

Tests are written first. They are split into two classes:

*   **Fail-first.** A concrete check that does not hold on today's trees (no `s3a-pack-kite` / `s3a-kite` host / UniFFI kite renderer, or the behavior is not implemented). Implementation must make these pass **without editing the tests** except as listed later with reasons.
*   **Guards.** Checks that already hold on today's trees, or that become meaningful once the pack exists and must then stay green. They are not claimed to fail first. AT-22 and AT-37 already pass today. AT-05, AT-36, and AT-41 are written so that, until the host or pack crate exists, they are skipped as guards (they do not "pass vacuously" by asserting nothing); once the subject exists, they must pass.

Fixtures: hyper-tile archives built the same way as `crates/s3a-cli/tests/tour_tests.rs` (`write_manifest: true` for AT-08); Edge vectors `micro_valid_4kb`, torn and signed vectors from `s3a-spec-vectors`; an 8 KiB file produced with `EdgeWriter<_, 8192>`. Text assertions use the text layer in §5.8 and must match the fixture bytes as contiguous substrings of concatenated `text_runs` (not only a fuzzy OCR of the pixmap).

Mechanical helpers: AT-03 diffs the vendored Kitesurf tree against pin `235111c6fe10f2d63f3476bf5e3d19b4c7089415` and greps each changed file for the change notice. AT-36 uses `cargo deny` (or equivalent) bans plus a grep on pack-owned source only, not the entire vendored upstream tree.

Negative write tests (AT-28, AT-46, AT-57, AT-73, AT-74, AT-89, AT-94, AT-105) first run a **control** select that succeeds on a valid `ADDON_EDU` working archive in the same harness, then run the failing case, then assert the failing target is unchanged (or, for AT-89, that no snapshot was created). AT-46 uses a **second host process** whose `--working-archive` is the file lacking `ADDON_EDU` (one process serves one archive, DD-48).

Pack tests live in the s3a-storage workspace. AT-38, AT-39, AT-43, and AT-63 live in s3a-reader and are feature-gated or in a separate test crate that does not build in the default reader CI job until the reader pin moves off `ff89564`. Android and iOS ATs are skipped, not failed, on a host-only CI job, and the skip is reported. "Land" means local branches; nothing is pushed until Ashley says so. AT-76 is exempt from the same-process control-select requirement (a viewer-only host cannot select); any control write for AT-76 runs on a separate Scholar host.

### Pack and license

*   **AT-01** (fail-first). `cargo test -p s3a-pack-kite` exists and the crate's Cargo manifest does not cause `s3a-core` to depend on kite or kitesurf (`cargo tree -p s3a-core` has no such edge).
*   **AT-02** (fail-first). The pack's distribution directory, and the Android APK / iOS IPA when those artifacts are built, contain Apache-2.0 `LICENSE`, Kitesurf `NOTICE` (including the Cloudflare 2026 copyright line and the Stylo / PSL MPL notes), OFL text for every shipped font, and IJG text if jpeg-encoder is linked. A CI check lists these files; a missing NOTICE fails.
*   **AT-03** (fail-first). A diff of the vendored Kitesurf tree against `235111c6fe10f2d63f3476bf5e3d19b4c7089415` lists every modified file, and each of those files starts with or includes a prominent "changed from upstream kitesurf `235111c6fe10f2d63f3476bf5e3d19b4c7089415`" notice in addition to the original copyright (Apache §4).
*   **AT-70** (fail-first). A generated third-party license inventory covers the resolved tree (not only Kitesurf's NOTICE). The distribution includes an MPL source offer for Stylo / PSL. The app's About / licenses screen shows those notices.

### Host

*   **AT-04** (fail-first). `s3a-kite host` with every Scholar-mode required flag in §5.2.2 (`--bind 127.0.0.1`, `--port <ephemeral>`, `--auth-token-file`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--researcher-id`, `--fetch-timeout-secs`, `--max-fetch-bytes`, `--max-open-handles`, `--headless`) starts without Wrangler and without a Worker Loader, and `GET http://127.0.0.1:<port>/health` returns HTTP 200, `Content-Type: text/plain`, and body exactly `ok\n`, without a token. Today there is no such binary.
*   **AT-05** (guard once the binary exists; skipped until then). That binary does not read `packages/engine/wrangler.jsonc` and does not call a `LOADER.load` API.
*   **AT-06** (fail-first). The same host, without a valid token, returns **401** on every route in the §5.2.1 table other than loopback `GET /health` (no body that looks like a successful frame or handle). With a valid token, a request to `/json`, `/json/list`, `/devtools/*`, or any other path not in the table, or an `Upgrade: websocket` request, returns **404**. Without a token, those same unknown paths also return 401 (authentication first; AT-115). There is no CDP listener. An open unauthenticated listener other than loopback `/health` is a failure.
*   **AT-07** (fail-first). Omitting `--bind` in any mode, or omitting `--port` when `--bind` does not start with `unix:`, exits non-zero with an error that names the missing flag. No implicit port is bound. `--bind unix:/path.sock` without `--port` is the AT-113 success case.
*   **AT-50** (fail-first). On a non-loopback bind with TLS (not `--behind-proxy`), `GET /health` without a token is rejected; with a valid Bearer token it succeeds with body `ok\n`.
*   **AT-51** (fail-first). `s3a-kite host --bind 0.0.0.0 --port <ephemeral> --auth-token-file <token>` plus other Scholar flags, without `--tls-cert`/`--tls-key` and without `--behind-proxy`, exits non-zero and binds nothing.
*   **AT-45** (fail-first). Startup with an empty `--auth-token-file`, a secret shorter than 32 characters, or a file containing any byte outside `A–Z a–z 0–9 _ -` other than a single trailing `\n` or `\r\n`, exits non-zero, names the token file, and binds nothing (AT-81).
*   **AT-52** (fail-first). A UI-token request to open a path containing `../` that would escape `--corpus-root` or `--wiki-root` is rejected and opens no file.
*   **AT-53** (fail-first). A `file://` URL whose path is outside the configured roots is rejected.
*   **AT-54** (fail-first). `POST /v1/select` of a current candidate whose `source_url` is `http://169.254.169.254/` is blocked; the request does not leave the host; `fetch_error` is set; no snapshot is written. In a second case, the configured LRS hostname is resolved to an otherwise-allowed address (a global-unicast test address) and a fetch to that address is still denied because it is the LRS host.
*   **AT-55** (fail-first). `POST /v1/select` of a current candidate whose `source_url` is a `file://` URL pointing at the token file, the working archive, or a corpus file is rejected: no HTTP client read, no filesystem read via the fetch path, `fetch_error` is set, and those files are opened only through `POST /v1/open` under allowed roots (AT-123).

### Hyper-tile `.s3a`

*   **AT-08** (fail-first). Open a sealed archive produced like `test_tour_displays_manifest_in_lobby_and_floor`. The first frame is produced by `render_manifest`. Its text layer contains the exact stored manifest payload as a contiguous substring (including `THIS IS AN S3A FILE` and the seal title/creator). The warehouse floors are not the first frame.
*   **AT-09** (fail-first). Open a hyper-tile archive with no tile 78. The first frame is a labeled "manifest absent" state and does not contain invented title/creator text. After `proceed_to_warehouse` (or a Continue `hit_test`), `render_warehouse` is the next view.
*   **AT-10** (fail-first). After the manifest view, the tour floor list matches `s3a-cli tour --json` on the same hyper-tile file: `index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`.
*   **AT-11** (fail-first). Opening floor *N* with `render_floor_detail` on a golden hyper-tile fixture whose `withheld_count` was hand-verified from the §5.3 item 4 algorithm (core `ConsentRecord` + marks only; latest-wins per `subject_id` for `purpose == Research`; `is_active` as in `consent.rs`; type 82 is never consent) shows those records and that `withheld_count`. Fixtures that contain type 82 are included in this suite and must *not* treat type 82 as Edge or hyper-tile consent. The CLI is not the `withheld_count` oracle when type 82 is present. AT-135 and AT-136 cover only-non-research and revoked-research cases.
*   **AT-12** (fail-first). A manifest or record that contains `<script>`, `</script>`, or HTML angle brackets is painted as those characters (text layer). No script from the archive runs. No JS context is created.
*   **AT-13** (fail-first). Without `instant`, the floor-to-detail transition matches §5.8 (AT-62 is the timed sampling). With `instant=true`, `render_floor_transition` returns only the end frame (the t = 0.45 layout) and ignores `t_seconds` (AT-93).
*   **AT-62** (fail-first). `render_floor_transition` at t = 0, 0.225, and 0.45 s (`instant=false`) places the card rectangle and opacity at the §5.8 start, bezier mid, and end values, within 1 CSS px (and 0.02 on opacity).
*   **AT-64** (fail-first). A committed tile 78 that is not valid UTF-8 yields a first frame labeled "manifest not UTF-8" and invents no replacement text.
*   **AT-65** (fail-first). Two committed valid tile-78 tiles: the first view uses the first tile's bytes; the second appears as a floor.
*   **AT-69** (fail-first). A file whose `FileHeader.version` is not 1 fails to open with `Unsupported file version: {n}`.
*   **AT-72** (fail-first). An unsealed hyper-tile with a valid tile 78 opens, the first view's text layer includes the manifest and an unsealed seal status, and no seal trailer is invented.

### Edge micro-tiles

*   **AT-14** (fail-first). Open `micro_valid_4kb.s3a` (MICRO-VEC-01). The reader reports `is_edge_archive` and `tile_stride == 4096`. The first frame is the derived header sheet, labeled as derived, and includes stride 4096 and the committed `tile_count`. It is not presented as tile 78.
*   **AT-15** (fail-first). The warehouse list for MICRO-VEC-01 has exactly the committed micro-tiles. Each floor `size_bytes == 4096`. Offsets follow `512 + i * 4096` for Variant A.
*   **AT-16** (fail-first). An 8 KiB `EdgeWriter` fixture shows `tile_stride == 8192` and `size_bytes == 8192`. Rendered **separately** from a 4 KiB fixture at the §5.8 reference scale (`device_pixel_ratio` 1.0), the 4 KiB bay's side is exactly 32 CSS px and the 8 KiB bay's side is exactly 45 CSS px (AT-92). Area is only approximately linear (DD-70). A 128 KiB bay is AT-120.
*   **AT-17** (fail-first). MICRO-VEC-02: only the valid tile(s) appear as floors; the torn tile is omitted and the header sheet reports it as ignored/torn.
*   **AT-18** (fail-first). MICRO-VEC-03: seal verification matches `s3a-engine`. A copy with a reordered or mutated tile fails verification, shows quarantine, and does not list the mutated tile (or any subsequent tile) as a valid floor.
*   **AT-19** (fail-first). A type-64 tile of 32-byte `EquipmentEventRecord`s paints timestamp, the EDGE §5.2.2 event name, severity, and the category label from §5.4. Severity 0, 1, and {2, 3} use three pairwise-different badge colors (pixel samples); 2 and 3 share a color. Exact RGB values are Q-10 and are not asserted here. A severity-0 maintenance event still carries the label "maintenance".
*   **AT-20** (fail-first). A type-64 tile whose record size is 96 bytes paints `DeviceIdentityRecord` fields (device id, hardware model), not event columns.
*   **AT-21** (fail-first). A compact-header Edge file (`FLAG_COMPACT_HEADER`) places tile 0 at offset 4096 (4 KiB stride) or 8192 (8 KiB stride), not at 512.
*   **AT-22** (guard; already passes today). `crates/s3a-edge/Cargo.toml` does not depend on kite or kitesurf. A `no_std` build of `s3a-edge` still succeeds.
*   **AT-23** (fail-first). When an Edge type-71 `EdgeConsentRecord` does not allow the research purpose, kite withholds event-log payload records and reports `withheld_count`. The implementation must use the Edge layout, not type 82 and not core `ConsentRecord`. The derived header sheet and type-64 identity floors remain visible (AT-86).
*   **AT-66** (fail-first). A file with `FLAG_PROFILE_EDGE` and a 128 KiB stride, and a file with `FLAG_TILE_SIZE_4KB` and stride 8192, each open as HTTP 200 with `kind: "unsupported"` and `first_view: "banner"`, and are not toured as a consistent Edge or hyper-tile file. `GET /v1/frame?view=banner` (and `view=manifest`) returns a frame whose text layer contains the inconsistent-header banner text.
*   **AT-67** (fail-first). A type-64 tile whose record size is neither 32 nor 96, or whose `record_count` is 0, is shown as an "unknown layout" floor.
*   **AT-68** (fail-first). An otherwise valid Edge file with no `DeviceIdentityRecord` opens; the header sheet omits device fields and is labeled "device identity record absent".

### Scholar 3-pass and snapshots

*   **AT-24** (fail-first). Building the Pass 1 candidate list twice (including `GET /v1/candidates` twice) leaves the corpus archive, the LRS store, and `--log-dir` and `--data-dir` byte-identical to the start (no `HumanLrsRecord`, no xAPI statements, no new snapshot tiles, no paper ids or URLs in those directories).
*   **AT-25** (fail-first). Dismissing a paper as off-topic / skip (`POST /v1/skip`) writes nothing (same byte-identity check, including `--log-dir` and `--data-dir`).
*   **AT-61** (fail-first). After a consent-off select, `--log-dir` contains no paper ids or source URLs, and `--data-dir` contains none other than in `inclusion-index.jsonl` (which records the identity, the working-archive `file_uuid`, and a nullable `snapshot_sha256`). The working archive still has the inclusion record. `--data-dir/lrs-retry/` is empty or absent (consent was off, so no queued statements).
*   **AT-26** (fail-first). Selecting or downloading a paper, with tracing consent on and a configured LRS, writes one inclusion statement as specified in §5.5 (verb IRI from `s3a-pack-scholar` `xapi.rs`, not a newly invented IRI; actor, object, extensions, and stable `id` as in DD-65) and a Scholar `HumanLrsRecord` (tile 10) for that paper identity in the working archive, plus a snapshot of the AT-31 remote fixture. AT-110 asserts the statement body.
*   **AT-27** (fail-first). The same select of the AT-31 remote-fetched fixture, with consent off: zero HTTP requests to the LRS, and the working archive still contains the inclusion record. The snapshot of the captured bytes is the file `--snapshot-dir/{lowercase hex SHA-256}.s3a`, not a tile in the working archive. The corpus and LRS store are unchanged.
*   **AT-48** (fail-first). Download of the AT-31 remote-fetched fixture with consent off still delivers the file to `--download-dir` as `{sha256_hex}` and writes the local inclusion record and snapshot, with zero LRS requests.
*   **AT-28** (fail-first). After a successful control select, a call to `/v1/select` or `/v1/download` without a token is rejected and writes nothing (working archive, snapshot dir, download dir, LRS unchanged versus the post-control state).
*   **AT-57** (fail-first). There is no agent-token flag and no CDP Input domain. A request that presents a second token file, an `X-Agent-Token` header, or a synthesized CDP `Input.dispatchMouseEvent` returns 401 or 404 (per §5.2.1) and writes nothing. (v1 has only the UI token; DD-41.)
*   **AT-29** (fail-first). After only `GET /v1/candidates`, `POST /v1/open`, and `GET /v1/frame` (a candidate page with `view=page`), waiting 15 seconds (no select, download, or skip) leaves the working archive, snapshot dir, download dir, `--log-dir`, and `--data-dir` byte-identical. The host has no write-on-timer endpoint.
*   **AT-30** (fail-first). Opening a configured wiki `index.md` via `/v1/open` and `GET /v1/frame?view=page` (or `render_page`) paints its heading text from the file bytes as a contiguous substring of the text layer. Opening a missing `papers/SRC-9999.md` reports missing and does not invent a dossier.
*   **AT-56** (fail-first). A wiki page opened with `view=page` whose Markdown contains a script tag that would set `document.title` and an image tag with an `http` URL paints those tags as text or omits them as raw HTML; no script runs; no JS context is created; the render issues zero network requests.
*   **AT-31** (fail-first). The fixture is a candidate whose `source_url` points at an `egress_test` loopback server that returns a known body (not an on-disk-only SRC dossier). After select (consent on or off), a sealed snapshot exists under `--snapshot-dir` as `{sha256_hex}.s3a` whose `blob_key` and payload hash equal SHA-256 of that body; `POST /v1/open` on that path is allowed and `verify_sealed_archive` succeeds. Skip of the same fixture does not create a snapshot. On-disk-only inclusion is AT-104.
*   **AT-32** (fail-first). That snapshot's reassembled PROVENANCE document (bytes after the 40-byte tile prefix) is exactly `sha256=<lowercase hex>\nurl=<source_url>\ncaptured=<RFC3339 seconds UTC>\n` (LF, final newline). `blob_key` of the PROVENANCE tile is SHA-256 of that document. No other metadata fields are present. A seal-generated tile 78, if any, is not counted as provenance metadata.
*   **AT-46** (fail-first). After a successful control select on a valid `ADDON_EDU` archive in host A, a **second host process** B is started with `--working-archive` pointed at a file whose `addon_mask` lacks `ADDON_EDU`. Select on B fails, names the missing bit, and leaves that second file and the LRS store byte-identical.
*   **AT-47** (fail-first). After a successful snapshot write, `verify_sealed_archive` on the snapshot file succeeds.
*   **AT-58** (fail-first). Revoking tracing consent mid-session via `POST /v1/consent` with `{ "state": "off" }` (not by editing the file on disk) drops subsequent LRS requests to zero, including any retry queue (`--data-dir/lrs-retry/` is deleted); the already-written local inclusion record remains.
*   **AT-59** (fail-first). With consent on, an LRS 5xx does not roll back the local inclusion record or snapshot; a retry file is written under `--data-dir/lrs-retry/`; the corpus is unchanged.
*   **AT-60** (fail-first). Selecting the same paper twice in one working archive leaves a single inclusion record and a single snapshot.
*   **AT-73** (fail-first). After a successful control write, a second `TileWriter` / host that cannot take `try_lock_exclusive` on the working archive fails the write and does not truncate the file.
*   **AT-74** (fail-first). Scholar-mode select with `--working-archive` omitted is a startup error (AT-77). A running Scholar host whose working-archive path cannot be created or opened fails the select closed and writes nothing. Phone UniFFI exports no select/download (AT-99).

### Web pages vs archives

*   **AT-33** (fail-first). Opening a local HTML fixture under `--view-root` (a file that contains a `<script>` setting `document.title` and a static heading) via `POST /v1/open` and `GET /v1/frame?view=page` (or `render_page`) uses native-host layout with JavaScript disabled (DD-44). The script does not run: the text layer contains the static heading and does not contain the scripted title. `cargo tree` for the host does not include `wasmtime` or `boa_engine` (AT-34). This test does not fetch the public Internet.
*   **AT-34** (guard once the pack crate exists; skipped until then). `cargo tree` for `s3a-pack-kite` and for the host binary, on host and Android targets, does not include `boa_engine` or `wasmtime`. The tour is painted by the native `vello_cpu` renderer.

### Safety and scope

*   **AT-35** (fail-first). A hyper-tile or Edge file with a failed tile CRC or a failed Ed25519 check is labeled quarantined. A failed CRC tile is not shown as a valid floor. A failed Edge Ed25519 signature invalidates the signed chain from that point (the type-70 tile and tiles whose `seal_hash` the signature was meant to cover), not only the type-70 tile (DD-57).
*   **AT-36** (guard once the pack crate exists; skipped until then). `cargo deny` (or equivalent) plus a grep on pack-owned source and tests (not vendored Kitesurf) contain no Bitcoin, OpenTimestamps-write, payment, or token-transfer code paths. Reading Edge `seal_hash` values is allowed.
*   **AT-37** (guard; already passes today). No new `TileType` constant is added under `s3a-core` relative to the set at s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`.
*   **AT-71** (fail-first). The `open_and_render` cargo-fuzz target exists, a seed corpus is checked in, and a CI corpus-replay job runs every seed to completion (including the AT-08 fixture, MICRO-VEC-01/02/03, and a malformed file) without aborting. A timed fuzz duration for CI is Q-14 and is not invented here.

### Mobile library

*   **AT-38** (fail-first). The UniFFI library builds for `aarch64-linux-android` and `x86_64-linux-android` with minSdk 26. `open` + `render_manifest` + `proceed_to_warehouse` + `render_warehouse` return a `RenderFrame` (RGBA plus text layer) or `KiteError`. For the AT-08 fixture, the first frame's text layer contains `THIS IS AN S3A FILE`; for MICRO-VEC-01, the first frame's text layer includes `4096`.
*   **AT-39** (guard once the kite tour Compose screen exists; skipped until then). The Android UI path that shows the tour does not instantiate `android.webkit.WebView` for that screen. A lint or unit test on the Compose screen's view tree fails if a WebView is the tour surface.
*   **AT-40** (fail-first). `cargo build --target aarch64-apple-ios -p s3a-pack-kite` produces a `staticlib` (and the `ios-sim` target also links). Shipping the iOS app remains a follow-on (DD-25); this test is the feasibility gate. The job is skipped, not failed, on a host-only CI runner without the Apple toolchain, and the skip is reported.
*   **AT-41** (guard once the pack crate exists; skipped until then). `cargo tree` for `s3a-pack-kite`, for Android and host targets, does not include `wgpu` or a Vulkan/Metal/GLES backend at all. The linked paint crate is `vello_cpu` / `anyrender_vello_cpu`.
*   **AT-42** (fail-first). With the required UA fonts removed from the test fixture, `render_manifest` returns a `KiteError` that names missing fonts. It does not silently render with system or host fonts (`system_fonts` stays false). This assertion runs against the **host** binary (fonts embedded with `include_bytes!`) as well as the mobile library (DD-94).
*   **AT-43** (fail-first). In the Android app, opening the AT-08 fixture shows the kite surface as the default screen after open (first the manifest, then the warehouse after Continue). `MapLayout` is not the default.
*   **AT-44** (fail-first). `getrandom` for Android and iOS targets is not enabled with the `js` / `wasm_js` features. A build without a JavaScript runtime succeeds (the fork change required in §5.7, "Native-link work").
*   **AT-49** (fail-first). A manifest containing CJK characters, with Noto Sans SC and Color Emoji absent, opens successfully; required Latin text is in the text layer; CJK code points are painted as `.notdef` (not from system fonts); zero network requests.
*   **AT-63** (fail-first). The first-view semantics tree includes the manifest heading (or the "manifest absent" label). An Android TalkBack (or equivalent accessibility) test reads that label from the kite surface.

### v3 additions (round-2 dispositions)

*   **AT-75** (fail-first). Every route in the §5.2.1 table exists with the documented request and success response on a Scholar host. A path not in the table returns 404 with the JSON error object and still requires a token except loopback `/health`.
*   **AT-76** (fail-first). Viewer-only mode starts with `--viewer`, `--bind 127.0.0.1` (or `--headless`), `--port`, `--auth-token-file`, `--view-root`, `--log-dir`, `--max-open-handles`, `--max-frame-pixels`, and `--handle-idle-secs`. `POST /v1/select`, `/v1/download`, `/v1/skip`, `/v1/consent` and `GET /v1/candidates`, `GET /v1/consent` return 409 with error code `viewer_only` and write nothing (control-then-negative as in the preamble). Opening a file under `--view-root` with `/v1/open` still succeeds. Starting a viewer without `--max-open-handles` is AT-131.
*   **AT-77** (fail-first). Scholar mode missing any required flag in §5.2.2 (`--bind`, `--auth-token-file`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--researcher-id`, `--fetch-timeout-secs`, `--max-fetch-bytes`, `--max-redirects`, `--max-open-handles`, `--max-frame-pixels`, `--handle-idle-secs`, and `--port` unless `--bind` starts with `unix:`) exits non-zero and names the flag. `--headless` is required when `--bind` is not loopback and not `unix:`. `--consent-state-file` saying `on` without `--lrs-url` is AT-114. `--lrs-url` without `--consent-state-file` is a startup error that names `--consent-state-file`. A required directory flag that points at a missing path is a startup error that names the flag. The host does not create those directories.
*   **AT-78** (fail-first). Scholar start with `--working-archive` pointing at a missing file whose parent directory exists succeeds and binds `/health`. If the parent directory does not exist, startup fails and names `--working-archive`.
*   **AT-79** (fail-first). The first successful Scholar write creates the working-archive file with `TileWriter::create` and `enable_addon(ADDON_EDU)`, and writes an active core `ConsentRecord` with `purpose = Research` and `scope = 0`. Opening that archive in kite lists the owner's inclusion records (`withheld_count` does not hide them all).
*   **AT-80** (fail-first). A snapshot written to `--snapshot-dir` can be opened via `POST /v1/open` with that absolute path. A path outside every allowed root still fails (AT-52).
*   **AT-81** (fail-first). A token file of 32 allowed characters plus a trailing newline is accepted when `Authorization: Bearer` is exactly those 32 characters (no newline). A Bearer value that includes the newline, or a file that contains a space, a slash, or any non-alphabet character other than the single trailing newline, is rejected (the bad file is a startup error).
*   **AT-82** (fail-first). A `POST /v1/select` of a candidate whose `source_url` is an `egress_test` fixture that returns at least 200 KiB of non-repeating bytes produces a sealed snapshot under `--snapshot-dir` whose BLOB tiles use the 40-byte prefix, `blob_key` equals SHA-256 of the full payload, `total_chunks` is greater than 1, concatenating chunks `0 .. total_chunks-1` equals the original bytes, and `verify_sealed_archive` succeeds.
*   **AT-83** (fail-first). `POST /v1/select` then `POST /v1/download` of the same paper identity in one working archive, using the Q-07 draft (one existing verb for both), leaves a single inclusion record and a single successful snapshot. The second select returns `200` with the existing snapshot path. The download reassembles bytes from that snapshot's BLOB tiles and performs no second fetch. The index line stores `snapshot_sha256` and the working-archive `file_uuid`. The download file is still delivered under `--download-dir`.
*   **AT-84** (fail-first). With `--lrs-url` set, a missing or malformed `--consent-state-file` is a startup error that names the file. The host does not invent an `on` default and does not bind a port.
*   **AT-85** (fail-first). An otherwise valid Edge file with no type-71 record opens unrestricted: research-purpose filtering withholds no records; `withheld_count` is 0. This matches `tour.rs` "open / unrestricted (no consent records)".
*   **AT-86** (fail-first). When a type-71 record denies the research purpose, the derived header sheet and type-64 `DeviceIdentityRecord` floors still show device identity fields; event-log payloads are withheld (AT-23).
*   **AT-87** (fail-first). `--bind 0.0.0.0` (or any non-loopback address) plus `--behind-proxy`, even with `--tls-cert` and `--tls-key` present, exits non-zero, names the conflict, and binds nothing. `--behind-proxy` with `--bind 127.0.0.1` starts.
*   **AT-88** (fail-first). `POST /v1/select` of a candidate whose `source_url` is an `egress_test` loopback fixture that responds with a redirect to `http://169.254.169.254/` is blocked at the redirect hop: the second request is not sent. A select whose `source_url` host resolves to `127.0.0.1` (or `::1`) on a port that is not the `--egress-test-loopback-port` is blocked at connect time and no TCP connection is made to that address.
*   **AT-89** (fail-first). If the inclusion-record write (step 1) fails (for example the working archive cannot be locked), no new snapshot file is created in `--snapshot-dir`, `--download-dir` is unchanged (AT-127), no LRS request is made, and the error is returned (`500` `write_failed` or `500` `lock_failed`). A later successful select of the same paper then writes the record and snapshot (the failed attempt is not treated as a successful idempotent artifact).
*   **AT-90** (fail-first). A hyper-tile file that has a seal trailer and fails `verify_sealed_archive` opens with a persistent quarantine banner naming the seal failure. CRC-valid committed tiles may still appear as floors; no floor is presented as a verified seal.
*   **AT-91** (fail-first). After AT-59's LRS 5xx, the retry file exists under `--data-dir/lrs-retry/` and is still there after the host process is stopped and started again with the same flags. `POST /v1/consent` to `off` deletes that directory.
*   **AT-92** (fail-first). At reference scale 1.0, measured integer bay-rectangle sides are exactly 32 CSS px (4 KiB) and exactly 45 CSS px (8 KiB).
*   **AT-93** (fail-first). Two `render_floor_transition` calls with `instant=true` and different `t_seconds` values return byte-identical pixmaps and the same text layer (the end frame). `t_seconds` is ignored.
*   **AT-94** (fail-first). When `--download-dir` already contains a file at `{sha256_hex}` whose contents do **not** hash to that name, `POST /v1/download` of the paper that would use that name fails, does not overwrite that file (byte-identical), and if the inclusion write already succeeded it is kept. Same-bytes re-download is AT-106.
*   **AT-95** (fail-first). Each `render_*` function in §5.1 returns `Result<RenderFrame, KiteError>`. `open` of a byte string that is not an S3A file returns `Err` and the process does not abort.
*   **AT-96** (fail-first). The pack exports `open`, `open_path`, `render_manifest`, `render_page`, `proceed_to_warehouse`, `render_warehouse`, `render_floor_transition`, `render_floor_detail`, `hit_test`, and `paper_identity`. On the AT-08 first view, `hit_test` at the Continue control returns `Continue`; a point on empty space returns `None`. `render_page` is AT-100.
*   **AT-97** (fail-first). A file whose `tile_stride` is 65536 (neither 4096, 8192, nor 128 KiB) is opened via `/v1/open` as HTTP 200 with `kind: "unsupported"` and `first_view: "banner"`; `GET /v1/frame?view=banner` returns a frame whose text layer contains the unsupported-stride banner text; `POST /v1/proceed` returns 409 `unsupported`; the file is not toured as a hyper-tile.
*   **AT-98** (fail-first). `open` / `from_bytes` of the AT-08 fixture succeeds. The pack and host do not depend on a memmap crate for `open_path` (`cargo tree` / source grep on pack-owned code); `open_path` reads the file and uses `from_bytes` (DD-75).
*   **AT-99** (fail-first). The v1 UniFFI surface does not export select, download, skip, consent-toggle, or working-archive write functions. A Kotlin / Swift binding test fails if those functions are present.


### v4 additions (round-3 dispositions)

*   **AT-100** (fail-first). `render_page` and `GET /v1/frame?view=page` exist. Opening a wiki dossier (`papers/SRC-*.md`) or the AT-33 HTML fixture returns `first_view: "page"` and a frame whose text layer contains a heading from the file. A `view` not in the §5.2.1 table is `400` `invalid_view`, not a silent manifest frame and not 404.
*   **AT-101** (fail-first). On a paper dossier page, `hit_test` at the Select control returns `Select` with that paper identity; a call to the pack FFI alone does not write an inclusion record. `hit_test` at a link returns `Link { href }` and the host performs no fetch of that href.
*   **AT-102** (fail-first). `POST /v1/select` of a current candidate with a `source_url` on an `egress_test` loopback fixture performs one GET of that URL (this is the defined fetch trigger). A second select of the same identity (same verb) does not fetch again if the snapshot already exists.
*   **AT-103** (fail-first). If that fetch fails or exceeds `--fetch-timeout-secs` after the inclusion write, the working archive still has the inclusion record, no snapshot file is created, the response includes `fetch_error`, and the index has the identity.
*   **AT-104** (fail-first). Select (and download) of an on-disk-only candidate (a `papers/SRC-*.md` whose `drive_path` canonicalizes inside `--corpus-root`, and which has no `source_url`) writes the inclusion record, creates **no** file in `--snapshot-dir`, and download copies the on-disk bytes to `--download-dir/{sha256_hex}`.
*   **AT-105** (fail-first). After a successful control select, `POST /v1/select` (and download, and skip) with a `paper_id` that is not in the **request-time** candidate set (§5.5) returns `404` `unknown_paper` and writes nothing (archive, snapshot dir, download dir, index, LRS unchanged versus the post-control state). The same 404 is returned if an earlier `GET /v1/candidates` listed that id but the file was removed before this request. A select with no prior GET succeeds if the id is in the set at request time.
*   **AT-106** (fail-first). A second download of the AT-31 fixture, after the `{sha256_hex}` file already exists with identical bytes, returns 200, returns that same path, and leaves the file byte-identical (success no-op).
*   **AT-107** (fail-first). Opening a kite-written BLOB+PROVENANCE snapshot (no `ConsentRecord`) is unrestricted under DD-67: `withheld_count` is 0 and the captured BLOB is viewable. This is the same rule as AT-112, applied to a file in `--snapshot-dir`. DD-63 is retired.
*   **AT-108** (fail-first). A third-party hyper-tile archive that contains withheld records, copied into `--snapshot-dir` under any name (including a name that looks like a SHA-256), is still filtered by the §5.3 item 4 algorithm: those records remain withheld. Sitting in `--snapshot-dir` grants no exemption.
*   **AT-109** (fail-first). An `egress_test` fixture host that is listed in `--egress-allowlist` and that redirects to `http://169.254.169.254/` is still blocked at the redirect hop (the deny list wins). A release build (no `egress_test` feature) rejects `--egress-test-loopback-port` at startup.
*   **AT-110** (fail-first). With consent on, the LRS request is `POST {lrs-url}/xapi/statements` with `Authorization: Basic` from the credentials file, `X-Experience-API-Version: 1.0.3`, and a JSON statement whose `id`, `actor.account`, `verb.id`, `object.id`, `timestamp`, and `EXT_PAPER_ID` match §5.5. A retry after an LRS 5xx resends the same `id`.
*   **AT-111** (fail-first). After AT-59 has queued a retry, the host is stopped, the consent file is `off` (or `POST /v1/consent` to `off` was used), and the host is started again: zero LRS requests are made, and the queue directory is deleted or not sendable.
*   **AT-112** (fail-first). A hyper-tile fixture with zero core `ConsentRecord`s opens unrestricted: `withheld_count` is 0 and records that would be withheld under an active denial are visible. (Distinct from AT-79, which still checks that `create` writes a research-purpose `ConsentRecord`.)
*   **AT-113** (fail-first). `--bind unix:/path.sock` without `--port` starts; `GET /health` over that socket without `--behind-proxy` returns `ok\n`; with `--behind-proxy`, `/health` without a token is 401. Passing `--port` together with `unix:` is a startup error.
*   **AT-114** (fail-first). `--consent-state-file` containing `on` without `--lrs-url` is a startup error that names the file and binds nothing.
*   **AT-115** (fail-first). Without a token, `GET /v1/frame` and `GET /no-such-route` are both 401. With a valid token, `GET /no-such-route` is 404 with the JSON error object.
*   **AT-116** (fail-first). On a Scholar host without `--lrs-url`, `POST /v1/consent` `{ "state": "on" }` returns 409 `lrs_not_configured` and leaves the consent file not `on`.
*   **AT-117** (fail-first). A token file with mode `0644` is refused at startup (names the file, binds nothing). The same refusal applies to `--lrs-credentials-file`, `--tls-key`, and `--pinned-keys-file` when those flags are set (AT-142). After a successful authenticated request and a consent-on LRS emit, `--log-dir`, stdout, and stderr contain neither the UI token secret, nor any `Authorization` header value (incoming Bearer or outgoing Basic), nor the LRS password, nor paper identities, nor `--researcher-id`.
*   **AT-118** (fail-first). With `--max-open-handles 1`, a second `/v1/open` before `/v1/close` returns 429 `too_many_handles`. After close, a further open succeeds. `POST /v1/hit-test` before any `/v1/frame` for that view returns 409 `no_frame`.
*   **AT-119** (fail-first). `POST /v1/close` of a valid handle returns 200; a second close or a close of an unknown handle returns 404 `unknown_handle`.
*   **AT-120** (fail-first). At reference scale 1.0, a 128 KiB hyper-tile bay's measured integer side is exactly 181 CSS px.
*   **AT-121** (fail-first). An HTML fixture under `--view-root` that contains an `<img src="http://…">` and a `<link rel="stylesheet" href="http://…">` is opened with `view=page`: the render issues zero network requests, and those tags are not fetched as subresources.
*   **AT-122** (fail-first). A `source_url` whose DNS is an IPv4-mapped RFC1918 address is denied at connect time. With `HTTP_PROXY` (and `HTTPS_PROXY`) set to an otherwise reachable origin, a denied destination is still not sent through the proxy, and an allowed `egress_test` fixture is reached with **zero proxy hits** (the variables are ignored). A fixture body larger than `--max-fetch-bytes` sets `fetch_error` and writes no snapshot.
*   **AT-123** (fail-first). Covered with AT-55: a `file://` `source_url` is rejected as a fetch.
*   **AT-124** (fail-first). If deleting `--data-dir/lrs-retry/` on revoke fails (the test plants an unwritable directory or injects a delete error), the response is a visible error, consent is `off`, and a subsequent start or retry issues zero LRS requests (the queue is not sendable).
*   **AT-125** (fail-first). A Scholar host started with `--headless` and a loopback bind has no GUI window and still serves the Host HTTP API. A native-window build (not `--headless`) sends the UI token on `POST /v1/select` (a test spy on the HTTP handler sees the Bearer secret); the test passes the required TLS flags when the bind would need them. Starting without `--headless` when `--bind` is `0.0.0.0` is a startup error that names `--headless`. If the test host has no `DISPLAY` (or equivalent), the window half of the test is skipped and the skip is reported.

### v5 additions (round-4 dispositions)

*   **AT-126** (fail-first). N concurrent `POST /v1/select` requests for the same identity against one Scholar host, with a `source_url` on an `egress_test` fixture, produce exactly one tile-10 record and at most one snapshot file (the same `snapshot_sha256`). The write mutex and snapshot temp+rename hold.
*   **AT-127** (fail-first). A `POST /v1/download` whose step-1 inclusion write is forced to fail leaves `--download-dir` byte-identical to the start; no snapshot is created; no LRS request is made (extends AT-89 to step 2b).
*   **AT-128** (fail-first). Select and download of a candidate with neither a usable `drive_path` nor a `source_url` (an `AcademicPaperRecord`, or a dossier with no `drive_path` and no `source_url`) write the inclusion record and index line (`snapshot_sha256` null). Download returns `200` with `path: null` and `download_error: "no_source"`, and `--download-dir` and `--snapshot-dir` are unchanged.
*   **AT-129** (fail-first). A candidate with both a usable `drive_path` and a `source_url` is selected and downloaded: the host copies the on-disk file, writes no snapshot, and issues zero HTTP fetches of `source_url`.
*   **AT-130** (fail-first). Kind detection matches §5.2.5: S3A magic with a version-1 128 KiB hyper-tile → `hyper-tile` / `manifest`; consistent Edge → `edge` / `header`; hint `.md` (no magic) → `wiki` / `page`; hint `.html` or `.htm` → `html` / `page`; hint `.pdf` or no hint (and no magic) → `other` / `page` with the exact label `no text conversion in archive` in the text layer (AT-148). A non-1 S3A version remains AT-69 (`invalid_magic`), not `other`.
*   **AT-131** (fail-first). A viewer-only start that includes every AT-76 flag except `--max-open-handles` is a startup error that names `--max-open-handles` and binds nothing.
*   **AT-132** (fail-first). A viewer-only start that also passes any Scholar-only flag listed in §5.2.2 (`--data-dir`, `--wiki-root`, `--lrs-url`, or `--researcher-id`) is a startup error that names the flag and binds nothing.
*   **AT-133** (fail-first). An Edge file whose type-70 signatures verify against a `DeviceIdentityRecord.public_key` that is **not** in `--pinned-keys-file` (the list is non-empty and holds a different key) opens as **Quarantined**. Re-signing the same file with a second unpinned key is also Quarantined. The embedded key is never treated as a trust anchor.
*   **AT-134** (fail-first). An Edge file with `FLAG_ED25519_SIGNED` set and no type-70 tile (and no usable pinned key that covers the file) opens as **Quarantined**. The banner names the missing signatures.
*   **AT-135** (fail-first). A hyper-tile fixture whose only `ConsentRecord`s have a purpose other than Research, and that has no withdrawn marks, opens unrestricted: `withheld_count` is 0.
*   **AT-136** (fail-first). A hyper-tile fixture with a later research-purpose `ConsentRecord` that withdraws an earlier grant for the same `subject_id` withholds that subject's records (`is_active` is false on the winning record).
*   **AT-137** (fail-first). `open_verified` of a file, given a `KiteVerificationReport` whose `status` or `invalid_tile_indices` disagrees with a fresh `s3a-reader-core` `verify(pinned_keys)` of the same bytes, still uses the supplied report for the banner and the valid floor list (the pack does not recompute).
*   **AT-138** (fail-first). After invalid tiles are omitted from the valid floor list, `render_floor_detail` / `GET /v1/frame?view=floor-detail&floor=` / `Floor(n)` use the tile `index`. A request whose `floor` is not a valid tile index returns `409` `invalid_floor`. The invalid-list banner text is not hit-testable as `Floor(n)`.
*   **AT-139** (fail-first). A `source_url` whose final HTTP status is 404 (error-page body of any size) yields `fetch_error`, writes no snapshot, and does not store the error-page body. The inclusion record from step 1 is kept.
*   **AT-140** (fail-first). A `source_url` that returns `200` with `Content-Encoding: gzip` is snapshotted as the **decoded** body. `snapshot_sha256` is SHA-256 of the decoded bytes. `--max-fetch-bytes` is applied to those decoded bytes (a gzip payload whose decoded size exceeds the cap is `fetch_error` and writes no snapshot). An unknown `Content-Encoding` is `fetch_error` and writes no snapshot.
*   **AT-141** (fail-first). `--max-frame-pixels` is required. `GET /v1/frame` with `w`, `h`, and `device_pixel_ratio` whose pixmap product `ceil(w*dpr) * ceil(h*dpr)` exceeds the cap, or with `w` or `h` equal to 0, or with a non-finite or non-positive `dpr`, returns `400` `invalid_frame_size`.
*   **AT-142** (fail-first). `--lrs-credentials-file` or `--tls-key` with mode `0644`, or owned by another user, is a startup error that names the file and binds nothing. After a host write, `inclusion-index.jsonl` and `lrs-retry/` files under `--data-dir` are mode `0600`. Extends AT-117.
*   **AT-143** (fail-first). At `device_pixel_ratio = 2.0`, a request with CSS `w=100`, `h=50` produces a pixmap of 200 by 100 pixels (`width`/`height` in the JSON, `rgba` length 80000). The product is checked against `--max-frame-pixels`.
*   **AT-144** (fail-first). In viewer-only mode (and on the UniFFI phone surface), `render_page` is called with `select_enabled=false`: the text/semantics layer has no Select control, and `hit_test` at the usual Select location returns `None`, not `Select`.
*   **AT-145** (fail-first). Scholar start with `--snapshot-dir` (or `--data-dir`, `--download-dir`, `--log-dir`, or the working-archive parent) equal to, or a prefix of, `--wiki-root` or `--corpus-root` is a startup error that names both flags and binds nothing.
*   **AT-146** (fail-first). A non-Edge file with `tile_stride` 128 KiB and `FLAG_TILE_SIZE_4KB` (or `FLAG_TILE_SIZE_8KB`) set opens as `kind: "unsupported"`, `first_view: "banner"`, and is not toured as a hyper-tile.
*   **AT-147** (fail-first). An otherwise consistent Edge file with `FLAG_SEAL_CHAIN_ACTIVE` clear opens (not auto-quarantined solely for that flag); the header sheet shows seal status "unsealed".
*   **AT-148** (fail-first). Opening a PDF (or an extensionless file) under `--view-root` returns `kind: "other"`, `first_view: "page"`, and the page text layer contains exactly the label `no text conversion in archive` and invents no title or abstract.
*   **AT-149** (fail-first). `--handle-idle-secs` is required. After a handle is idle for that many seconds (no frame, hit-test, proceed, or close), the next `/v1/frame` or `/v1/hit-test` returns `404` `unknown_handle`.
*   **AT-150** (fail-first). A `drive_path` that is relative is resolved against the dossier's parent directory and then must canonicalize inside an allowed root; a relative path that escapes is not an on-disk source (`on_disk` is false). The dossier `.md` itself is never treated as the on-disk source.

--------------------------------------------------------------------------------


## 8. Draft decisions

Ashley may accept a draft, change it, or leave it open. Draft values follow. DD-01–DD-78 keep their topics except **DD-63, which is retired** in v5 (F-22; use DD-67). Changes of meaning since v4 are noted in the "Notes" column and in the round-4 disposition.

| ID | Topic | Draft value | Notes |
| --- | --- | --- | --- |
| **DD-01** | Pack location | `crates/s3a-pack-kite` in the s3a-storage workspace, with the Kitesurf Rust core vendored from `ashleycribb/s3a-Kite` at commit `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. | Unchanged. |
| **DD-02** | Host compile mode | Link `kitesurf-core` and `renderer-core` with `native-host` as native rlibs. Wasmtime and Boa are not used in v1. | Unchanged in meaning. |
| **DD-03** | Bind address and port | Required flags `--bind` and `--port`, except `--port` is forbidden when `--bind` is `unix:`. No silent production default. Docs may show `127.0.0.1:8787` as an example. | Tightened (F-24). |
| **DD-04** | Visual metaphor | Call the UI a warehouse. Data model remains lobby + floors. Bays map 1:1 to floors. Bay *area* is approximately linear in stride (sides rounded to the nearest integer; DD-70). | Tightened (F-19). |
| **DD-05** | Device-side stored manifest | Not written on the MCU in v1. The hub/PC/phone shows the derived header sheet whose fields are listed in §5.4. | Unchanged. |
| **DD-06** | Slimmed renderer / MCU | v1 uses the same `native-host` + `vello_cpu` renderer on hub, PC, and phone. No second renderer until a measured memory budget on a named device fails. Never on the microcontroller. | Unchanged. |
| **DD-07** | Event colors | Color by severity: 0, 1, and {2, 3} are three pairwise-different colors (draft still: green / yellow / red). Print `event_type` as the EDGE §5.2.2 name, and show the non-color category label in §5.4. Exact RGB is Q-10. | Unchanged. Category labels are now inlined in §5.4. |
| **DD-08** | Pass 1 timer | Show a 15-second skim timer in the native UI. It is client-side. Expiry does nothing to records and the host has no write-on-timer endpoint. | Tightened (F-31). |
| **DD-09** | Snapshot and inclusion vs consent | Snapshots and inclusion records of selected papers are local working-archive writes and do **not** require tracing consent. Skip does not snapshot. Tracing consent gates only LRS xAPI. | Unchanged. |
| **DD-10** | Device transport | v1 opens files already on the hub, PC, or phone. BLE/USB/SoftAP stay Edge Phase 2. | Unchanged. |
| **DD-11** | Host authentication | Fail closed. `--auth-token-file` points at a token that is not in the repository. Unauthenticated requests are 401 except the tokenless health case in §5.2.1. | Tightened (DD-71). |
| **DD-12** | LRS URL | Must be configured (`--lrs-url`) to emit. No invented localhost default. The v3 "consent on and URL missing, statement fails closed" state is unreachable: that combination is a startup error (F-23). | Amended. |
| **DD-13** | Markdown | The pack parses CommonMark with `pulldown-cmark` (raw HTML disabled or escaped) to HTML, then native-host Blitz lays out that HTML. No JS and no remote subresources on wiki/corpus pages. | Unchanged. |
| **DD-14** | PDF pages and `kind: "other"` | v1 does not rasterize PDF input and invents no title or abstract. A file that is not S3A and whose name hint is not `.md`/`.html`/`.htm` is `kind: "other"`, `first_view: "page"`, and `render_page` shows the exact label `no text conversion in archive` (AT-148). | Tightened (F-39). |
| **DD-15** | New tile IDs | None in v1. Snapshots use existing BLOB + PROVENANCE. Inclusion uses Scholar tiles when `ADDON_EDU` is set. The inclusion index is a file under `--data-dir`, not a new tile. | Clarified (F-05). |
| **DD-16** | Addon mask | No new addon bit in v1. Scholar writes require `ADDON_EDU`. A missing bit on an existing working archive is a hard error (§5.6). | Unchanged. |
| **DD-17** | Upstream pin | Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. | Unchanged. |
| **DD-18** | Animation timing | Match `tour_html_template.html`: 0.45s `cubic-bezier(.2,.8,.2,1)` from the bay rect at the current scroll to `0.06w / 0.08h / 0.88w / 0.84h`, skipped when `instant` is set. `1vw = width/100`, `1vh = height/100`. | Tightened (F-20). |
| **DD-19** | App relationship | s3a-kite is the viewer inside `s3a-reader`. Verification stays in `s3a-reader-core`. | Unchanged. |
| **DD-20** | Quarantine UX | A quarantined file can be opened with a persistent banner. Failed-CRC, torn, and chain-broken tiles are not valid floors (§5.4 item 2). Seal-trailer failure is AT-90. | Unchanged. |
| **DD-21** | GPU | v1 stays on `vello_cpu`. No wgpu, Vulkan, Metal, or GLES backend at all in the pack dependency tree. | Unchanged. |
| **DD-22** | Mobile fonts | Ship Noto Sans, Italic, Serif, Mono, and Symbols. `system_fonts` remains false. Color Emoji and CJK SC are optional bundled asset packs, not downloaded. | Unchanged. |
| **DD-23** | Android ABIs and UI | minSdk 26, `arm64-v8a` and `x86_64`, UniFFI + Kotlin, Jetpack Compose. No 32-bit ARM in v1. | Unchanged. |
| **DD-24** | Kotlin deck plan | The kite warehouse is the primary view and the default screen after open. `MapLayout` may remain as a secondary diagnostic screen, not the default. | Unchanged. |
| **DD-25** | iOS | Feasible. Ship Android first. iOS follows when AT-40 stays green. | Unchanged. |
| **DD-26** | Page JS on a native host | **v1 does not run page JavaScript.** HTML is laid out with JS disabled. Wasmtime is not required. There is no general open-by-URL in v1 (DD-59). | Clarified (F-02). |
| **DD-27** | Write target | Inclusion records go to `--working-archive`. Snapshots go to `--snapshot-dir`. The corpus is read-only to kite. | Unchanged. |
| **DD-28** | Write API, seal, lock | `TileWriter::create` / `open_append` + Scholar `create_human_lrs`; exclusive lock; new archives get `ADDON_EDU` and an active research-purpose `ConsentRecord`; snapshots sealed with `seal_archive`. | Unchanged. |
| **DD-29** | Health, token, TLS | Health is 200 `text/plain` `ok\n`; token rules in DD-68; TLS required for non-loopback binds. | Token charset tightened (F-14). |
| **DD-30** | Roots | Roots and working-archive parent canonicalization as in §5.2.2. | Unchanged. |
| **DD-31** | Host-fetch egress | Deny list always applies, at connect time and every redirect hop, including mapped and non-global unicast forms (DD-64, DD-76). | Tightened (F-09, F-29). |
| **DD-32** | Human control | Only the UI token may call write routes. The native window is an HTTP client that sends that token. Custody of the token file remains the control point. | **Amended** (F-04, DD-60). |
| **DD-33** | Tracing consent model | `--consent-state-file`; `--researcher-id`; revoke drops LRS and the retry queue, keeps local records. Retries re-check consent (DD-66). | Tightened (F-11). |
| **DD-34** | Inclusion side effects | Order: local record and index, then fetch/snapshot, then LRS. Failure of step 1 skips 2 and 3. Fetch failure keeps the record. Idempotent per identity string + verb via the index, for successful artifacts only. | Tightened (F-02, F-05). |
| **DD-35** | Missing optional fonts | Absent CJK/emoji packs: open succeeds, `.notdef`, no system fonts, no network. | Unchanged. |
| **DD-36** | Manifest-to-tour trigger | Explicit Continue / `proceed_to_warehouse` / Continue `hit_test` only; no timer. | Unchanged. |
| **DD-37** | Warehouse scale and frame API | The §5.1 functions, including `render_page`. Bay sides 32 / 45 / 181 CSS px (DD-70). `device_pixel_ratio` is on every signature (DD-69). | Amended (F-01, F-18, F-19). |
| **DD-38** | Text and semantics layer | Every frame returns `text_runs` and `semantics` with the schema in §5.8. | Schema now inlined (F-03). |
| **DD-39** | Phone Scholar writes | **Out of v1.** Phone builds are viewers only. | Unchanged. |
| **DD-40** | Inconsistent Edge headers | Quarantine / `kind: "unsupported"`; do not guess a stride. Other strides: AT-97. | Tightened (F-27). |
| **DD-41** | No CDP, no agent tokens | v1 does not reimplement CDP and does not mint or accept an agent token. | Unchanged. |
| **DD-42** | Native UI and Host HTTP API | The native window is optional and is a client of §5.2.1 (DD-60). | Amended (F-04). |
| **DD-43** | `KiteError` | Every fallible pack function returns `Result<_, KiteError>`. | Unchanged. |
| **DD-44** | Page JS off in v1 | No Boa, no Wasmtime, no wasm32 page-isolate ABI in v1. | Unchanged. |
| **DD-45** | Phone is viewer-only | UniFFI exports only open / render / hit-test / `paper_identity`. | Unchanged. |
| **DD-46** | Paper identity and inclusion record | Identity is the `SRC-NNNN` stem or a decimal `paper_id` (DD-61). `object_hash` is CRC32C of the UTF-8 identity (`u32`). `actor_hash` is `hash_iri(--researcher-id)` (`u64`). Verb mapping is Q-07. | **Amended**: actor_hash is no longer CRC32C; idempotency is not by `object_hash` alone. |
| **DD-47** | Snapshot chunking | 40-byte prefix; `blob_key` is SHA-256 of the complete captured bytes. Provenance document is the three-line text in §5.6. | Unchanged. |
| **DD-48** | One researcher, durable disk | One Scholar-mode process serves one `--researcher-id` and one working archive. Durable local disk is required in Scholar mode, not in viewer-only mode (DD-78). Cloud Run is not a v1 target. | Tightened (F-36). |
| **DD-49** | Fetch timeout | Scholar mode requires `--fetch-timeout-secs`. No silent default. | Unchanged. |
| **DD-50** | Consent file writer | Required when `--lrs-url` is set. `on` without `--lrs-url` is a startup error. Host is the only writer. Toggle is `POST /v1/consent`. | Tightened (F-23). |
| **DD-51** | LRS retry queue | Files under `--data-dir/lrs-retry/`. Revoke deletes the directory. Every send re-checks consent (DD-66). Backoff is Q-09. | Tightened (F-11). |
| **DD-52** | `--behind-proxy` | Accepted only with a loopback or Unix-socket bind. With the flag, `/health` also requires the token. | Tightened (F-24). |
| **DD-53** | Consent filtering | No consent records means unrestricted for Edge and hyper-tile files (DD-67). Header sheet and `DeviceIdentityRecord` are exempt. Create still writes a research-purpose `ConsentRecord`. Snapshot exemption is DD-63. | Tightened (F-08, F-12). |
| **DD-54** | Product name | Keep the name `s3a-kite`. Rename is Q-11. | Unchanged. |
| **DD-55** | Required dirs and ephemeral temp | `--log-dir` is required in both modes; `--data-dir` is required in Scholar mode and **not accepted** in viewer-only mode. Ephemeral temp is deleted on exit. | Tightened (F-36). |
| **DD-56** | Download directory | `--download-dir` must exist in Scholar mode. Names are `{sha256_hex}`. Same-bytes re-download is a success no-op. A planted name with different bytes fails and does not overwrite. Delivery (step 2b) runs only after a successful step 1; a failed inclusion leaves `--download-dir` unchanged. The earlier "keep a delivered file if inclusion fails" sentence is deleted as unreachable. | Tightened (F-05). |
| **DD-57** | Edge signature failure | A failed `DeviceSealRecord` signature invalidates the tile at `tile_index` and every subsequent tile, as specified in §5.4 item 2. | Tightened (F-28). |
| **DD-58** | Page render and links | `render_page` and `view=page` are the wiki/HTML API. `hit_test` may return `Select` or `Link`. Links are not auto-fetched. Writes go through the Host HTTP API. | New (F-01). |
| **DD-59** | Source model and fetch trigger | Decision table in §5.5: source_url only → fetch+snapshot; on-disk only → copy, no snapshot; both → prefer on-disk, no fetch; neither → inclusion written, `download_error: "no_source"`. Relative `drive_path` is resolved against the dossier parent. The dossier `.md` is not the on-disk source. No open-by-URL in v1. | Tightened (F-06). |
| **DD-60** | Native window and headless | The window is a Host HTTP API client that sends the UI token. `--headless` starts no window and is required when `--bind` is not loopback and not Unix. The HTTP listener always starts. | New (F-04). |
| **DD-61** | Identity, candidates, index | SRC stem vs decimal `paper_id` are distinct identities (no join). Duplicate SRC paths: wiki-root wins. The SRC glob is exactly `{root}/papers/SRC-*.md` (not recursive). Duplicate `paper_id`s: first sorted `.s3a` path, then first in tile order. The candidate set is recomputed per request. The index stays in `--data-dir` and stores `file_uuid`, canonical archive path, and nullable `snapshot_sha256`. Idempotency is current archive `file_uuid` + identity + verb. | Tightened (F-01, F-02, F-03, F-07, F-35). |
| **DD-62** | Download filename | `--download-dir/{lowercase sha256 of delivered bytes}`. Same bytes: success no-op. Different bytes at that name: fail, no overwrite. | New (F-07). |
| **DD-63** | ~~Kite-written snapshot criterion~~ **Retired** | Retired in v5. A BLOB+PROVENANCE snapshot has no `ConsentRecord`, so DD-67 already leaves it unrestricted. Sitting in `--snapshot-dir` grants no exemption (AT-107, AT-108). | Retired (F-22). |
| **DD-64** | Allowlist vs deny list | The deny list always wins. `--egress-allowlist` never overrides it. Loopback fixtures use the `egress_test` feature and `--egress-test-loopback-port`, rejected in builds without the feature. | New (F-09). |
| **DD-65** | LRS client | HTTP Basic from `--lrs-credentials-file`, `POST {lrs-url}/xapi/statements`, `X-Experience-API-Version: 1.0.3`, statement schema in §5.5. `--lrs-url` is https except loopback http. `--lrs-actor-homepage` is required with LRS. | New (F-10). |
| **DD-66** | Retry re-checks consent | Every LRS send, including startup retry, requires in-memory consent `on`. Consent `off` at start deletes the queue. A failed revoke delete is a visible error and the queue is not sendable. | New (F-11). |
| **DD-67** | Hyper-tile consent filter | The algorithm in §5.3 item 4 (grounded in `consent.rs` / `tour.rs`): latest-wins per `subject_id` for Research only; empty map and no withdrawn marks, including only-non-research records, is unrestricted. Type 82 is never consent. Also covers kite-written snapshots (replaces DD-63). | Tightened (F-12). |
| **DD-68** | Token file charset | One line of ≥32 characters from `A–Z a–z 0–9 _ -`, optional single trailing newline stripped at load. Bearer is the secret without that newline. | New (F-14). |
| **DD-69** | Frame parameter defaults | `device_pixel_ratio` is on every `render_*` signature and on `/v1/frame`. Contract defaults when omitted: `scroll_x=0`, `scroll_y=0`, `instant=false`, `device_pixel_ratio=1.0`. `w` and `h` have no default. This is an explicit exception to the no-defaults rule. | New (F-18). |
| **DD-70** | Bay sides | Integer sides 32, 45, and 181 CSS px at scale 1.0. Area is approximately linear. Mixed-stride warehouses are out of v1. | New (F-19). |
| **DD-71** | Auth status order | Authentication first: 401 without a token (except the tokenless health case); 404 with a valid token on an unknown path. | New (F-21). |
| **DD-72** | Unix-socket bind | `--bind unix:/absolute/path.sock`; `--port` is forbidden. Token still required. | New (F-24). |
| **DD-73** | Host is Unix; token privacy | v1 host is specified for Linux/Unix only. Token file owner is the process euid, mode `0600`. Secret and identifier logging is extended by DD-85. | Tightened (F-18). |
| **DD-74** | Handles | `POST /v1/close`; required `--max-open-handles` ≥ 1; 429 when exceeded. Per-handle serialization; `hit_test` uses the last frame of that handle and view. Idle expiry is DD-92. | Tightened (F-34). |
| **DD-75** | No path mmap in v1 | `open_path` reads the file and uses `from_bytes`. No SIGBUS handling is required. | New (F-32 / F-35). |
| **DD-76** | Fetch size and deny-list completeness | Required `--max-fetch-bytes` (applied to decoded bodies; DD-86). Deny all non-global unicast, including mapped and translated forms, and every IP of the LRS host. Ignore `HTTP(S)_PROXY` / `ALL_PROXY` even for an allowed `egress_test` fixture. | Tightened (F-16, F-24). |
| **DD-77** | No HTML subresource fetches | Remote or local HTML laid out in v1 does not fetch CSS, images, or fonts over the network. | New (F-30). |
| **DD-78** | Viewer data-dir and locked reads | `--data-dir` is not accepted in viewer-only mode. Durable disk is required only in Scholar mode. `/v1/open` of the locked working archive uses a `from_bytes` copy. | New (F-36). |
| **DD-79** | Pinned keys as the Edge trust anchor | A public key embedded in the file is never a trust anchor. Trust is an optional `--pinned-keys-file` (hex 32-byte keys, one per line, euid+0600). Empty or omitted list: at best Self-signed. Non-empty list and signer not in it: Quarantined. Labels: Verified / Self-signed / Unsigned / Quarantined, matching `s3a-reader-core` `verify(pinned_keys)` and vault contact pinning. `FLAG_ED25519_SIGNED` with no type-70/key is Quarantined. | New (F-13). |
| **DD-80** | `open_verified` and `KiteVerificationReport` | Pack type `KiteVerificationReport`; the app maps a reader-core report. If a report is supplied, kite uses it even when a fresh verify would disagree. The pack does not depend on `s3a-reader-core`. `/v1/open` runs the same algorithm with `--pinned-keys-file`. | New (F-14). |
| **DD-81** | `view=banner` | Alias of `render_manifest`. `GET /v1/frame?view=banner` returns the banner / header / manifest text layer. | New (F-09). |
| **DD-82** | `floor_index` is the tile index | `floor_index`, `Floor(n)`, and `?floor=` are the tile `index` after invalid tiles are hidden. Unknown or omitted (where required) is `409` `invalid_floor`. The invalid-list is banner text only. | New (F-15). |
| **DD-83** | Error catalogue | The status/code table in §5.2.1 is normative. Unknown `view` is `400` `invalid_view`. ATs assert the listed codes. | New (F-10, F-26). |
| **DD-84** | Write mutex and numbered steps | One process-wide mutex serializes select, download, skip, and consent end-to-end. Snapshot writes use temp + atomic rename. The archive exclusive lock is held only for step 1. Steps: (1) inclusion+index, (2) fetch+snapshot, (2b) download delivery, (3) LRS. Step-1 failure skips 2/2b/3 and leaves `--download-dir` unchanged. | New (F-04, F-05). |
| **DD-85** | Secrets, TLS store, no-logging | `--lrs-credentials-file`, `--tls-key`, `--pinned-keys-file`, and an existing consent file must be euid+0600. Host-created index, retry-queue, and consent writes are 0600. No logging of Authorization values, LRS passwords, paper ids, source URLs, or `--researcher-id`. LRS HTTPS uses the system / webpki root store; v1 does not pin LRS certificates. | New (F-18). |
| **DD-86** | Fetch 2xx and content encoding | Required `--max-redirects` ≥ 0. Only a final 2xx is stored. The hashed body is decoded `Content-Encoding` (`identity`, `gzip`, `deflate`); unknown encoding or a non-2xx is `fetch_error` and no snapshot. `--max-fetch-bytes` applies to decoded bytes. | New (F-16). |
| **DD-87** | Kind detection and name hint | The ordered table in §5.2.5. `open` / `/v1/open` take an optional name hint (or the path basename). Bad S3A version is an error, not `other`. | New (F-08). |
| **DD-88** | Logs and ephemeral temp | `--log-dir` records startup (flag names only), shutdown, and identifier-free errors. No access log of paper ids. Ephemeral temp is `mkdtemp` 0700 under `$XDG_RUNTIME_DIR` or `/tmp` with prefix `s3a-kite-`; cleaned on exit and of stale same-euid directories at startup. | New (F-31). |
| **DD-89** | Path-rule scope | The inside-root rule applies to request paths, `Link` follow-ups, candidate files, and resolved `drive_path`. Host config and state files (token, TLS, credentials, consent, pinned keys, log-dir, data-dir) are listed separately and need not sit in a request root. | New (F-28). |
| **DD-90** | Frame pixel cap and CSS vs pixmap | Required `--max-frame-pixels` and UniFFI `max_frame_pixels`. `w`/`h`/`x`/`y` are CSS px. Pixmap is `ceil(w*dpr)` by `ceil(h*dpr)`; that product must be ≤ the cap. `dpr` must be finite and > 0. Else `400` `invalid_frame_size`. `TextRun` / `SemanticNode` `x`/`y` are `i32` and may lie off-pixmap. | New (F-17, F-29). |
| **DD-91** | `select_enabled` | `render_page(..., select_enabled)`. True only in Scholar mode when `/v1/select` is available. False in viewer-only mode and on the phone; no Select control is painted. | New (F-30). |
| **DD-92** | Handle idle | Required `--handle-idle-secs` ≥ 1. An idle handle is closed; the next use is `404` `unknown_handle`. | New (F-34). |
| **DD-93** | Disjoint write and read locations | After canonicalization, snapshot, download, data, log, and the working-archive parent are pairwise-disjoint from wiki, corpus, and view roots. A violation is a startup error that names both flags. | New (F-36). |
| **DD-94** | Host fonts | The host embeds the same required UA font bytes with `include_bytes!` and registers them through the same path as mobile. AT-42 applies to the host. | New (F-37). |

--------------------------------------------------------------------------------

## 9. Open questions

*   **Q-01.** Lab host authentication: token file (DD-11, DD-29) or an existing lab SSO? Draft is the token file until Ashley names an SSO.
*   **Q-02.** Exact LRS base URL on the s3a-lrs VM, and the consent-copy shown before tracing.
*   **Q-03.** Whether a later phase should enable phone Scholar writes or phone network for LRS. Draft (DD-39, DD-45): not in v1.
*   **Q-04.** Preferred CommonMark library if not `pulldown-cmark` (DD-13).
*   **Q-05.** Whether the optional CJK and emoji font packs should ship in the first Play/App build or only as additional bundled / Feature Delivery packs.
*   **Q-06.** Named minSdk-26 measurement device, and numeric budgets for peak RSS, time to first manifest frame, and APK size delta versus the current reader. No numbers are invented here.
*   **Q-07.** Which existing verb IRI in `s3a-pack-scholar` `xapi.rs` is used for select and which (if different) for download. This spec will not invent a new IRI. **Draft:** use one existing verb for both, so select-then-download is a single inclusion record (AT-83).
*   **Q-08.** How `--researcher-id` is provisioned in the lab (manual flag, existing Scholar session, or something Ashley names).
*   **Q-09.** LRS retry backoff schedule and a maximum attempt count. No intervals or caps are invented here (DD-51).
*   **Q-10.** Exact RGB (or palette) values for the three severity badge colors in DD-07. AT-19 asserts only pairwise difference plus labels until this is answered.
*   **Q-11.** Whether the `s3a-kite` / "Kite" name should change to reduce trademark overlap with Cloudflare Kitesurf, and if so to what. Draft (DD-54): keep the name until Ashley chooses otherwise.
*   **Q-12.** Whether the optional dossier YAML key for a remote URL should be `source_url` (DD-59 draft) or another name Ashley already uses. The grounded 3Pass tree has no such key today.
*   **Q-13.** Whether `--lrs-actor-homepage` should later be a fixed Scholar IRI rather than a required flag (DD-65).
*   **Q-14.** Timed `cargo-fuzz` duration for CI (seconds or a CI runner budget). The seed corpus and corpus-replay job are required (AT-71); this spec does not invent a timed duration.

Lawyer review is not required for the fork itself (Apache-2.0 + MPL file-level terms are listed in §6), but distribution of the app's font files must keep OFL notices (AT-02, AT-70). Q-11 is a naming question, not a license-term question.

--------------------------------------------------------------------------------

## 10. Risks

1.  **`native-host` is a thin feature flag.** Much of Kitesurf's DOM and JS is `#[cfg(target_arch = "wasm32")]`. The tour and JS-disabled HTML layout path should use paint/layout entry points that already have native-host or non-Wasm coverage (`scene_replay`, `VelloCpuImageRenderer`). Additional native entry points may still be needed in the fork.
2.  **Wasm-oriented dependencies.** Workspace `getrandom` uses `js` / `wasm_js`. `fonts.rs` uses `wasm_bindgen`. Android and iOS builds fail until the pack forks those bits (AT-44), even though Wasmtime and Boa themselves are out of v1.
3.  **Today's `s3a-cli tour` is not a drop-in Edge tour.** It steps 128 KiB tiles and has a type-82 consent *branch*. Kite must use `MmapReader` for all file types, compute `withheld_count` itself, and must not build the warehouse walk on a full `tour_file` output (§11 step 3).
4.  **Type-number reuse** (64, 71, 75) between hyper-tile and Edge layouts. Branching on `is_edge_archive` is mandatory.
5.  **Phone memory and binary size.** Full Stylo + Blitz + UA fonts is heavier than the current Kotlin map. The DD-06 trigger cannot fire until Q-06 is answered.
6.  **No PDF input renderer** in the Kitesurf tree (Krilla is output). Pass 1 can look incomplete for PDF-only papers (DD-14).
7.  **Kitesurf is experimental** (README; last grounded commit 2026-11-16, `235111c`). Upstream will move. The pin is explicit (DD-17).
8.  **Host auth.** There is no CDP port in v1 (DD-41). AT-06, AT-50, AT-51, AT-87, and AT-115 remain gates. Token-file custody is the human-control point, including for the native window (DD-32, DD-60).
9.  **License completeness.** Forgetting NOTICE, MPL source, the third-party inventory, or OFL/IJG texts is a release blocker (AT-02, AT-70). The `s3a-kite` name overlaps "Kitesurf" (DD-54, Q-11).
10. **Working-archive durability and the inclusion index.** Unsealing or locking incorrectly could damage records. If `--data-dir/inclusion-index.jsonl` is lost, a later select may write a duplicate tile-10 record (DD-61 chooses that over silently dropping a real inclusion). AT-46, AT-47, AT-73, and AT-89 are gates.

--------------------------------------------------------------------------------

## 11. Implementation notes (after tests exist and fail)

Order of work, when Ashley authorizes code. Do not push to s3a-Kite, s3a-storage, or s3a-reader until she says so. "Land" means local branches only.

1.  Land the fail-first acceptance tests in s3a-storage so they fail for the documented reason, and land the guard tests so they stay green. Reader tests (AT-38, AT-39, AT-43, AT-63) are feature-gated or in a separate test crate until the reader pin moves off `ff89564`. Names in §5.1 are already fixed, so this step does not wait on "names as implemented."
2.  Vendor the Kitesurf Rust core from `ashleycribb/s3a-Kite` at `235111c`, add LICENSE/NOTICE, enable `native-host`, fix getrandom/wasm_bindgen for native and Android targets, and keep Apache change notices (AT-03).
3.  Implement hyper-tile **and Edge** manifest / header + warehouse paint (text layer, §5.1 / §5.8 animation, `hit_test`, `render_page`, pinned-key verification) using **`MmapReader` for every file type**. Reuse only safe pieces of tour data — stored-manifest bytes and the floor-list fields `index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`. **Do not** feed `tour_file` output into withheld_count or into the unsealed walker (that path includes the type-82 branch and a stride-blind 128 KiB scanner). Compute `withheld_count` in kite with the §5.3 item 4 algorithm. Do not treat an embedded device public key as a trust anchor (DD-79).
4.  Host binary with required flags (including `--max-frame-pixels`, `--max-redirects`, `--handle-idle-secs`), Host HTTP API (including `view=page`, `view=banner`, and `/v1/close`), token, TLS/proxy, health, path rules, secrets mode 0600, and optional `--headless`.
5.  Write path, Scholar 3-pass gates, source-url fetch (2xx, decoded bodies), BLOB/PROVENANCE snapshots (including payloads larger than 128 KiB), inclusion index (`file_uuid` + `snapshot_sha256`), write mutex, consent/LRS client, and host-fetch egress. HTTP snapshots stay in this step; there is no Wasmtime step in v1.
6.  UniFFI + Android Compose viewer surface in `s3a-reader` (tests already landed, feature-gated, in step 1), including `open_verified` and `KiteVerificationReport`; then the iOS `staticlib` link gate. No phone Scholar writes in this step (DD-45).
7.  Land the `open_and_render` fuzz target with a checked-in seed corpus and a corpus-replay CI job (AT-71). Package LICENSE/NOTICE and font OFL notices with the host and mobile artifacts (AT-02, AT-70).

Do not "temporarily" call the old WebAssembly viewer, invent a manifest, walk files with `HYPER_TILE_SIZE`, treat type 82 as Edge consent, gate local inclusion on tracing consent, invent a new `TileType` or verb IRI, stand up a CDP server, or add Wasmtime "just in case."

--------------------------------------------------------------------------------

## 12. Document history

*   2026-11-16: v1. Tests-first pack spec from Ashley's 2026-11-16 direction, grounded in Kitesurf `4a29038`, s3a-storage `b7bdcda`, and s3a-reader `792742c`.
*   2026-11-16: v2. Rebases the Kitesurf pin to `235111c`. Addresses goldfish round 1 (F-01–F-38). Acceptance tests: AT-01–AT-74. Draft decisions: DD-01–DD-40. Open questions: Q-01–Q-08.
*   2026-11-16: v3. Addresses goldfish round 2 (F-01–F-42). Narrows v1: no CDP / agent tokens, no page JS / Wasmtime, no phone Scholar writes, no Cloud Run. Acceptance tests: AT-01–AT-99. Draft decisions: DD-01–DD-57. Open questions: Q-01–Q-11.
*   2026-11-16: v4. Addresses goldfish round 3 (F-01–F-36). Adds `render_page` / `view=page`, the source-url fetch model, the inclusion index, the LRS statements client, and inlines every rule that v3 left as "as in v2." This document is self-contained. Acceptance tests: AT-01–AT-125. Draft decisions: DD-01–DD-78. Open questions: Q-01–Q-13.
*   2026-11-16: v5. Addresses goldfish round 4 (F-01–F-40). Narrows AT-61 so the inclusion index may keep identities in `--data-dir`. Stores `file_uuid` and `snapshot_sha256` in the index, serializes writes, numbers the download step, pins Edge device keys, and adds the error catalogue, `view=banner`, kind detection, frame-size cap, and explicit ConsentRecord filter. Acceptance tests: AT-01–AT-150. Draft decisions: DD-01–DD-94 (DD-63 retired). Open questions: Q-01–Q-14.

--------------------------------------------------------------------------------
