# s3a-kite: S3A Browser Pack and Viewer

*   **Author:** Grok Bot (elephant) for Ashley Cribb
*   **Status:** v3 — spec only. Ashley has forked Kitesurf as [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) (`main` at `235111c6fe10f2d63f3476bf5e3d19b4c7089415`). This document does not authorize implementing the pack, or pushing to that fork or to s3a-storage / s3a-reader, until Ashley says so.
*   **Date:** 2026-11-16 (America/New_York)
*   **Ashley direction (chat, 2026-11-16):** unchanged from v2: s3a-kite is the S3A viewer (hub, PC, or phone, not the microcontroller); first use is Scholar's 3-pass wiki browser; local inclusion records must not depend on tracing consent; xAPI to the LRS is consent-only; no money or crypto in the S3A core. Independent review rounds continue before any push.
*   **Grounded commits (read, not modified):** same as v2 — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`).
*   **Reviews:** round 1, `/workspace/s3a-kite/reviews/round1-disposition.md`; round 2, `/workspace/goldfish/goldfish_s3a-kite_round2.md` (42 findings, F-01–F-42), dispositions in `/workspace/s3a-kite/reviews/round2-disposition.md`.

This is a tests-first pack contract. Fail-first tests must fail on today's trees; guard tests are tagged in §7 and are not claimed to fail first. No stubs, fallbacks, or invented runtime defaults. Anything that needs Ashley's judgment is a **DD** with a draft value, or an open question. Existing DD numbers keep their topic; meaning changes are called out in §8.

v3 narrows v1 scope where a full design would have been a new engine: **no CDP and no agent tokens**, **no page JavaScript / Wasmtime**, **no Scholar writes on the phone**, and **Cloud Run is not a v1 target**. Those return only if Ashley accepts a later DD.

--------------------------------------------------------------------------------

## 1. Overview

`s3a-kite` is an S3A **add-on pack** that forks Kitesurf's Apache-2.0 Rust core (vendored from [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) at the pin above) and replaces Kitesurf's Cloudflare TypeScript host with a native host and a UniFFI library inside the S3A mobile app.

s3a-kite **is** the S3A viewer. It reads `.s3a` and Edge files with `s3a-engine` / `s3a-edge` and paints the manifest or header sheet and the warehouse tour with Kitesurf's Rust layout and CPU rasterizer. On a hub or PC, the `s3a-kite` process shows a **native window** that calls the pack API directly. A first-party **Host HTTP API** (§5.2.1) is the wire contract for tests and for that UI's remote-equivalent; it is not a Chrome DevTools server. The phone app, in v1, is a viewer only.

First product use, on the hub or PC: Scholar's wiki browser for Keshav's 3-pass method. Pass 1 is a human skim of about 15 seconds and is never recorded. Selecting or downloading a paper is the inclusion decision and is written to the researcher's working archive even when tracing consent is off. Tracing is consent-only xAPI to `s3a-lrs-server`. AI and the browser only check that things exist and record what the researcher did; humans make the judgments. Custody of the UI token file is the human-control point (DD-32).

S3A core stays database-only. No money and no cryptocurrency. Everything lives in the pack.

A v1 host process serves **one researcher**, requires **durable local disk**, and runs on an Aura Rack, the LRS VM, or a lab PC (DD-48). Cloud Run is not a v1 target.

--------------------------------------------------------------------------------

## 2. Glossary

| Term | Meaning in this spec |
| --- | --- |
| Pack | A domain crate that extends S3A without changing `s3a-core`. See `docs/pack-contract.md`. |
| Kitesurf core | Rust crates `packages/core` (`kitesurf-core`), `packages/renderer-core`, and `packages/kitesurf-cli`, kept and forked. |
| Cloudflare layer | TypeScript `packages/engine` plus `wrangler.jsonc`, the Worker Loader binding `LOADER`, and Dynamic Workers. Replaced, not required at runtime. Upstream CDP also lives in this layer and is **not** reimplemented in v1 (DD-41). |
| Host | The `s3a-kite` process for one researcher on a hub, Aura Rack, LRS VM, or lab PC. |
| Scholar mode | Host started with the Scholar flag set in §5.2.2. Enables the 3-pass wiki browser and writes. |
| Viewer-only mode | Host started with `--viewer` and `--view-root`. Opens files under that root. No Scholar writes, no LRS. |
| Mobile library | The same Rust renderer, exposed through UniFFI from crate `s3a-pack-kite`, linked into the S3A Android (and later iOS) app. v1 phone builds are viewers only. |
| Hyper-tile archive | A standard S3A file whose `tile_stride` is 128 KiB (the default when the header does not declare an Edge stride). |
| Edge archive | An S3A file with `FLAG_PROFILE_EDGE` and a consistent `tile_stride` of 4096 or 8192. Still a valid S3A file (EDGE profile §1.2). A file whose stride is neither 4096, 8192, nor 128 KiB, or whose Edge flags disagree with the stride, is opened as inconsistent / unsupported (AT-66, AT-97), not as a hyper-tile. |
| Micro-tile | One 4,096-byte or 8,192-byte Edge tile (`MICRO_TILE_4KB_SIZE` / `MICRO_TILE_8KB_SIZE`). |
| Lobby | `TourLobby` in `crates/s3a-cli/src/tour.rs`. |
| Floor | One tile in the tour directory (`TourFloorItem`). |
| Warehouse tour | Ashley's name for the animated walk of those floors (DD-04). |
| Stored manifest | The UTF-8 payload of `TileType::MANIFEST` (78). |
| Derived header sheet | For Edge files (no tile 78 in the Edge profile), a labeled plain-text readout of present `FileHeader` and `DeviceIdentityRecord` fields. |
| Pass 1 | The researcher's own skim, about 15 seconds per paper. Never recorded. |
| Inclusion | The researcher selects or downloads a paper for Pass 2/3. Written to the working archive regardless of tracing consent. The inclusion record is a `HumanLrsRecord` (Scholar tile 10). |
| Paper identity | The stable string used as the idempotency key's paper field: the `SRC-NNNN` stem of a `papers/SRC-*.md` file, or the decimal `paper_id` of an `AcademicPaperRecord`, or the source URL if the paper has neither (DD-46). |
| Tracing consent | Permission to emit xAPI to the LRS. Gates only LRS traffic. |
| Working archive | A per-researcher live `.s3a` file that receives inclusion records. Not the corpus. |
| Text layer | Structured text and semantics returned with every frame. Not OCR. |
| Host HTTP API | The v1 wire contract in §5.2.1. Not CDP. |

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
*   Boa is used to run page `Runtime.evaluate` expressions in the core Wasm (`packages/core/Cargo.toml`). Page `WebAssembly.*` is shimmed in `packages/page-script/src/globals.ts`. **s3a-kite does not use Boa or that shim to run an old S3A viewer.** Opening a `.s3a` file does not instantiate the page JavaScript runtime. Boa remains relevant only for ordinary `http(s)` pages the Scholar browser may open, under the model in §5.2 and DD-26.
*   Upstream CDP lives in TypeScript under `packages/engine/src/cdp/` (modules include `page.ts`, `runtime.ts`, `dom.ts`, `network.ts`, `input.ts`, `target.ts`, `debugger.ts`, `accessibility.ts`, `fetch.ts`, and others).

### 4.2 S3A tour and manifest

*   `s3a-cli tour [--json] [--floor N] [--slice] [--cross] [--history] <archive>` and `s3a-cli tour --html <archive> <out.html>` are implemented in `crates/s3a-cli/src/tour.rs`, `tour_html.rs`, and `tour_html_template.html`.
*   `tour_file` builds a `TourReport` with a `TourLobby`. If a floor has `type_id == TileType::MANIFEST` (`s3a-core`: `MANIFEST = 78`), the UTF-8 payload is `lobby.manifest`. Plain-text output prints `--- PLAIN-TEXT MANIFEST ---` then that payload (`print_tour_report`). The test `test_tour_displays_manifest_in_lobby_and_floor` expects `THIS IS AN S3A FILE` and the seal title/creator.
*   Default mode lists the floor directory. `--floor N` is floor detail (schema fields, records, `withheld_count`). Consent: records without active `purpose=research` are withheld (`tour.rs` header comment and `evaluate_consent_summary`). All kite views of archive records apply this research-purpose filtering. v1 has no unfiltered or staff view mode.
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

### 4.7 Facts used in v3 dispositions

*   `s3a-pack-clean` `archive::chunk_tiles` / `unchunk` prefix each BLOB or PROVENANCE tile payload with a 32-byte key, a little-endian `u32` chunk index, and a little-endian `u32` total, then the chunk bytes (`crates/s3a-pack-clean/src/archive.rs`). The core registry describes the same 40-byte field layout for tiles 68 and 72 (`docs/spec/registry.md`). Hyper-tile payload size is `HYPER_TILE_SIZE - HYPER_TILE_HEADER_SIZE` (131,072 − 512).
*   `AcademicPaperRecord.paper_id` is a `u64` described as a hash of DOI / URI. `HumanLrsRecord.object_hash` is a `u32` "hash of target artifact / paper / claim". 3Pass dossiers live at `wiki/papers/SRC-XXXX.md`.
*   `tour.rs` `evaluate_consent_summary` returns status `"open / unrestricted (no consent records)"` when there are no consent records, no withdrawal marks, and no Edge consent value. That is the no-type-71 behavior v3 follows.
*   `kitesurf-cli` downloads a Wasm scene rasterizer for playback; that download path is not used in v1 kite (DD-44). `boa_eval.rs` / `js.rs` remain wasm32-only; v1 does not invent a Wasmtime import ABI for them.

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

FFI / pack API (every `render_*` returns `Result<RenderFrame, KiteError>`):

| Function | Role |
| --- | --- |
| `open(bytes) -> Result<Handle, KiteError>` | Open an archive or document already loaded as bytes (mobile SAF and host both use this, or the path form below). |
| `open_path(path) -> Result<Handle, KiteError>` | Open a filesystem path the caller has already authorized. |
| `render_manifest(handle, width, height, scroll_y)` | First view (manifest, labeled absent state, or Edge header sheet). |
| `proceed_to_warehouse(handle)` | Leave the first view. No-op (not an error) if already in the warehouse. |
| `render_warehouse(handle, width, height, scroll_x, scroll_y)` | Warehouse floor grid at the given scroll offset. |
| `render_floor_transition(handle, floor_index, t_seconds, width, height, instant)` | Floor-to-detail animation for `floor_index`. |
| `render_floor_detail(handle, floor_index, width, height, scroll_y)` | Floor N records, including `withheld_count`. |
| `hit_test(handle, view, x, y) -> Result<Hit, KiteError>` | Map a point in the last frame of `view` to `Continue`, `Floor(n)`, `Select`, or `None`. |
| `paper_identity(handle) -> Option<String>` | Paper identity when the handle is a paper/wiki page, else none. |

v1 UniFFI on the phone exports the open/render/hit-test functions only. It does not export select, download, or write.

The pack vendors the Kitesurf Rust core from [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) at `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. The TypeScript engine, Wrangler, Worker Loader, and TypeScript CDP are not runtime dependencies.

`s3a-core` MUST NOT depend on this pack. The pack depends on `s3a-engine`, `s3a-edge`, `s3a-pack-scholar`, and the forked Kitesurf crates.

**Test locations and pins.** Pack tests (AT-01–AT-37, AT-41, AT-42, AT-44–AT-62, AT-64–AT-97 except the reader-only tests below) live in the s3a-storage workspace. AT-38, AT-39, AT-43, and AT-63 live in s3a-reader and are **feature-gated or in a separate test crate that does not build in the default reader CI job** until the reader pin moves off `ff89564` to a git revision of s3a-storage that contains `s3a-pack-kite`. AT-40 lives in s3a-storage as the iOS link gate. Path dependencies are allowed only in local developer checkouts. "Land" means local branches; nothing is pushed until Ashley says so. Android/iOS ATs run only on a job that has the NDK or Apple toolchain; they are skipped, not failed, on a host-only CI job, and the skip is reported.

### 5.2 Native host (hub, Aura Rack, LRS VM, lab PC)

The host process:

1.  Compiles `kitesurf-core` and `renderer-core` with feature `native-host` and links them as native `rlib`s (DD-02). Wasmtime is not used in v1.
2.  Requires the flags in §5.2.2 for the chosen mode. Missing required flags are an error in every mode. There is no silent port or bind default (DD-03).
3.  Rejects unauthenticated HTTP on every route except loopback `GET /health` (DD-11, DD-29). There is no CDP listener in v1 (DD-41).
4.  For `http(s)` **resource fetch** (a snapshot or download of a remote source): the host itself performs the HTTP request, applies the egress policy in §5.2.4, and stores the raw response body. It does not start Boa, Wasmtime, or a page-script isolate. HTML may be laid out for viewing with native-host Blitz / `vello_cpu` **with JavaScript disabled** (DD-44).
5.  For `.s3a` and Edge files, and for wiki Markdown: the host calls the pack API in §5.1 and paints `RenderFrame`s. No Boa, no Wasmtime, no page-script.

The human UI is a **native window** in this process that calls the pack API directly (DD-42). The Host HTTP API in §5.2.1 is the testable wire contract that performs the same operations.

One host process serves one `--researcher-id` and one working archive (Scholar mode). Durable local disk is required (DD-48).

#### 5.2.1 Host HTTP API (DD-42)

All routes below are the complete v1 HTTP surface. AT-06 is "every route in this table, other than loopback `GET /health`, rejects a missing or invalid token." There is no CDP upgrade.

JSON uses UTF-8 and `Content-Type: application/json`. Errors are `{ "error": "<code>", "message": "<text>" }` with a 4xx or 5xx status. `<handle>` is an opaque string minted by `/v1/open`.

| Method | Path | Token | Request | Success |
| --- | --- | --- | --- | --- |
| `GET` | `/health` | None on a loopback bind; UI token on a non-loopback bind | (none) | `200`, `Content-Type: text/plain`, body exactly `ok\n` |
| `POST` | `/v1/open` | UI | `{ "path": "<path>" }` | `200` `{ "handle": "<id>", "kind": "hyper-tile"\|"edge"\|"wiki"\|"html"\|"other", "first_view": "manifest"\|"header"\|"page" }` |
| `POST` | `/v1/proceed` | UI | `{ "handle": "<id>" }` | `200` `{ "ok": true }` |
| `GET` | `/v1/frame` | UI | Query: `handle`, `view` (`manifest`\|`warehouse`\|`floor-transition`\|`floor-detail`), `w`, `h`, `scroll_x` (default 0), `scroll_y` (default 0), `floor` (required for floor views), `t` (seconds, floor-transition), `instant` (`true`\|`false`, default `false`) | `200` `{ "png_base64": "<png>", "width": <w>, "height": <h>, "text_runs": [...], "semantics": [...] }` |
| `POST` | `/v1/hit-test` | UI | `{ "handle", "view", "x", "y" }` | `200` `{ "hit": "continue"\|"floor"\|"select"\|"none", "floor": <n or null> }` |
| `GET` | `/v1/candidates` | UI | (none) | `200` `{ "papers": [ { "id": "<paper identity>", "title": "<from the file or record, or empty>" } ] }` — Scholar mode only; `409` in viewer-only mode |
| `POST` | `/v1/select` | UI | `{ "paper_id": "<paper identity>" }` | `200` `{ "ok": true, "snapshot": "<absolute path or null>" }` — Scholar mode; `409` in viewer-only |
| `POST` | `/v1/download` | UI | `{ "paper_id": "<paper identity>" }` | `200` `{ "ok": true, "path": "<absolute path in --download-dir>" }` — Scholar mode; `409` in viewer-only |
| `POST` | `/v1/skip` | UI | `{ "paper_id": "<paper identity>" }` | `200` `{ "ok": true }` — Scholar mode; writes nothing; `409` in viewer-only |
| `GET` | `/v1/consent` | UI | (none) | `200` `{ "state": "on"\|"off", "timestamp": "<RFC3339 seconds UTC or null>" }` — Scholar mode; `409` in viewer-only |
| `POST` | `/v1/consent` | UI | `{ "state": "on"\|"off" }` | `200` with the same body as GET after the host writes the consent file |

Any other path returns `404` with the error object (and still requires a token except loopback `/health`). `POST /v1/consent` is UI-token only; v1 has no agent token that could call it (DD-41).

#### 5.2.2 Modes, flags, and paths

**Viewer-only mode** (`--viewer`): required flags are `--bind`, `--port`, `--auth-token-file`, `--view-root`, `--log-dir`. Optional: `--data-dir` (if omitted, the host uses no durable data dir and keeps only ephemeral temp). Select, download, skip, and consent routes return `409` with error code `viewer_only`. The process opens paths only under `--view-root`.

**Scholar mode** (no `--viewer`): required flags are `--bind`, `--port`, `--auth-token-file`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--researcher-id`, `--fetch-timeout-secs`. If `--lrs-url` is set, `--consent-state-file` is also required. `--view-root` may be added; if set, it is also an allowed open root.

`--working-archive` names a file path. At startup the host canonicalizes its **parent directory** (the parent must exist); the file itself may be missing. The first Scholar write creates it with `TileWriter::create` and `enable_addon(ADDON_EDU)` (AT-79). `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--wiki-root`, `--corpus-root`, and `--view-root` must exist as directories at startup (the host does not create them). Each existing directory is canonicalized with `realpath`. A missing or non-canonicalizable required path is a startup error that names the flag.

After start, a path the host opens must canonicalize to a location strictly inside an allowed directory, or be exactly the working-archive path (once created). Allowed directories are the configured roots above, including `--snapshot-dir` and `--download-dir` in Scholar mode. `..` segments, symlink escapes, and `file://` URLs outside those directories are rejected.

**Health, tokens, TLS (DD-29, DD-52).** As in v2, with these tightenings:

*   `GET /health` success is exactly HTTP 200, `Content-Type: text/plain`, body `ok\n`.
*   Token comparison is constant-time on the **raw file bytes**. The file is not trimmed. It must contain at least 32 bytes; longer files are used in full. Encoding is not required to be UTF-8. Unix permissions must be `0600`. Empty, missing, or short files are a startup error that names the file (AT-45).
*   A non-loopback `--bind` requires `--tls-cert` and `--tls-key`. `--behind-proxy` is accepted **only** with a loopback bind or a Unix-socket bind (AT-87). Non-loopback plus `--behind-proxy` is a startup error. There is no plaintext bearer token on a non-loopback interface in a supported bind.

#### 5.2.3 Consent file (DD-50)

`--consent-state-file` is required when `--lrs-url` is set, and forbidden to be the only consent mechanism when LRS is unset: if `--lrs-url` is absent, tracing is off and the consent file, if the flag is still passed, must start as `off` or be absent (absent means off). When the flag is required, a missing or malformed file is a **startup error** (no invented `on` default).

The file format is unchanged: the exact UTF-8 tokens `on` or `off`, a newline, and an RFC3339-seconds-UTC timestamp of the last change.

The host is the only writer. It writes with a temp file plus atomic rename in the same directory. `POST /v1/consent` is the only supported toggle. Mid-session outside edits are not honored (the host keeps the last state it wrote or read at startup). AT-58 uses `POST /v1/consent`, not an outside editor.

#### 5.2.4 Egress for host fetches (DD-31, DD-49)

v1 has no page subprocess. Egress rules apply to the host's own snapshot and download fetches:

*   Deny at **connect time on the resolved address**, and again on every redirect hop: loopback (IPv4 and `::1`), IPv4-mapped IPv6 loopback, `0.0.0.0`, RFC1918, RFC4193, link-local, CGNAT `100.64.0.0/10`, metadata (`169.254.169.254`, `metadata.google.internal`), and the configured LRS host plus every IP it resolves to.
*   Both `http` and `https` are allowed to addresses that are not denied, unless `--egress-allowlist` is set, in which case only those hosts are allowed (tests for a local fixture server use the allowlist).
*   A denied connect does not send the request.
*   Per-fetch wall-time limit is the required `--fetch-timeout-secs` flag (DD-49). There is no silent default. There is no page-process CPU/memory limit in v1 because there is no page process.

--------------------------------------------------------------------------------

### 5.3 Opening a hyper-tile `.s3a` file

1.  Open with `open` / `open_path` (`MmapReader` or `MmapReader::from_bytes`). If it is not an S3A file, fail with engine `Invalid S3A file magic`, or on mobile the existing `This is not an S3A file`. If `FileHeader.version` is not 1, fail with `Unsupported file version: {n}`. If `tile_stride` is set and is neither 4096, 8192, nor 128 KiB, fail or quarantine as an unsupported stride (AT-97); do not treat it as a 128 KiB hyper-tile.
2.  **First view** is `render_manifest(handle, width, height, scroll_y)`. Tile-78 rules are unchanged from v2 (first committed valid UTF-8 tile 78; labeled absent / quarantined / not-UTF-8 states; wrap to `width` and scroll with `scroll_y`; no invented text).
3.  The first view stays until `proceed_to_warehouse(handle)` or a Continue hit (`hit_test` on the first view). There is no timer auto-advance. Then `render_warehouse` is used.
4.  Floor N detail is `render_floor_detail`. `withheld_count` uses core `ConsentRecord` (and marks), never type 82. Golden fixtures are the oracle when type 82 is present.
5.  Archive bytes are painted as text, never executed as HTML or JavaScript.
6.  Animation is `render_floor_transition` (§5.8). Seal verification uses `s3a-engine`. Unsealed hyper-tile files may be opened; the first view shows seal status. If `verify_sealed_archive` fails on a file that has a seal trailer, the file opens with a persistent quarantine banner; CRC-valid committed tiles may still be listed as floors, but the banner reports the seal failure and no floor is presented as a verified seal (AT-90). The host must not use the CLI's 128 KiB unsealed scanner on Edge files.
7.  The portable open path is `from_bytes` (a snapshot of the file). Optional Unix path-mmap is allowed; a fault because the file changed under the mapping is reported as `KiteError`, and the process must not abort. There is no extra size cap beyond the engine and the OS.

### 5.4 Opening an Edge micro-tile file

1.  Detect with `MmapReader::is_edge_archive()`. Inconsistent flags (v2 AT-66) still quarantine. `tile_stride` is not silently fixed.
2.  Verification is performed only by `s3a-engine`. On mobile, `s3a-reader-core` runs first and is that engine result; kite consumes the `VerificationReport`. **Invalid tiles** (torn, CRC-failed, chain-broken) follow the v2 rule: omitted from the valid floor list; tiles before a chain break that verified remain valid; the breaking tile and every subsequent tile are not valid floors. Edge Ed25519 signatures live in type 70 and cover the seal chain: a failed signature invalidates the signed chain from that point (the type-70 tile and all tiles whose `seal_hash` the signature was meant to cover), not only the type-70 tile itself (DD-57).
3.  **First view:** derived header sheet as in v2, via `render_manifest`. Device identity fields on the header sheet and on type-64 identity floors are **exempt** from research-purpose record filtering: they identify the file and the device, not a consent-gated payload record (DD-53). Missing identity still gets a labeled note.
4.  Warehouse and offsets as in v2, via `render_warehouse`, scaled per §5.8.
5.  Type 64 event logs, category labels, and DD-07 colors as in v2. Unknown layouts as in v2.
6.  Edge consent uses type 71, not type 82 and not core `ConsentRecord`. If the file has **no** type-71 record, research-purpose filtering does not withhold records: the tour's own "open / unrestricted (no consent records)" behavior applies (AT-85). When a type-71 record exists and does not allow the research purpose, event-log payload records are withheld and `withheld_count` is reported (AT-23).
7.  No transcoding is required to view. The microcontroller does not link kitesurf.

### 5.5 Scholar wiki browser (3-pass) — hub and PC only

Phone v1 does not implement this section (DD-45).

*   Render wiki Markdown from under `--wiki-root` / `--corpus-root` as in v2: `pulldown-cmark` with raw HTML disabled or escaped, no JS, no remote subresources, missing pages reported as missing.
*   **Candidate list** (`GET /v1/candidates`): every regular file matching `papers/SRC-*.md` under those roots (the `SRC-NNNN` stem is the paper identity), plus every live `AcademicPaperRecord` in the corpus archive (identity is the decimal `paper_id`). The list is a directory and record check; building it twice writes nothing (AT-24).
*   Pass 1 list and skip write nothing. The 15-second timer may be shown; expiry does not change records (DD-08).
*   Select or download always writes a `HumanLrsRecord` (tile 10) to the working archive when §5.6 preconditions hold, and writes a snapshot when bytes are captured, without tracing consent (DD-09). Download also copies the captured bytes to `--download-dir` (DD-56). If the destination name already exists, the download fails and does not overwrite; the inclusion write, if it already succeeded, is kept. If the inclusion write fails, the download file is still delivered when the bytes are in hand, and the error is shown (F-21).
*   Tracing consent on/off, LRS fail-closed, and "zero LRS requests when off" are unchanged from v2, except consent toggling is only `POST /v1/consent` (DD-50).
*   Only a UI-token `POST /v1/select` or `POST /v1/download` creates an inclusion record (DD-32). "Human select" means "a call authenticated with the UI token"; custody of that token file is the human control. There is no extra gesture bit, and v1 has no agent token and no CDP Input (DD-41). AT-28 tests an unauthenticated call.
*   Pass 1 list and skip never write paper ids or URLs into `--log-dir` or `--data-dir`. Render caches and per-request temp files live in an ephemeral directory that is deleted when the process exits (not under `--data-dir` or `--log-dir`). AT-24/AT-25/AT-61 cover `--log-dir` and `--data-dir` only; they do not require byte-identity of the ephemeral dir, but that dir must not survive process exit.

**Inclusion record (DD-46).** Tile 10 `HumanLrsRecord`: `timestamp_sec` is the write time (Unix seconds); `actor_hash` is computed from `--researcher-id` with the same hashing already used for `HumanLrsRecord` actors in the Scholar pack when one exists, otherwise CRC32C of the UTF-8 `--researcher-id` (the pack's tests often use literal hashes; kite must be consistent and documented in the pack once implemented); `verb_id` is the id of the existing `xapi.rs` verb chosen in Q-07; `object_hash` is CRC32C of the UTF-8 paper identity; `decision_score` is 0; `ai_trace_coord` is the default / empty coordinate; flags are `FLAG_ACTIVE`. The xAPI statement, when consent is on, uses that same verb IRI and uses the source URL as the object id when the paper has a URL, otherwise the paper identity string (no new IRI host is minted).

**Idempotency (DD-34, Q-07).** Key: working archive + paper identity + verb id. Selecting the same paper twice with the same verb writes one record and one successful snapshot. The v3 draft for Q-07 is **one existing verb for both select and download**, so select-then-download is also one record. If Ashley later picks two verbs, select-then-download writes two records; AT-83 follows the accepted mapping.

**Side-effect order (DD-34).** (1) local inclusion record, (2) snapshot if bytes were captured, (3) LRS statement if consent is on. Failure of step 1 skips steps 2 and 3 (AT-89). Idempotency applies only to **successful** artifacts: a failed snapshot may be retried by a later select/download of the same paper. The retry trigger is that same UI call.

**Retry queue (DD-51).** Queued LRS statements live on disk under `--data-dir/lrs-retry/` and survive a host restart. Revoke (`POST /v1/consent` to `off`) deletes that directory. Backoff and a maximum attempt count are Q-09; until Ashley sets them, the host retries at process start and after each later successful LRS emit, and surfaces a visible error after a failed attempt without dropping the queue (no silent skip).

### 5.6 Write path and sealed snapshots

**Target (DD-27).** Inclusion records go to `--working-archive`. Snapshots go in `--snapshot-dir` as `{lowercase hex SHA-256 of the captured bytes}.s3a`. The corpus and wiki are read-only. The snapshot directory is an allowed open path, so AT-31's re-open is allowed (F-05).

**API and locking (DD-28).** Unchanged: `TileWriter::create` / `open_append`, exclusive lock, `ADDON_EDU` on create, fail closed if an existing file lacks the bit. On `create`, kite also writes one active core `ConsentRecord` with `purpose = Research` and `scope = 0` (all), `granted_by` derived from `--researcher-id`, so the owner's records are visible under the global research-purpose filter (DD-53, AT-86). Snapshots are exempt from that filter when opened as kite-written snapshot files in `--snapshot-dir` (they carry the captured source, not third-party subject records).

**Snapshot encoding (DD-47).** Captured bytes are stored in core BLOB tiles (68) using the 40-byte prefix in `docs/spec/registry.md` / `s3a-pack-clean` `chunk_tiles`: `blob_key` (32-byte SHA-256 of the **complete** captured bytes), little-endian `chunk_index`, little-endian `total_chunks`, then chunk data. Each tile payload is at most `HYPER_TILE_PAYLOAD_SIZE` bytes, so each chunk is at most `HYPER_TILE_PAYLOAD_SIZE - 40` bytes. Reassembly concatenates chunks `0 .. total_chunks-1` of the same key. The "payload hash" in AT-31 is that SHA-256; it must equal `blob_key` and SHA-256 of the reassembled bytes. A payload larger than 128 KiB is required in AT-82.

PROVENANCE tiles (72) use the same chunk layout. The reassembled provenance **document** (after the 40-byte prefix) is UTF-8 text containing exactly three facts, one per line: `sha256=<lowercase hex>`, `url=` plus the source URL or empty, and `captured=<RFC3339 seconds UTC>`. The 40-byte chunk header is the tile layout, not extra snapshot metadata. Kite seals with `seal_archive` (no seal-generated tile-78 requirement). If a seal manifest is later added, it is the archive's ordinary stored manifest and is not counted against AT-32.

On-disk wiki or corpus sources are not re-snapshotted; the inclusion record's paper identity points at that existing path (the path is not copied into `--snapshot-dir`).

Skip does not snapshot and does not write an inclusion record.

### 5.7 Mobile build (Android, then iOS)

s3a-kite is the **viewer** inside [s3a-reader](https://github.com/ashleycribb/s3a-reader). v1 phone builds do not offer Scholar candidate lists, select, download, LRS, or working-archive writes (DD-45). Those stay on the hub/PC host.

The feasibility of CPU paint, fonts (DD-22, DD-35), Android ship target, iOS follow-on, no-GPU (DD-21), native-link work, and "never on the MCU" are unchanged from v2.

SAF still feeds `open(bytes)` via `MmapReader::from_bytes`. TalkBack uses the text layer (AT-63). The default screen after open is the kite first view, then the warehouse after Continue (`hit_test` / `proceed_to_warehouse`).

The no-network Android manifest stays the default. Q-03 is therefore "not in v1 phone builds"; a later DD would be required to add phone network or phone writes.

**Budgets (DD-06, Q-06).** Unchanged: the second-renderer trigger stays dormant until Ashley names a device and numbers.

### 5.8 Render API, warehouse scale, and animation

`RenderFrame` is unchanged from v2 (rgba, width, height, text_runs, semantics). Every `render_*` in §5.1 also takes `width` and `height`. A `device_pixel_ratio` parameter is accepted and defaults to `1.0`; v1 tests use `1.0`. At `1.0`, 1 CSS px = 1 pixmap px (DD-37).

`proceed_to_warehouse` and `hit_test` are part of this API. Floor N is always passed as `floor_index`. Scroll offsets are explicit.

**`instant` (AT-13).** When `instant` is true, `render_floor_transition` returns only the end frame (the t = 0.45 layout), and ignores `t_seconds`. When `instant` is false, `t_seconds` is used as in v2 (samples at 0, 0.225, 0.45).

**Scale (DD-37, AT-16, AT-92).** At reference scale 1.0, bays are square and area is linear in stride. A 4 KiB bay's side is **exactly 32** CSS px; an 8 KiB bay's side is **exactly 45** CSS px (nearest integer to 32√2). AT-16/AT-92 assert those integer sides, not a loose area ratio.

The warehouse is a scrollable grid; `scroll_x` / `scroll_y` reach floors that do not fit the viewport.

**Animation** endpoints and bezier are unchanged from v2 (AT-62).

### 5.9 Robustness

Unchanged in intent: `cargo-fuzz` target `open_and_render` (AT-71); malformed input is an error or quarantine, not a crash.

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

Negative write tests (AT-28, AT-46, AT-57, AT-73, AT-74, AT-76, AT-89, AT-94) first run a **control** select that succeeds on a valid `ADDON_EDU` working archive in the same harness, then run the failing case, then assert the failing target is unchanged (or, for AT-89, that no snapshot was created).

Pack tests live in the s3a-storage workspace. AT-38, AT-39, AT-43, and AT-63 live in s3a-reader and are feature-gated or in a separate test crate that does not build in the default reader CI job until the reader pin moves off `ff89564`. Android and iOS ATs are skipped, not failed, on a host-only CI job, and the skip is reported. "Land" means local branches; nothing is pushed until Ashley says so.

### Pack and license

*   **AT-01** (fail-first). `cargo test -p s3a-pack-kite` exists and the crate's Cargo manifest does not cause `s3a-core` to depend on kite or kitesurf (`cargo tree -p s3a-core` has no such edge).
*   **AT-02** (fail-first). The pack's distribution directory, and the Android APK / iOS IPA when those artifacts are built, contain Apache-2.0 `LICENSE`, Kitesurf `NOTICE` (including the Cloudflare 2026 copyright line and the Stylo / PSL MPL notes), OFL text for every shipped font, and IJG text if jpeg-encoder is linked. A CI check lists these files; a missing NOTICE fails.
*   **AT-03** (fail-first). A diff of the vendored Kitesurf tree against `235111c6fe10f2d63f3476bf5e3d19b4c7089415` lists every modified file, and each of those files starts with or includes a prominent "changed from upstream kitesurf `235111c6fe10f2d63f3476bf5e3d19b4c7089415`" notice in addition to the original copyright (Apache §4).
*   **AT-70** (fail-first). A generated third-party license inventory covers the resolved tree (not only Kitesurf's NOTICE). The distribution includes an MPL source offer for Stylo / PSL. The app's About / licenses screen shows those notices.

### Host

*   **AT-04** (fail-first). `s3a-kite host` with every Scholar-mode required flag in §5.2.2 (`--bind 127.0.0.1`, `--port <ephemeral>`, `--auth-token-file`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--researcher-id`, `--fetch-timeout-secs`) starts without Wrangler and without a Worker Loader, and `GET http://127.0.0.1:<port>/health` returns HTTP 200, `Content-Type: text/plain`, and body exactly `ok\n`, without a token. Today there is no such binary.
*   **AT-05** (guard once the binary exists; skipped until then). That binary does not read `packages/engine/wrangler.jsonc` and does not call a `LOADER.load` API.
*   **AT-06** (fail-first). The same host, without a valid token, rejects every route in the §5.2.1 table other than loopback `GET /health` (non-2xx, no body that looks like a successful frame or handle). There is no CDP listener: a request to `/json`, `/json/list`, or `/devtools/*`, or an `Upgrade: websocket` request to any path, is 404 (and still requires a token except loopback `/health`). An open unauthenticated listener other than loopback `/health` is a failure.
*   **AT-07** (fail-first). Omitting `--port` or `--bind` in any mode exits non-zero with an error that names the missing flag. No implicit port is bound.
*   **AT-50** (fail-first). On a non-loopback bind with TLS (not `--behind-proxy`), `GET /health` without a token is rejected; with a valid Bearer token it succeeds with body `ok\n`.
*   **AT-51** (fail-first). `s3a-kite host --bind 0.0.0.0 --port <ephemeral> --auth-token-file <token>` plus other Scholar flags, without `--tls-cert`/`--tls-key` and without `--behind-proxy`, exits non-zero and binds nothing.
*   **AT-45** (fail-first). Startup with an empty `--auth-token-file`, or a token file shorter than 32 bytes, exits non-zero, names the token file, and binds nothing. The host compares the **raw file bytes** with no trimming; a file of 32 bytes of secret plus a trailing newline is a 33-byte token (AT-81).
*   **AT-52** (fail-first). A UI-token request to open a path containing `../` that would escape `--corpus-root` or `--wiki-root` is rejected and opens no file.
*   **AT-53** (fail-first). A `file://` URL whose path is outside the configured roots is rejected.
*   **AT-54** (fail-first). A host fetch to `http://169.254.169.254/` (and to the configured LRS private address, when one is configured) is blocked; the request does not leave the host.
*   **AT-55** (fail-first). A host fetch URL that would retrieve the token file, the working archive, or a corpus file as an HTTP resource is denied. Those files are opened only through `POST /v1/open` under allowed roots.

### Hyper-tile `.s3a`

*   **AT-08** (fail-first). Open a sealed archive produced like `test_tour_displays_manifest_in_lobby_and_floor`. The first frame is produced by `render_manifest`. Its text layer contains the exact stored manifest payload as a contiguous substring (including `THIS IS AN S3A FILE` and the seal title/creator). The warehouse floors are not the first frame.
*   **AT-09** (fail-first). Open a hyper-tile archive with no tile 78. The first frame is a labeled "manifest absent" state and does not contain invented title/creator text. After `proceed_to_warehouse` (or a Continue `hit_test`), `render_warehouse` is the next view.
*   **AT-10** (fail-first). After the manifest view, the tour floor list matches `s3a-cli tour --json` on the same hyper-tile file: `index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`.
*   **AT-11** (fail-first). Opening floor *N* with `render_floor_detail` on a golden hyper-tile fixture whose `withheld_count` was hand-verified from core `ConsentRecord` data shows those records and that `withheld_count`. Fixtures that contain type 82 are included in this suite and must *not* treat type 82 as Edge consent. The CLI is not the `withheld_count` oracle when type 82 is present.
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
*   **AT-16** (fail-first). An 8 KiB `EdgeWriter` fixture shows `tile_stride == 8192` and `size_bytes == 8192`. Rendered alone at the §5.8 reference scale (`device_pixel_ratio` 1.0), a 4 KiB bay's side is exactly 32 CSS px and an 8 KiB bay's side is exactly 45 CSS px (AT-92). Bay area is therefore linear in stride at this scale.
*   **AT-17** (fail-first). MICRO-VEC-02: only the valid tile(s) appear as floors; the torn tile is omitted and the header sheet reports it as ignored/torn.
*   **AT-18** (fail-first). MICRO-VEC-03: seal verification matches `s3a-engine`. A copy with a reordered or mutated tile fails verification, shows quarantine, and does not list the mutated tile (or any subsequent tile) as a valid floor.
*   **AT-19** (fail-first). A type-64 tile of 32-byte `EquipmentEventRecord`s paints timestamp, the EDGE §5.2.2 event name, severity, and the category label from §5.4. Severity 0, 1, and {2, 3} use three pairwise-different badge colors (pixel samples); 2 and 3 share a color. Exact RGB values are Q-10 and are not asserted here. A severity-0 maintenance event still carries the label "maintenance".
*   **AT-20** (fail-first). A type-64 tile whose record size is 96 bytes paints `DeviceIdentityRecord` fields (device id, hardware model), not event columns.
*   **AT-21** (fail-first). A compact-header Edge file (`FLAG_COMPACT_HEADER`) places tile 0 at offset 4096 (4 KiB stride) or 8192 (8 KiB stride), not at 512.
*   **AT-22** (guard; already passes today). `crates/s3a-edge/Cargo.toml` does not depend on kite or kitesurf. A `no_std` build of `s3a-edge` still succeeds.
*   **AT-23** (fail-first). When an Edge type-71 `EdgeConsentRecord` does not allow the research purpose, kite withholds event-log payload records and reports `withheld_count`. The implementation must use the Edge layout, not type 82 and not core `ConsentRecord`. The derived header sheet and type-64 identity floors remain visible (AT-86).
*   **AT-66** (fail-first). A file with `FLAG_PROFILE_EDGE` and a 128 KiB stride, and a file with `FLAG_TILE_SIZE_4KB` and stride 8192, each open into the "inconsistent Edge header" quarantine state and are not toured as a consistent Edge or hyper-tile file.
*   **AT-67** (fail-first). A type-64 tile whose record size is neither 32 nor 96, or whose `record_count` is 0, is shown as an "unknown layout" floor.
*   **AT-68** (fail-first). An otherwise valid Edge file with no `DeviceIdentityRecord` opens; the header sheet omits device fields and is labeled "device identity record absent".

### Scholar 3-pass and snapshots

*   **AT-24** (fail-first). Building the Pass 1 candidate list twice (including `GET /v1/candidates` twice) leaves the corpus archive, the LRS store, and `--log-dir` and `--data-dir` byte-identical to the start (no `HumanLrsRecord`, no xAPI statements, no new snapshot tiles, no paper ids or URLs in those directories).
*   **AT-25** (fail-first). Dismissing a paper as off-topic / skip (`POST /v1/skip`) writes nothing (same byte-identity check, including `--log-dir` and `--data-dir`).
*   **AT-61** (fail-first). After a consent-off select, `--log-dir` and `--data-dir` contain no paper ids or source URLs. The working archive still has the inclusion record.
*   **AT-26** (fail-first). Selecting or downloading a paper, with tracing consent on and a configured LRS, writes one inclusion statement whose verb IRI is taken from `s3a-pack-scholar` `xapi.rs` (not a newly invented IRI) and a Scholar `HumanLrsRecord` (tile 10) for that paper identity in the working archive, plus a snapshot when bytes were captured.
*   **AT-27** (fail-first). The same select with consent off: zero HTTP requests to the LRS, and the working archive still contains the inclusion record (and a snapshot when bytes were captured). The corpus and LRS store are unchanged.
*   **AT-48** (fail-first). Download with consent off still delivers the file to `--download-dir` and writes the local inclusion record and snapshot, with zero LRS requests.
*   **AT-28** (fail-first). After a successful control select, a call to `/v1/select` or `/v1/download` without a token is rejected and writes nothing (working archive, snapshot dir, download dir, LRS unchanged versus the post-control state).
*   **AT-57** (fail-first). There is no agent-token flag and no CDP Input domain. A request that presents a second token file, a `X-Agent-Token` header, or a synthesized CDP `Input.dispatchMouseEvent` is rejected or 404 and writes nothing. (v1 has only the UI token; DD-41.)
*   **AT-29** (fail-first). The Pass 1 timer reaching 15 seconds does not create or delete any record.
*   **AT-30** (fail-first). Opening a configured wiki `index.md` paints its heading text from the file bytes as a contiguous substring of the text layer. Opening a missing `papers/SRC-9999.md` reports missing and does not invent a dossier.
*   **AT-56** (fail-first). A wiki page whose Markdown contains a script tag that would set `document.title` and an image tag with an `http` URL paints those tags as text or omits them as raw HTML; no script runs; no JS context is created; the render issues zero network requests.
*   **AT-31** (fail-first). After an inclusion (consent on or off), a sealed snapshot exists under `--snapshot-dir` as `{sha256_hex}.s3a` whose `blob_key` and payload hash equal SHA-256 of the captured bytes; `POST /v1/open` on that path is allowed and `verify_sealed_archive` succeeds. Skip does not create a snapshot.
*   **AT-32** (fail-first). That snapshot's reassembled PROVENANCE document (bytes after the 40-byte tile prefix, UTF-8) contains exactly three lines: `sha256=<lowercase hex>`, `url=` plus the source URL or empty, and `captured=<RFC3339 seconds UTC>`. No other metadata fields are present in that document. A seal-generated tile 78, if any, is not counted as provenance metadata.
*   **AT-46** (fail-first). After a successful control select on a valid `ADDON_EDU` archive, select against a second working archive whose `addon_mask` lacks `ADDON_EDU` fails, names the missing bit, and leaves that second file and the LRS store byte-identical.
*   **AT-47** (fail-first). After a successful snapshot write, `verify_sealed_archive` on the snapshot file succeeds.
*   **AT-58** (fail-first). Revoking tracing consent mid-session via `POST /v1/consent` with `{ "state": "off" }` (not by editing the file on disk) drops subsequent LRS requests to zero, including any retry queue (`--data-dir/lrs-retry/` is deleted); the already-written local inclusion record remains.
*   **AT-59** (fail-first). With consent on, an LRS 5xx does not roll back the local inclusion record or snapshot; a retry file is written under `--data-dir/lrs-retry/`; the corpus is unchanged.
*   **AT-60** (fail-first). Selecting the same paper twice in one working archive leaves a single inclusion record and a single snapshot.
*   **AT-73** (fail-first). After a successful control write, a second `TileWriter` / host that cannot take `try_lock_exclusive` on the working archive fails the write and does not truncate the file.
*   **AT-74** (fail-first). Scholar-mode select with `--working-archive` omitted is a startup error (AT-77). A running Scholar host whose working-archive path cannot be created or opened fails the select closed and writes nothing. Phone UniFFI exports no select/download (AT-99).

### Web pages vs archives

*   **AT-33** (fail-first). Opening a local HTML fixture under `--view-root` (a file that contains a `<script>` setting `document.title` and a static heading) uses native-host layout with JavaScript disabled (DD-44). The script does not run: the text layer contains the static heading and does not contain the scripted title. The process does not start Wasmtime or a Boa isolate (the host binary's running process tree contains no wasmtime, and `cargo tree` for the host does not include the `wasmtime` crate as a v1 dependency). This test does not fetch the public Internet. A live `http(s)` fetch, when used, is a host fetch under §5.2.4 and also starts no Wasmtime.
*   **AT-34** (fail-first). Opening a `.s3a` file does not create a Boa context, a Wasmtime isolate, or a page-script isolate for the tour. The tour is painted by the native `vello_cpu` renderer.

### Safety and scope

*   **AT-35** (fail-first). A hyper-tile or Edge file with a failed tile CRC or a failed Ed25519 check is labeled quarantined. A failed CRC tile is not shown as a valid floor. A failed Edge Ed25519 signature invalidates the signed chain from that point (the type-70 tile and tiles whose `seal_hash` the signature was meant to cover), not only the type-70 tile (DD-57).
*   **AT-36** (guard once the pack crate exists; skipped until then). `cargo deny` (or equivalent) plus a grep on pack-owned source and tests (not vendored Kitesurf) contain no Bitcoin, OpenTimestamps-write, payment, or token-transfer code paths. Reading Edge `seal_hash` values is allowed.
*   **AT-37** (guard; already passes today). No new `TileType` constant is added under `s3a-core` relative to the set at s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`.
*   **AT-71** (fail-first). The `open_and_render` cargo-fuzz target exists and, on the AT-08 fixture, MICRO-VEC-01/02/03, and a malformed file, does not abort.

### Mobile library

*   **AT-38** (fail-first). The UniFFI library builds for `aarch64-linux-android` and `x86_64-linux-android` with minSdk 26. `open` + `render_manifest` + `proceed_to_warehouse` + `render_warehouse` return a `RenderFrame` (RGBA plus text layer) or `KiteError`. For the AT-08 fixture, the first frame's text layer contains `THIS IS AN S3A FILE`; for MICRO-VEC-01, the first frame's text layer includes `4096`.
*   **AT-39** (fail-first). The Android UI path that shows the tour does not instantiate `android.webkit.WebView` for that screen. A lint or unit test on the Compose screen's view tree fails if a WebView is the tour surface.
*   **AT-40** (fail-first). `cargo build --target aarch64-apple-ios -p s3a-pack-kite` produces a `staticlib` (and the `ios-sim` target also links). Shipping the iOS app remains a follow-on (DD-25); this test is the feasibility gate. The job is skipped, not failed, on a host-only CI runner without the Apple toolchain, and the skip is reported.
*   **AT-41** (guard once the pack crate exists; skipped until then). `cargo tree` for `s3a-pack-kite`, for Android and host targets, does not include `wgpu` or a Vulkan/Metal/GLES backend at all. The linked paint crate is `vello_cpu` / `anyrender_vello_cpu`.
*   **AT-42** (fail-first). With the required UA fonts removed from the test fixture, `render_manifest` returns a `KiteError` that names missing fonts. It does not silently render with system fonts (`system_fonts` stays false).
*   **AT-43** (fail-first). In the Android app, opening the AT-08 fixture shows the kite surface as the default screen after open (first the manifest, then the warehouse after Continue). `MapLayout` is not the default.
*   **AT-44** (fail-first). `getrandom` for Android and iOS targets is not enabled with the `js` / `wasm_js` features. A build without a JavaScript runtime succeeds (the fork change required in §5.7).
*   **AT-49** (fail-first). A manifest containing CJK characters, with Noto Sans SC and Color Emoji absent, opens successfully; required Latin text is in the text layer; CJK code points are painted as `.notdef` (not from system fonts); zero network requests.
*   **AT-63** (fail-first). The first-view semantics tree includes the manifest heading (or the "manifest absent" label). An Android TalkBack (or equivalent accessibility) test reads that label from the kite surface.

### v3 additions (round-2 dispositions)

*   **AT-75** (fail-first). Every route in the §5.2.1 table exists with the documented request and success response on a Scholar host. A path not in the table returns 404 with the JSON error object and still requires a token except loopback `/health`.
*   **AT-76** (fail-first). Viewer-only mode (`--viewer`, `--bind`, `--port`, `--auth-token-file`, `--view-root`, `--log-dir`) starts. `POST /v1/select`, `/v1/download`, `/v1/skip`, `/v1/consent` and `GET /v1/candidates`, `GET /v1/consent` return 409 with error code `viewer_only` and write nothing (control-then-negative as in the preamble). Opening a file under `--view-root` with `/v1/open` still succeeds.
*   **AT-77** (fail-first). Scholar mode missing any required flag in §5.2.2 (`--bind`, `--port`, `--auth-token-file`, `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--log-dir`, `--data-dir`, `--researcher-id`, `--fetch-timeout-secs`) exits non-zero and names the flag. `--lrs-url` without `--consent-state-file` is a startup error that names `--consent-state-file`. A required directory flag that points at a missing path is a startup error that names the flag. The host does not create those directories.
*   **AT-78** (fail-first). Scholar start with `--working-archive` pointing at a missing file whose parent directory exists succeeds and binds `/health`. If the parent directory does not exist, startup fails and names `--working-archive`.
*   **AT-79** (fail-first). The first successful Scholar write creates the working-archive file with `TileWriter::create` and `enable_addon(ADDON_EDU)`, and writes an active core `ConsentRecord` with `purpose = Research` and `scope = 0`. Opening that archive in kite lists the owner's inclusion records (`withheld_count` does not hide them all).
*   **AT-80** (fail-first). A snapshot written to `--snapshot-dir` can be opened via `POST /v1/open` with that absolute path. A path outside every allowed root still fails (AT-52).
*   **AT-81** (fail-first). A token file of 32 secret bytes plus a trailing newline (33 bytes) is accepted only when the `Authorization: Bearer` value is exactly those 33 bytes (for example via the raw-bytes comparison in the test). A request that sends only the 32 secret bytes, or a trimmed UTF-8 string, is rejected.
*   **AT-82** (fail-first). A captured payload of at least 200 KiB of non-repeating bytes produces a sealed snapshot under `--snapshot-dir` whose BLOB tiles use the 40-byte prefix, `blob_key` equals SHA-256 of the full payload, `total_chunks` is greater than 1, concatenating chunks `0 .. total_chunks-1` equals the original bytes, and `verify_sealed_archive` succeeds.
*   **AT-83** (fail-first). `POST /v1/select` then `POST /v1/download` of the same paper identity in one working archive, using the Q-07 draft (one existing verb for both), leaves a single inclusion record and a single successful snapshot. The download file is still delivered under `--download-dir`.
*   **AT-84** (fail-first). With `--lrs-url` set, a missing or malformed `--consent-state-file` is a startup error that names the file. The host does not invent an `on` default and does not bind a port.
*   **AT-85** (fail-first). An otherwise valid Edge file with no type-71 record opens unrestricted: research-purpose filtering withholds no records; `withheld_count` is 0. This matches `tour.rs` "open / unrestricted (no consent records)".
*   **AT-86** (fail-first). When a type-71 record denies the research purpose, the derived header sheet and type-64 `DeviceIdentityRecord` floors still show device identity fields; event-log payloads are withheld (AT-23).
*   **AT-87** (fail-first). `--bind 0.0.0.0` (or any non-loopback address) plus `--behind-proxy`, even with `--tls-cert` and `--tls-key` present, exits non-zero, names the conflict, and binds nothing. `--behind-proxy` with `--bind 127.0.0.1` starts.
*   **AT-88** (fail-first). A host fetch whose first response is an HTTP redirect to `http://169.254.169.254/` is blocked at the redirect hop: the second request is not sent. A fetch whose DNS resolution is `127.0.0.1` (or `::1`) is blocked at connect time and no TCP connection is made to that address.
*   **AT-89** (fail-first). If the inclusion-record write (step 1) fails (for example the working archive cannot be locked), no new snapshot file is created in `--snapshot-dir`, no LRS request is made, and the error is returned. A later successful select of the same paper then writes the record and snapshot (the failed attempt is not treated as a successful idempotent artifact).
*   **AT-90** (fail-first). A hyper-tile file that has a seal trailer and fails `verify_sealed_archive` opens with a persistent quarantine banner naming the seal failure. CRC-valid committed tiles may still appear as floors; no floor is presented as a verified seal.
*   **AT-91** (fail-first). After AT-59's LRS 5xx, the retry file exists under `--data-dir/lrs-retry/` and is still there after the host process is stopped and started again with the same flags. `POST /v1/consent` to `off` deletes that directory.
*   **AT-92** (fail-first). At reference scale 1.0, measured integer bay-rectangle sides are exactly 32 CSS px (4 KiB) and exactly 45 CSS px (8 KiB).
*   **AT-93** (fail-first). Two `render_floor_transition` calls with `instant=true` and different `t_seconds` values return byte-identical pixmaps and the same text layer (the end frame). `t_seconds` is ignored.
*   **AT-94** (fail-first). When `--download-dir` already contains a file of the destination name, `POST /v1/download` fails, does not overwrite that file (byte-identical), and if the inclusion write already succeeded it is kept.
*   **AT-95** (fail-first). Each `render_*` function in §5.1 returns `Result<RenderFrame, KiteError>`. `open` of a byte string that is not an S3A file returns `Err` and the process does not abort.
*   **AT-96** (fail-first). The pack exports `open`, `open_path`, `render_manifest`, `proceed_to_warehouse`, `render_warehouse`, `render_floor_transition`, `render_floor_detail`, `hit_test`, and `paper_identity`. On the AT-08 first view, `hit_test` at the Continue control returns `Continue`; a point on empty space returns `None`.
*   **AT-97** (fail-first). A file whose `tile_stride` is 65536 (neither 4096, 8192, nor 128 KiB) opens as unsupported / inconsistent and is not toured as a hyper-tile.
*   **AT-98** (fail-first). `open` / `from_bytes` of the AT-08 fixture succeeds. On Unix, if the optional path-mmap path is used and the file is truncated under the mapping, the next `render_*` returns `KiteError` and the process stays up (no abort).
*   **AT-99** (fail-first). The v1 UniFFI surface does not export select, download, skip, consent-toggle, or working-archive write functions. A Kotlin / Swift binding test fails if those functions are present.

--------------------------------------------------------------------------------

## 8. Draft decisions

Ashley may accept a draft, change it, or leave it open. Draft values follow. DD-01–DD-40 keep their topics. Changes of meaning since v2 are noted in the "Notes" column and in the round-2 disposition.

| ID | Topic | Draft value | Notes |
| --- | --- | --- | --- |
| **DD-01** | Pack location | `crates/s3a-pack-kite` in the s3a-storage workspace, with the Kitesurf Rust core vendored from `ashleycribb/s3a-Kite` at commit `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. | Names in §5.1 remain fixed. |
| **DD-02** | Host compile mode | Link `kitesurf-core` and `renderer-core` with `native-host` as native rlibs. Wasmtime is not used in v1. | **Amended** (DD-44). Wasmtime is no longer required for page JS. |
| **DD-03** | Bind address and port | Required flags `--bind` and `--port`. No silent production default in any mode. Docs may show `127.0.0.1:8787` as an example because that is upstream Kitesurf's local port. | Unchanged. |
| **DD-04** | Visual metaphor | Call the UI a warehouse. Data model remains lobby + floors. Bays map 1:1 to floors. Bay area is linear in tile stride. | Reference scale is in DD-37. |
| **DD-05** | Device-side stored manifest | Not written on the MCU in v1. The hub/PC/phone shows the derived header sheet. If firmware later stores UTF-8 manifest text, it must use a declared tile type, and the viewer must prefer those stored bytes. | Unchanged. |
| **DD-06** | Slimmed renderer / MCU | v1 uses the same `native-host` + `vello_cpu` renderer on hub, PC, and phone. No second renderer until a measured memory budget on a named device fails. Never on the microcontroller. | Trigger stays dormant until Q-06 is answered. |
| **DD-07** | Event colors | Color by severity: 0, 1, and {2, 3} are three pairwise-different colors (draft still: green / yellow / red). Print `event_type` as the EDGE §5.2.2 name, and show the non-color category label in §5.4. Exact RGB is Q-10. | **Tightened** (F-24). Exact channel values are not invented. |
| **DD-08** | Pass 1 timer | Show a 15-second skim timer. Expiry does nothing to records. | Unchanged. |
| **DD-09** | Snapshot and inclusion vs consent | Snapshots and inclusion records of selected papers are local working-archive writes and do **not** require tracing consent. Skip does not snapshot. Tracing consent gates only LRS xAPI. | Unchanged. |
| **DD-10** | Device transport | v1 opens files already on the hub, PC, or phone. BLE/USB/SoftAP stay Edge Phase 2. | Unchanged. |
| **DD-11** | Host authentication | Fail closed. `--auth-token-file` points at a token that is not in the repository. Unauthenticated requests are rejected except loopback `GET /health`. | Transport and token-file rules are in DD-29 / DD-52. |
| **DD-12** | LRS URL | Must be configured for LRS emission. No invented localhost default. If consent is on and the URL is missing, the LRS statement fails closed with an error. Local inclusion and snapshot still write (DD-09). | Unchanged. |
| **DD-13** | Markdown | The pack parses CommonMark with `pulldown-cmark` (raw HTML disabled or escaped) to HTML, then native-host Blitz lays out that HTML. No JS and no remote subresources on wiki/corpus pages. | Unchanged. |
| **DD-14** | PDF pages | v1 does not rasterize PDF input. Skim shows title, abstract, and any markdown conversion already in the archive. If that conversion is missing, show "no text conversion in archive", not a generated summary. | Unchanged. |
| **DD-15** | New tile IDs | None in v1. Snapshots use existing BLOB + PROVENANCE. Inclusion uses Scholar tiles when `ADDON_EDU` is set. | Unchanged. |
| **DD-16** | Addon mask | No new addon bit in v1. Scholar writes require `ADDON_EDU`. Edge files already carry `ADDON_WEARABLE`. A missing bit on an existing working archive is a hard error (§5.6). | Unchanged. |
| **DD-17** | Upstream pin | Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. | Unchanged. |
| **DD-18** | Animation timing | Match `tour_html_template.html`: 0.45s `cubic-bezier(.2,.8,.2,1)` from the tile rect to 6vw/8vh/88vw/84vh, skipped when `instant` is set. | Frame API names are in DD-37. |
| **DD-19** | App relationship | s3a-kite is the viewer inside `s3a-reader`. Verification stays in `s3a-reader-core`. Kite does not replace the edge writer or the MCU firmware, and does not run a second verifier. | Unchanged. |
| **DD-20** | Quarantine UX | Follow the existing reader: a quarantined file can be opened with a persistent banner. Failed-CRC, torn, and chain-broken tiles are not valid floors (§5.4 item 2). Seal-trailer failure is AT-90. | Extended by DD-57. |
| **DD-21** | GPU | v1 stays on `vello_cpu`. No wgpu, Vulkan, Metal, or GLES backend at all in the pack dependency tree. | Unchanged. |
| **DD-22** | Mobile fonts | Ship Noto Sans, Italic, Serif, Mono, and Symbols with the app and register them as UA fonts. `system_fonts` remains false. Color Emoji and CJK SC are optional bundled asset packs, not required to open a file, and not downloaded over the network. | Unchanged. |
| **DD-23** | Android ABIs and UI | minSdk 26, `arm64-v8a` and `x86_64`, UniFFI + Kotlin, Jetpack Compose displays the Rust pixmap and text layer. No 32-bit ARM in v1. | Unchanged. |
| **DD-24** | Kotlin deck plan | The kite warehouse is the primary view and the default screen after open. `MapLayout` may remain as a secondary diagnostic screen, not the default. | Unchanged. |
| **DD-25** | iOS | Feasible (CPU raster + UniFFI Swift). Ship Android first. iOS follows when AT-40 stays green and a SwiftUI surface shows the same frames. | Unchanged. |
| **DD-26** | Page JS on a native host | **v1 does not run page JavaScript.** `http(s)` documents are host-fetched and laid out with native-host Blitz / `vello_cpu` with JS disabled. Wasmtime is not required. Native Boa is out of v1. | **Amended** (was Wasmtime + wasm32 Boa). See DD-44. |
| **DD-27** | Write target | Inclusion records go to `--working-archive`. Snapshots go to `--snapshot-dir`. The corpus is read-only to kite. | Clarified paths (F-05). |
| **DD-28** | Write API, seal, lock | `TileWriter::create` / `open_append` + Scholar `create_human_lrs`; exclusive `try_lock_exclusive`; new archives get `ADDON_EDU` and an active research-purpose `ConsentRecord` (DD-53); existing archives without the bit fail closed; snapshots are separately sealed with `seal_archive` and `verify_sealed_archive`; the working archive stays live. | Extended by DD-53. |
| **DD-29** | Health, token, TLS | `GET /health` is 200 `text/plain` body `ok\n`; Bearer token compared as raw file bytes, no trim, at least 32 bytes; TLS required for non-loopback binds. | Tightened (F-33, F-34). `--behind-proxy` is DD-52. |
| **DD-30** | Roots | `--wiki-root`, `--corpus-root`, `--working-archive`, `--snapshot-dir`, `--download-dir`, `--view-root`; canonicalize as in §5.2.2. The working-archive **parent** is canonicalized at start; the file may be missing. | **Amended**: no CDP scope and no agent tokens (DD-41). Path rules fix F-05. |
| **DD-31** | Host-fetch egress | No page subprocess in v1. The host applies default-deny private / loopback / mapped / `0.0.0.0` / CGNAT / link-local / metadata / LRS at connect time and on every redirect hop. Optional `--egress-allowlist`. | **Amended**: no page-process sandbox (F-18, DD-44). |
| **DD-32** | Human control | Only the UI token may call write routes. Custody of the token file is the human-control point. v1 has no agent token and no gesture bit. | **Amended** (DD-41). |
| **DD-33** | Tracing consent model | `--consent-state-file` (`on`/`off` + RFC3339 UTC timestamp); `--researcher-id` required; revoke drops LRS and the retry queue, keeps local records. | File rules are in DD-50; queue is DD-51. |
| **DD-34** | Inclusion side effects | Order: local record, then snapshot, then LRS. Failure of step 1 skips 2 and 3. LRS failure does not roll back local writes. Idempotent per working archive + paper identity + verb, for **successful** artifacts only. Pass 1 does not log paper ids. | Tightened (F-21). |
| **DD-35** | Missing optional fonts | Absent CJK/emoji packs: open succeeds, `.notdef` for missing glyphs, no system fonts, no network. | Unchanged. |
| **DD-36** | Manifest-to-tour trigger | Explicit Continue / `proceed_to_warehouse` / Continue `hit_test` only; no timer; long text wraps and scrolls with `scroll_y`. | Unchanged in meaning. |
| **DD-37** | Warehouse scale and frame API | The §5.1 functions, including explicit viewport, floor index, scroll, `instant`, and `hit_test`. At scale 1.0, 4 KiB bay side is 32 CSS px and 8 KiB bay side is 45 CSS px. `device_pixel_ratio` defaults to 1.0 in the API (tests use 1.0). | **Amended** (F-04, F-23, F-42). The previous `render_warehouse_frame` name is retired. |
| **DD-38** | Text and semantics layer | Every frame returns `text_runs` and `semantics`; tests and TalkBack use it, not OCR. | Unchanged. |
| **DD-39** | Phone Scholar writes | **Out of v1.** Phone builds are viewers only: open and render, no candidate list, select, download, LRS, or working-archive writes. | **Amended** (was local writes, no LRS). See DD-45. Q-03 draft follows this. |
| **DD-40** | Inconsistent Edge headers | Quarantine; do not guess a stride or profile. Missing identity: warn and open. Unknown type-64 layout: labeled floor. Other strides: AT-97. | Extended by AT-97 (F-31). |
| **DD-41** | No CDP, no agent tokens | v1 does not reimplement `packages/engine/src/cdp/` and does not mint or accept an agent token. The Host HTTP API is the only HTTP surface. A later phase may add CDP under a new DD. | New. Narrows F-02. |
| **DD-42** | Native UI and Host HTTP API | The human UI is a native window in the `s3a-kite` process that calls the pack API directly. §5.2.1 is the complete v1 wire contract for tests and for an equivalent remote UI. | New. Fixes F-01. |
| **DD-43** | `KiteError` | Every fallible pack function returns `Result<_, KiteError>`. `KiteError` carries a stable code and a message string. The process must not abort on a bad file (AT-95, AT-98). | New (F-29, F-32). |
| **DD-44** | Page JS off in v1 | No Boa, no Wasmtime, no wasm32 page-isolate ABI in v1. HTML is laid out with JS disabled. Remote bytes are retrieved by the host fetch in §5.2.4. A later phase may restore Wasmtime under a new DD. | New. Narrows F-03, F-16, F-39. |
| **DD-45** | Phone is viewer-only | UniFFI exports only open / render / hit-test. Scholar writes stay on the hub/PC host. | New. Narrows F-06 / DD-39. |
| **DD-46** | Paper identity and inclusion record | Paper identity is the `SRC-NNNN` stem, or the decimal `AcademicPaperRecord.paper_id`, or the source URL if the paper has neither. `HumanLrsRecord.object_hash` is CRC32C of the UTF-8 identity. `actor_hash` is CRC32C of the UTF-8 `--researcher-id`. Verb mapping is Q-07 (draft: one existing `xapi.rs` verb for both select and download). xAPI object id is the source URL if any, else the identity string (no new IRI host). | New (F-11). CRC32C is already used in S3A; Scholar tests often use literal hashes, and no other canonical hash-from-id was found in Scholar code. |
| **DD-47** | Snapshot chunking | BLOB and PROVENANCE tiles follow the 40-byte prefix in `docs/spec/registry.md` and `s3a-pack-clean` `chunk_tiles`. `blob_key` is SHA-256 of the complete captured bytes. Max chunk data is `HYPER_TILE_PAYLOAD_SIZE - 40`. The provenance document after the prefix is the three-line UTF-8 text in §5.6. `seal_archive` is used; a seal manifest is not counted as provenance metadata. | New (F-12, F-37). |
| **DD-48** | One researcher, durable disk | One host process serves one `--researcher-id` and one working archive. Durable local disk is required. Cloud Run and other stateless or multi-tenant hosts are not v1 targets. | New (F-17). |
| **DD-49** | Fetch timeout | Scholar mode requires `--fetch-timeout-secs`. There is no silent default. Ashley may later name a recommended value; this spec does not invent one. | New (F-18). |
| **DD-50** | Consent file writer | `--consent-state-file` is required when `--lrs-url` is set; missing or malformed is a startup error. The host is the only writer (temp file + atomic rename). The only toggle is `POST /v1/consent`. Mid-session outside edits are not honored. | New (F-09). |
| **DD-51** | LRS retry queue | Queued statements live under `--data-dir/lrs-retry/` and survive restart. Revoke deletes the directory. Backoff and a maximum attempt count are Q-09; until those are set, the host retries at process start and after a later successful LRS emit, and surfaces a visible error after a failed attempt without dropping the queue. | New (F-10). |
| **DD-52** | `--behind-proxy` | Accepted only with a loopback bind or a Unix-socket bind. Non-loopback plus `--behind-proxy` is a startup error, even if TLS files are also present. | New (F-19). |
| **DD-53** | Consent filtering | No type-71 (and no core consent records, on a third-party Edge file) means unrestricted, matching `tour.rs`. Header sheet and `DeviceIdentityRecord` are exempt from research-purpose filtering. On `create` of a working archive, kite writes an active research-purpose `ConsentRecord` so the owner's records remain visible. Snapshots in `--snapshot-dir` are exempt from that filter when opened as kite-written snapshot files. | New (F-14, F-15). |
| **DD-54** | Product name | Keep the crate and binary name `s3a-kite`. Cloudflare / Kitesurf marks are not granted by Apache-2.0 except as needed to describe origin and reproduce NOTICE (Q-11). This spec does not rename the pack. | New (F-38). |
| **DD-55** | Required dirs and ephemeral temp | `--log-dir` is required in both modes; `--data-dir` is required in Scholar mode; those directories must exist at startup. Render caches and per-request temp live in an ephemeral directory deleted on process exit, not under `--data-dir` or `--log-dir`. | New (F-13). |
| **DD-56** | Download directory | `--download-dir` must exist at startup in Scholar mode. A name collision fails the download and does not overwrite. If the inclusion write already succeeded, it is kept. If the inclusion write fails, a delivered file is kept and the error is shown. | New (F-21, F-41). |
| **DD-57** | Edge signature failure | A failed Edge Ed25519 signature invalidates the signed chain from that point (the type-70 tile and the tiles whose `seal_hash` the signature was meant to cover), not only the type-70 tile. Hyper-tile `verify_sealed_archive` failure is a persistent banner; CRC-valid committed tiles may still list as floors and are not presented as a verified seal. | New (F-22). |

--------------------------------------------------------------------------------

## 9. Open questions

*   **Q-01.** Lab host authentication: token file (DD-11, DD-29) or an existing lab SSO? Draft is the token file until Ashley names an SSO.
*   **Q-02.** Exact LRS base URL on the s3a-lrs VM, and the consent-copy shown before tracing.
*   **Q-03.** Whether a later phase should enable phone Scholar writes or phone network for LRS. Draft (DD-39, DD-45): not in v1; tracing and writes stay on the hub/PC host.
*   **Q-04.** Preferred CommonMark library if not `pulldown-cmark` (DD-13).
*   **Q-05.** Whether the optional CJK and emoji font packs should ship in the first Play/App build or only as additional bundled / Feature Delivery packs. They are not downloaded over the network from the no-network app.
*   **Q-06.** Named minSdk-26 measurement device, and numeric budgets for peak RSS, time to first manifest frame, and APK size delta versus the current reader, so DD-06's second-renderer trigger can be tested. No numbers are invented here.
*   **Q-07.** Which existing verb IRI in `s3a-pack-scholar` `xapi.rs` is used for select and which (if different) for download. This spec will not invent a new IRI. **Draft for v3:** use one existing verb for both, so select-then-download is a single inclusion record (AT-83). If Ashley picks two verbs, AT-83 will be revised to expect two records.
*   **Q-08.** How `--researcher-id` is provisioned in the lab (manual flag, existing Scholar session, or something Ashley names).
*   **Q-09.** LRS retry backoff schedule and a maximum attempt count for `--data-dir/lrs-retry/`. No intervals or caps are invented here (DD-51).
*   **Q-10.** Exact RGB (or palette) values for the three severity badge colors in DD-07. AT-19 asserts only pairwise difference plus labels until this is answered.
*   **Q-11.** Whether the `s3a-kite` / "Kite" name should change to reduce trademark overlap with Cloudflare Kitesurf, and if so to what. Draft (DD-54): keep the name until Ashley chooses otherwise.

Lawyer review is not required for the fork itself (Apache-2.0 + MPL file-level terms are listed in §6), but distribution of the app's font files must keep OFL notices (AT-02, AT-70). Q-11 is a naming question, not a license-term question.

--------------------------------------------------------------------------------

## 10. Risks

1.  **`native-host` is a thin feature flag.** Much of Kitesurf's DOM and JS is `#[cfg(target_arch = "wasm32")]`. The tour path should use paint/layout entry points that already have native-host or non-Wasm coverage (`scene_replay`, `VelloCpuImageRenderer`). v1 does not run page JS (DD-44), so the wasm32-only Boa path is not required, but additional native entry points may still be needed in the fork for HTML layout with JS disabled.
2.  **Wasm-oriented dependencies.** Workspace `getrandom` uses `js` / `wasm_js`. `fonts.rs` uses `wasm_bindgen`. Android and iOS builds fail until the pack forks those bits (AT-44), even though Wasmtime itself is out of v1.
3.  **Today's `s3a-cli tour` is not a drop-in Edge tour.** It steps 128 KiB tiles and has a type-82 consent *branch* (not only a comment) that does not match the Edge registry. Kite must use `MmapReader` edge APIs and `edge_display_layouts()`, and must not use the CLI as the `withheld_count` oracle on files that contain type 82.
4.  **Type-number reuse** (64, 71, 75) between hyper-tile and Edge layouts. Branching on `is_edge_archive` is mandatory.
5.  **Phone memory and binary size.** Full Stylo + Blitz + UA fonts is heavier than the current Kotlin map. DD-06 and DD-22 exist because this may force a later slim. The trigger cannot fire until Q-06 is answered.
6.  **No PDF input renderer** in the Kitesurf tree (Krilla is output). Pass 1 can look incomplete for PDF-only papers (DD-14).
7.  **Kitesurf is experimental** (README; last grounded commit 2026-11-16, `235111c`). Upstream will move. The pin is explicit (DD-17). `235111c` itself is TypeScript-only, but future upstream commits may touch Rust.
8.  **Host auth.** There is no CDP port in v1 (DD-41). AT-06, AT-50, AT-51, and AT-87 remain gates on the Host HTTP API. Token-file custody is the human-control point (DD-32).
9.  **License completeness.** Forgetting NOTICE, MPL source, the third-party inventory, or OFL/IJG texts is a release blocker (AT-02, AT-70). The `s3a-kite` name overlaps "Kitesurf" (DD-54, Q-11).
10. **Working-archive durability.** Unsealing or locking incorrectly could damage a researcher's records. AT-46, AT-47, AT-73, and AT-89 are gates. The pack must use the existing `TileWriter` unseal-on-write and lock APIs, not a new writer. v1 assumes durable local disk and one writer (DD-48).

--------------------------------------------------------------------------------

## 11. Implementation notes (after tests exist and fail)

Order of work, when Ashley authorizes code. Do not push to s3a-Kite, s3a-storage, or s3a-reader until she says so. "Land" means local branches only.

1.  Land the fail-first acceptance tests in s3a-storage so they fail for the documented reason, and land the guard tests so they stay green. Reader tests (AT-38, AT-39, AT-43, AT-63) are feature-gated or in a separate test crate until the reader pin moves off `ff89564`. Names in §5.1 are already fixed, so this step does not wait on "names as implemented."
2.  Vendor the Kitesurf Rust core from `ashleycribb/s3a-Kite` at `235111c`, add LICENSE/NOTICE, enable `native-host`, fix getrandom/wasm_bindgen for native and Android targets, and keep Apache change notices (AT-03).
3.  Implement hyper-tile manifest + warehouse paint (text layer, §5.1 / §5.8 animation, `hit_test`) against `tour_file` data for hyper-tiles, and `MmapReader` for Edge. Do not copy the type-82 consent branch or the 128 KiB unsealed scanner.
4.  Host binary with required flags, Host HTTP API, token, TLS/proxy, health, and path rules.
5.  Write path, Scholar 3-pass gates, BLOB/PROVENANCE snapshots (including payloads larger than 128 KiB), consent/LRS split, and host-fetch egress. HTTP snapshots stay in this step; there is no Wasmtime step in v1.
6.  UniFFI + Android Compose viewer surface in `s3a-reader` (tests already landed, feature-gated, in step 1); then the iOS `staticlib` link gate. No phone Scholar writes in this step (DD-45).

Do not "temporarily" call the old WebAssembly viewer, invent a manifest, walk Edge files with `HYPER_TILE_SIZE`, treat type 82 as Edge consent, gate local inclusion on tracing consent, invent a new `TileType` or verb IRI, stand up a CDP server, or add Wasmtime "just in case."

--------------------------------------------------------------------------------

## 12. Document history

*   2026-11-16: v1. Tests-first pack spec from Ashley's 2026-11-16 direction, grounded in Kitesurf `4a29038`, s3a-storage `b7bdcda`, and s3a-reader `792742c`.
*   2026-11-16: v2. Rebases the Kitesurf pin to `235111c` (Ashley's `s3a-Kite` fork; TypeScript-only upstream commit). Addresses goldfish round 1 (F-01–F-38). Splits local inclusion from tracing consent (F-03). Adds the write path (F-02) and the native page-JS model (F-01). Acceptance tests: AT-01–AT-74. Draft decisions: DD-01–DD-40. Open questions: Q-01–Q-08.
*   2026-11-16: v3. Addresses goldfish round 2 (F-01–F-42). Narrows v1: no CDP / agent tokens (DD-41), no page JS / Wasmtime (DD-44), no phone Scholar writes (DD-45), no Cloud Run (DD-48). Defines the Host HTTP API and the pack render/hit-test API (F-01, F-04). Canonicalizes working-archive parent paths (F-05). Acceptance tests: AT-01–AT-99. Draft decisions: DD-01–DD-57. Open questions: Q-01–Q-11.

--------------------------------------------------------------------------------
