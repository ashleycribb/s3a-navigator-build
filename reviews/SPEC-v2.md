# s3a-kite: S3A Browser Pack and Viewer

*   **Author:** Grok Bot (elephant) for Ashley Cribb
*   **Status:** v2 — spec only. Ashley has forked Kitesurf as [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) (`main` at `235111c6fe10f2d63f3476bf5e3d19b4c7089415`). This document does not authorize implementing the pack, or pushing to that fork or to s3a-storage / s3a-reader, until Ashley says so.
*   **Date:** 2026-11-16 (America/New_York)
*   **Ashley direction (chat, 2026-11-16):** fork Cloudflare Kitesurf as an S3A add-on pack; first use is Scholar's wiki browser for Keshav's 3-pass method; sealed snapshots; opening a `.s3a` file shows the plain-text manifest first, then a warehouse-based animated tour; later the same day, s3a-kite *is* the S3A tool and viewer (it reads `.s3a` in Rust and draws the manifest and tour itself; it does not host the old WebAssembly viewer); it also reads Edge micro-tile files (4–8 KiB, `s3a-edge`); the browser runs on a hub, PC, or phone, not on the microcontroller; and s3a-kite replaces the existing S3A viewer and becomes the mobile viewer inside the planned S3A mobile app (Android phone reader, Edge Phase 4). Ashley then created the public fork `ashleycribb/s3a-Kite` and asked for independent review rounds before any push. Standing rule used in this revision: humans make judgments; AI only checks that things exist; Pass 1 skim is never recorded; selecting or downloading (Pass 2/3) is the inclusion decision; the researcher's own records must not depend on tracing consent; xAPI tracing to the LRS is consent-only.
*   **Grounded commits (read, not modified):**
    *   Kitesurf / s3a-Kite: `235111c6fe10f2d63f3476bf5e3d19b4c7089415` (committer 2026-11-16 05:09 EST; author 2026-11-05, cherry-picked from `e669ce5`). Message: `engine: slim what each page isolate receives at startup`. Upstream [cloudflare/kitesurf](https://github.com/cloudflare/kitesurf); Ashley's fork [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) `main` is at the same commit. A shallow clone at `/tmp/kitesurf` was fetched to inspect this pin. This commit touches only TypeScript (`packages/engine` and `packages/page-script`). The Rust crates `packages/core`, `packages/renderer-core`, and `packages/kitesurf-cli`, and the workspace `Cargo.toml`, are identical to v1's pin `4a2903813a22b08d4f3cc64a4b35339d0293a5d1`. Rust, paint, font, and Boa claims below therefore still hold at `235111c`.
    *   s3a-storage (local `/workspace/s3a-storage`): `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6` (2026-11-15).
    *   s3a-reader (local `/workspace/s3a-reader`): `792742ca9c1659cb5fd4766237d13c2f5ae24404` (2026-10-28), engine pin `ff89564`.
*   **Round-1 review:** independent goldfish report `/workspace/goldfish/goldfish_s3a-kite_round1.md` (38 findings, F-01–F-38). Dispositions: `/workspace/s3a-kite/reviews/round1-disposition.md`.

This is a tests-first pack contract. Acceptance tests are listed before any implementation guidance. Each fail-first test names a concrete fixture and an expected result that does not hold in today's trees: there is no `s3a-pack-kite` crate or `s3a-kite` host, `s3a-cli tour` does not walk Edge strides correctly, and the Android app's map is Kotlin `MapLayout`, not this renderer. A smaller set of **guard** tests already hold on today's trees and must stay green; they are tagged in §7 and are not claimed to fail first.

No stubs, fallbacks, or invented runtime defaults. Anything that needs Ashley's judgment is a **DD** with a draft value, or an open question. Existing DD numbers keep their topic; meaning changes are called out in §8.

--------------------------------------------------------------------------------

## 1. Overview

`s3a-kite` is an S3A **add-on pack** that forks Kitesurf's Apache-2.0 Rust core (vendored from [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) at the pin above) and replaces Kitesurf's Cloudflare TypeScript host (Dynamic Workers, Durable Objects, Wrangler) with a host that runs on an Aura Rack, the LRS VM, or Cloud Run, and with an embedded library that runs inside the S3A mobile app.

s3a-kite **is** the S3A viewer. It is not a wrapper around the existing Scholar / `s3a-wasm-snapshot` / harness WebAssembly viewer. It reads `.s3a` files and Edge micro-tile files directly with `s3a-engine` and `s3a-edge`, and it paints the plain-text manifest (or the Edge header sheet) and the warehouse tour with Kitesurf's Rust layout and CPU rasterizer.

First product use: Scholar's wiki browser for Keshav's 3-pass literature method. The Markdown wiki and the S3A research corpus are rendered in the same engine. Pass 1 is a human skim of about 15 seconds. Off-topic papers are never recorded. Selecting or downloading a paper is the inclusion decision and is written to the researcher's working archive even when tracing consent is off. Tracing is consent-only xAPI to `s3a-lrs-server` on the s3a-lrs VM. AI and the browser only check that things exist and record what the researcher did; humans make the judgments.

S3A core stays database-only. No money and no cryptocurrency. Everything lives in the pack.

--------------------------------------------------------------------------------

## 2. Glossary

| Term | Meaning in this spec |
| --- | --- |
| Pack | A domain crate that extends S3A without changing `s3a-core`. See `docs/pack-contract.md`. |
| Kitesurf core | Rust crates `packages/core` (`kitesurf-core`), `packages/renderer-core`, and `packages/kitesurf-cli`, kept and forked. |
| Cloudflare layer | TypeScript `packages/engine` plus `wrangler.jsonc`, the Worker Loader binding `LOADER`, and Dynamic Workers used to start a page isolate. Replaced, not required at runtime. Commit `235111c` only changes this layer (and `packages/page-script`). |
| Host | The `s3a-kite` process that listens for users or agents on a hub, Aura Rack, LRS VM, or Cloud Run. |
| Mobile library | The same Rust renderer, exposed through UniFFI from crate `s3a-pack-kite`, linked into the S3A Android (and later iOS) app. |
| Hyper-tile archive | A standard S3A file whose tiles have a 128 KiB stride. |
| Edge archive | An S3A file with `FLAG_PROFILE_EDGE` and/or `tile_stride` 4096 or 8192, written by `#![no_std]` `s3a-edge`. Still a valid S3A file (EDGE profile §1.2). |
| Micro-tile | One 4,096-byte or 8,192-byte Edge tile (`MICRO_TILE_4KB_SIZE` / `MICRO_TILE_8KB_SIZE` in `crates/s3a-edge/src/header.rs`). |
| Lobby | `TourLobby` in `crates/s3a-cli/src/tour.rs`: file version, footer, tile count, seal status, consent summary, schema names, optional UTF-8 `manifest`. |
| Floor | One tile in the tour directory (`TourFloorItem`). The HTML tour zooms a floor into detail. |
| Warehouse tour | Ashley's name for the animated walk of those floors. The data model stays lobby + floors. The UI labels it a warehouse (DD-04). |
| Stored manifest | The UTF-8 payload of `TileType::MANIFEST` (78), shown by `s3a-cli tour` under `--- PLAIN-TEXT MANIFEST ---`. |
| Derived header sheet | For Edge files, which have no tile 78 in the Edge profile, a plain-text readout of `FileHeader` and `DeviceIdentityRecord` fields that are actually present, labeled as derived. |
| Pass 1 | The researcher's own skim, about 15 seconds per paper. Listing candidates writes nothing. The skim itself is never recorded. |
| Inclusion | The researcher selects or downloads a paper for Pass 2/3. That choice is written to the researcher's **working archive** as a Scholar inclusion record (and a snapshot when the bytes are captured). It does **not** depend on tracing consent. |
| Tracing consent | Permission to emit xAPI statements to the configured LRS. It gates only LRS traffic, never the local inclusion record or snapshot. |
| Working archive | A separate, configured per-researcher live `.s3a` file that receives inclusion records and snapshot tiles. It is not the corpus archive. |
| Text layer | Structured text and accessibility semantics returned with every painted frame, used for tests and for TalkBack / VoiceOver. Not OCR. |

--------------------------------------------------------------------------------

## 3. Non-goals

### 3.1 This document does not authorize

*   Implementing the pack in this revision.
*   Pushing to [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite), s3a-storage, or s3a-reader. The fork already exists; publishing further commits or vendoring into a public repo still needs Ashley's word.

### 3.2 v1 product non-goals

*   Changing `s3a-core` record layouts or adding kite tile IDs to core (DD-15).
*   Running s3a-kite, Kitesurf, or a GPU renderer on the microcontroller. `s3a-edge` stays `no_std` and heapless. The renderer never runs on the microcontroller.
*   BLE / USB / SoftAP transport (Edge Phase 2 / EDGE profile §10.1). The mobile app already opens local files; v1 kite renders those files (DD-10).
*   Hosting or embedding the existing `s3a-wasm-snapshot` / harness WebAssembly viewer.
*   Rasterizing PDF input. Kitesurf's Krilla dependency is PDF *output*. Pass 1 uses title, abstract, and any markdown conversion already in the corpus (DD-14).
*   Money, cryptocurrency, or writing Bitcoin / OpenTimestamps anchors. Reading an existing on-device SHA-256 seal chain is required for Edge verification.
*   Cloudflare Workers Paid, Wrangler, or Dynamic Workers as a runtime dependency.

v1 **does** distribute a Kitesurf derivative, under §6, when Ashley authorizes vendoring the Rust core into s3a-storage and shipping the Android (and later iOS) app. That distribution is not a non-goal.

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

--------------------------------------------------------------------------------

## 5. Architecture

### 5.1 Pack layout and names (draft: DD-01)

These names are fixed for tests-first work (they are not "as implemented later"):

| Thing | Name |
| --- | --- |
| Pack crate | `s3a-pack-kite` in the s3a-storage workspace (`crates/s3a-pack-kite`) |
| Host binary | `s3a-kite` (`s3a-kite host …`) |
| UniFFI surface | exported by `s3a-pack-kite` (cdylib / staticlib) |
| FFI functions | `open`, `render_manifest`, `render_warehouse_frame` |
| Crate types | `rlib`, `cdylib`, and `staticlib` (iOS link gate) |

The pack vendors the forked Kitesurf Rust core (`packages/core`, `packages/renderer-core`, and the `kitesurf-cli` library as needed) from [ashleycribb/s3a-Kite](https://github.com/ashleycribb/s3a-Kite) at pin `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. The TypeScript engine, Wrangler config, and Worker Loader are not a runtime dependency. `235111c`'s TypeScript isolate-startup change is therefore not shipped as behavior; the native host in §5.2 replaces that layer.

`s3a-core` MUST NOT depend on this pack. The pack depends on `s3a-engine`, `s3a-edge`, `s3a-pack-scholar` (for 3-pass records and the xAPI registry), and the forked Kitesurf crates.

**Test locations and pins.** Fail-first and guard tests AT-01–AT-37, AT-41, AT-42, AT-44–AT-62, AT-64–AT-74 live in the s3a-storage workspace (pack tests / `tests/` that compile against `s3a-pack-kite`). AT-38, AT-39, AT-43, and AT-63 live in s3a-reader (Android instrumented or unit tests). AT-40 lives in s3a-storage as the iOS link gate. When the pack exists, s3a-reader pins `s3a-pack-kite` (and the `s3a-engine` it needs) by a git revision of s3a-storage that contains the pack; today's `ff89564` pin is not sufficient and must be moved as part of that work. Path dependencies are allowed only in local developer checkouts.

### 5.2 Native host (hub, Aura Rack, LRS VM, Cloud Run)

Replace Cloudflare isolates with a host process that:

1.  Compiles `kitesurf-core` and `renderer-core` with feature `native-host` and links them as native `rlib`s (DD-02). Wasmtime is not used for the `.s3a` / Edge tour.
2.  Requires an explicit `--port` and `--bind` (DD-03). Missing values are an error in every mode. There is no silent production default and no implicit port.
3.  Rejects unauthenticated CDP and HTTP (DD-11, DD-29). Upstream Kitesurf is fail-open; s3a-kite is fail-closed.
4.  For `http(s)` pages: runs page JavaScript by loading the existing `wasm32` `kitesurf-core` module (Boa included) in **Wasmtime 36** inside one sandboxed OS process per page (DD-26, DD-31). This is the replacement for `LOADER.load` and for `packages/page-script`. The host provides the CDP `Runtime.evaluate` / navigate path into that Wasm module. Native (non-Wasm) Boa is not the v1 path; the evaluation code in the tree is `cfg(target_arch = "wasm32")`. The `.s3a` tour still does not start Boa, Wasmtime, or page-script (item 5).
5.  For `.s3a` and Edge files: do **not** start Boa, Wasmtime, or page-script. The host calls the pack's Rust reader and paints frames with `vello_cpu`.

**Health and tokens (DD-29).** The host listens for `GET /health` at that exact path (not configurable). On a loopback bind, `/health` returns success without a token so a local process manager can probe. On a non-loopback bind, `/health` requires the same token as every other route. All other HTTP routes and all CDP session upgrades require a valid token. HTTP presents the token as `Authorization: Bearer <token>`. A CDP WebSocket upgrade presents the same header. A missing, empty, or non-matching token is non-2xx and creates no session.

`--auth-token-file` is required. Startup fails if the file is missing, empty, shorter than 32 bytes of secret material, or (on Unix) has group-or-other read/write permissions (must be `0600`). Token comparison is constant-time. The token is not logged. Rotation is done by writing a new file and restarting the host (v1 has no in-process rotation API).

**TLS (DD-29).** A non-loopback `--bind` requires `--tls-cert` and `--tls-key`, or the explicit `--behind-proxy` flag meaning a reverse proxy in front of the process terminates TLS. `--bind 0.0.0.0` (or any non-loopback address) without TLS and without `--behind-proxy` is a startup error. There is no plaintext bearer token on a lab LAN in a supported bind.

**Configured roots (DD-30).** Required flags: `--wiki-root`, `--corpus-root`, and `--working-archive`. Each path is canonicalized (`realpath`) at startup; a missing or non-canonicalizable path is a startup error. After start, any file the host opens must canonicalize to a path strictly inside `--wiki-root` or `--corpus-root`, or must be exactly the `--working-archive` path. `..` segments, symlink escapes, and `file://` URLs outside those roots are rejected. `file://` inside the roots is allowed. Opening a system password file, or any path outside the roots, fails.

**CDP capability (DD-30, DD-32).** An authenticated session is one of two kinds, distinguished by which token file it matches: `--auth-token-file` is the **UI token** (human session). `--agent-token-file` (optional; if omitted, no agent sessions are accepted) is the **agent token**. Both are fail-closed. Agent tokens are read-only: they cannot invoke inclusion, select, download, snapshot, or any other working-archive write. CDP domains enabled in v1: Target, Page, Runtime, DOM, Network (observation), Input. Debugger is off. `Runtime.evaluate` runs only inside the page isolate, never against the host process. `Page.navigate` may target `http(s)` URLs that pass the egress policy, or `file://` URLs under the configured roots. There is no general host filesystem CDP method.

**Page sandbox and egress (DD-31).** Each `http(s)` page runs in a separate OS process that does not inherit open host file descriptors for the corpus, wiki, working archive, token files, or host logs. Its filesystem view is a per-page temporary directory. The host proxies network I/O. Default egress policy: deny loopback, RFC1918, RFC4193, link-local, and metadata addresses (`169.254.169.254`, `metadata.google.internal`); also deny the configured LRS host and any IP it resolves to. Public HTTPS that is not denied may proceed, unless `--egress-allowlist` is set, in which case only those hosts are allowed. A page fetch to a denied address is blocked and does not leave the host.

### 5.3 Opening a hyper-tile `.s3a` file

1.  Open with `s3a_engine::MmapReader` (path) or `MmapReader::from_bytes` (already-loaded bytes, including the mobile SAF path). If it is not an S3A file, fail with a clear error: engine `Invalid S3A file magic`, or on mobile the existing `This is not an S3A file`. If `FileHeader.version` is not 1, fail with `Unsupported file version: {n}` and do not open a compatibility mode.
2.  **First view:** if a committed, non-quarantined tile 78 is present and valid UTF-8, paint that exact stored manifest text (the same bytes as `lobby.manifest`, trimmed only as `print_tour_report` already trims). If several committed valid tile-78 tiles exist, use the first in tile order as the manifest; later tile-78 tiles still appear as ordinary floors. If tile 78 is absent, or every tile 78 is quarantined, paint a labeled "manifest absent" or "manifest quarantined" state and do not invent title/creator text. If tile 78 is present but not valid UTF-8, paint a labeled "manifest not UTF-8" state; do not invent replacement text. Long manifest text wraps to the pixmap width and scrolls; it is not silently truncated.
3.  The first view stays until the user (or the UI session) dismisses it with an explicit Continue control, or the API `proceed_to_warehouse` is called on a UI token. There is no timer auto-advance. **Then** the warehouse tour is shown and is reachable (AT-09). Floors come from the same data `tour_file` would produce for a hyper-tile archive (index, type, record count, size, hash), except consent/`withheld_count` as in item 4.
4.  Floor N detail uses the same record layout as `s3a-cli tour --floor N`. `withheld_count` and consent evaluation use core `ConsentRecord` (and marks) for hyper-tile archives, never the type-82 branch. Golden fixtures with hand-verified counts are the oracle for `withheld_count`. The CLI may be used as a cross-check only on hyper-tile fixtures that contain no type-82 tile.
5.  Archive bytes are painted as text, never executed as HTML or JavaScript (same policy as `tour_html.rs` escaping).
6.  Zoom animation is defined in §5.8 and applies to hyper-tile and Edge floors. Seal verification uses `s3a-engine` (`verify_sealed_archive` / `verify_sealed_bytes`). Unsealed hyper-tile files may be opened; the first view shows seal status (unsealed) and still shows the manifest or "manifest absent" state. The host must not use the CLI's 128 KiB unsealed scanner on Edge files.
7.  A path-backed mmap that faults because the file changed under the mapping is reported as an error or quarantine. The process must not abort on that fault. `from_bytes` snapshots are unaffected by later file changes. There is no extra size cap beyond the engine and the OS; a failed open is an error, not a hang.

### 5.4 Opening an Edge micro-tile file

1.  Detect with `MmapReader::is_edge_archive()` (flag or stride 4096/8192). If `FLAG_PROFILE_EDGE` is set but `tile_stride` is not 4096 or 8192, or a size flag disagrees with `tile_stride` (for example `FLAG_TILE_SIZE_4KB` with stride 8192), or both 4 KiB and 8 KiB size flags are set, do not tour as if the header were consistent: show a quarantine / "inconsistent Edge header" state (AT-66). `tile_stride` is not silently "fixed."
2.  Verification is performed only by `s3a-engine`. On mobile, `s3a-reader-core` runs first and is that engine result (it already calls `verify_sealed_bytes` and related APIs); kite consumes the `VerificationReport` and does not compute a second result. Failed signatures, failed CRCs, or a broken seal chain produce a quarantine banner (DD-20). **One rule for invalid tiles:** torn tiles, CRC-failed tiles, and chain-broken tiles are omitted from the valid floor list, counted on the header sheet / banner, and may appear only in a clearly marked invalid list. They are never drawn as valid floors. When the seal chain breaks, tiles before the break that already verified remain valid floors; the breaking tile and every subsequent tile are not valid floors. The banner reports the break index.
3.  **First view:** a derived header sheet, clearly labeled as derived from the file header and `DeviceIdentityRecord`, not as a stored tile 78 manifest. Fields are only those present: magic, flags, `tile_stride`, `tile_count`, generation, timestamps (RFC3339 seconds UTC, same as §4.2), `file_uuid`, device id, hardware model, firmware version. Missing fields are omitted, not invented. If no `DeviceIdentityRecord` is present, omit device fields and show a labeled "device identity record absent" note; still open the file (the EDGE §6.3 MUST is on the writer). If several identity records exist, the header sheet uses the first in tile order and later ones remain floors.
4.  **Warehouse tour scaled to micro-tiles:** each committed valid micro-tile is a floor. `size_bytes` is 4096 or 8192. Offsets follow Variant A (`512 + i * tile_stride`) or Variant B (tile 0 at `P = tile_stride`, which is 4096 or 8192). Visual footprint follows §5.8 / DD-04 / DD-37. The Continue control from the header sheet reaches the tour, same as §5.3.
5.  Type 64 with 32-byte records renders as an equipment event log: timestamp (RFC3339 seconds UTC), `event_type` name from EDGE §5.2.2, severity, and a non-color category label derived from `event_type`: **fault** for `0x0003` and `0x0004`; **maintenance** for `0x0005` and `0x0006`; **normal** for `0x0001`, `0x0002`, and `0x0007`; **unknown** for any other value. Color is by severity (DD-07), which **overrides** the category coloring in EDGE §10 (a maintenance event with severity 0 is green, not yellow). Color is never the only channel. Type 64 with 96-byte records renders as `DeviceIdentityRecord` using `edge_display_layouts()`. Type 64 with `record_count` 0, or a record size that is neither 32 nor 96, is an "unknown layout" floor, not event columns and not identity columns.
6.  Edge consent uses the Edge layout of type 71, not core `ConsentRecord` and not type 82. All kite views apply research-purpose consent filtering.
7.  No transcoding into a 128 KiB hyper-tile file is required in order to view (EDGE §1.2). Import via `import_edge_archive` is a separate, existing engine operation, not the viewer path.

The microcontroller continues to run only `s3a-edge`. It does not link kitesurf or the host (AT-22, DD-06).

### 5.5 Scholar wiki browser (3-pass)

*   Render wiki Markdown (for example `index.md`, `papers/SRC-*.md`) from the corpus or the wiki tree under the configured `--wiki-root` / `--corpus-root`. A missing page is reported as missing; the pack does not invent a dossier.
*   Markdown is parsed with `pulldown-cmark` with raw HTML disabled or escaped so it cannot become elements (DD-13). The resulting HTML is laid out by `kitesurf-core` / Blitz. Wiki and corpus Markdown do not run page JavaScript (no Boa, no Wasmtime). Remote subresources (`img`, `link`, `script`, fonts, fetches) are not loaded; the render produces zero network requests.
*   Pass 1 candidate list writes nothing (no `HumanLrsRecord`, no xAPI, no snapshot). Skip / off-topic writes nothing. The 15-second skim timer may be shown; expiry does not change records (DD-08).
*   Select or download **always** writes the Scholar inclusion record to the working archive (DD-27, DD-28) when the write preconditions in §5.6 hold. A snapshot of captured bytes is written the same way. Tracing consent is not required for those local writes (DD-09 as amended).
*   Download with tracing consent off still delivers the file to the researcher and still writes the local inclusion record and snapshot.
*   If tracing consent is **on** and an LRS URL is configured, the host also emits one inclusion statement to `s3a-lrs-server`, with a verb IRI taken from `s3a-pack-scholar` `xapi.rs` (Q-07). If consent is on and the URL is missing, the LRS statement fails closed with an error that names the missing URL (DD-12, narrowed to the LRS path); the local inclusion record and snapshot are still written, and the error is shown so the statement is not silently skipped.
*   If tracing consent is **off**, zero HTTP requests go to the LRS, including no queued retry later unless consent is turned back on. Local records still exist.
*   The pack cannot itself write include/exclude/score. Only a human select or download from a **UI-token** session creates the inclusion record (DD-32). An agent-token or unauthenticated API call that tries to write include/exclude/score is rejected and writes nothing (AT-28, AT-57).
*   Host logs and caches (DD-34, AT-24/AT-25 as extended): Pass 1 list and skip never write paper ids or URLs into host logs, CDP traces, or on-disk HTTP caches, regardless of consent. With consent off, those identifiers also do not appear in logs of a select. With consent on, they may appear only in the LRS statement path and its retry queue. Caches used to render wiki or pages are ephemeral and are not Pass 1 records.

**Tracing consent model (DD-33).** Consent is a UI toggle. Its state is stored in a host-local file given by `--consent-state-file` (required for a host that can talk to an LRS), containing the exact UTF-8 tokens `on` or `off`, a newline, and an RFC3339-seconds-UTC timestamp of the last change. It is not a new `TileType` and is not stored in the corpus. Revoking consent (on → off) mid-session drops subsequent LRS requests to zero and drops any not-yet-sent retry queue; already-sent statements are not retracted in v1. Local inclusion records and snapshots are left in place. The xAPI actor is the researcher identity supplied by required `--researcher-id` when a statement or `HumanLrsRecord` is written; `actor_hash` is computed from that identifier with the existing Scholar hashing already used for `HumanLrsRecord`. There is no invented default identity; a write without `--researcher-id` fails closed.

**Side effects, ordering, idempotency (DD-34).** On a UI-token select or download: (1) write the local inclusion record, (2) write the snapshot if bytes were captured, (3) if consent is on, emit or queue the LRS statement. A snapshot failure is reported and may be retried; the inclusion record already written is kept. An LRS 5xx or network error does not roll back local writes; the statement is retried from a host-local retry queue that exists only while consent is on. Selecting the same paper again in the same working archive does not create a second inclusion record or a second snapshot (idempotency key: working archive + paper identity + verb).

### 5.6 Write path and sealed snapshots

This section is the write path that v1 left undefined.

**Target (DD-27).** Inclusion records and snapshots go to the configured `--working-archive`, a per-researcher live `.s3a` file. The corpus archive and the wiki tree are read-only to kite. AT-24's byte-identity check applies to the corpus and the LRS store (and, as extended, to host log/data dirs for Pass 1). It does not forbid writes to the working archive after a select.

**API and locking (DD-28).** Writes use `s3a_engine::TileWriter`: `create` when the working archive does not yet exist, otherwise `open_append`. Tiles are written with `write_hyper_tile` / `write_raw_hyper_tile` and Scholar pack `create_human_lrs`. The writer takes `try_lock_exclusive` and holds it for the write. A second writer (another host, or a phone and a host) fails with a lock error. This matches the engine's single-writer durability model. v1 does not define a multi-writer merge.

**ADDON_EDU (DD-16, DD-28).** Scholar writes require `ADDON_EDU`. Creating a new working archive calls `enable_addon(ADDON_EDU)` on that new file. Opening an existing working archive that lacks the bit is a hard error: no tiles are written, the file is left byte-identical, and the error names the missing bit. Kite does not silently set the bit on an archive the operator already had.

**Snapshots (DD-15, DD-09 amended).** A snapshot is a sealed `.s3a` written beside the working archive (same directory, a distinct file name derived from the content SHA-256) containing the captured bytes in existing core BLOB (68) + PROVENANCE (72) tiles, then sealed with `seal_archive` or `seal_archive_with_manifest`. After the seal, `verify_sealed_archive` must succeed and the payload hash must equal SHA-256 of the captured bytes. The working archive itself stays live (appendable) and is not required to be re-sealed after every inclusion. If a snapshot path is opened later with `TileWriter::open_append`, the existing unseal-on-first-write behavior applies; kite does not append to a sealed snapshot in v1.

**Captured bytes and metadata (AT-32).** Exact artifact:

*   `http(s)` page: the raw response body bytes as received (not a screenshot).
*   Downloaded file (including PDF): the file bytes.
*   A wiki or corpus file already on disk under the configured roots is not re-snapshotted; the inclusion record points at the existing object. If the user selects a remote source, the captured bytes are as above.

Provenance fields, and only these, are recorded: content SHA-256, source URL if the source had a URL, and capture time as RFC3339 seconds UTC (`humantime::format_rfc3339_seconds`). No other metadata fields are invented.

Skip does not snapshot and does not write an inclusion record.

### 5.7 Mobile build (Android, then iOS)

s3a-kite is the viewer inside [s3a-reader](https://github.com/ashleycribb/s3a-reader), as a UniFFI library from `s3a-pack-kite`.

**Why this is feasible without a mobile GPU.** Kitesurf already rasterizes on the CPU through `vello_cpu` / `VelloCpuImageRenderer` to an RGBA pixmap. The mobile library paints the manifest and warehouse into a CPU buffer and hands pixels plus the text layer to the UI toolkit. v1 does not add a wgpu or platform GPU backend (DD-21). `cargo tree` for Android and host targets must not include `wgpu` or a Vulkan / Metal / GLES backend at all, not only "on the tour path" (AT-41).

**Fonts (DD-22, DD-35).** Keep `system_fonts: false`. The app ships UA font bytes (Noto Sans, Italic, Serif, Mono, and Symbols) and registers them with the same `register_ua_fonts` / `populate_ua_fonts` path. Missing those required UA fonts is an error, not a fallback to device fonts (AT-42). Noto Color Emoji and Noto Sans SC are optional **bundled asset packs** (Play Feature Delivery, or a sideloaded asset packaged with the APK). They are not fetched over the network. The no-network Android manifest stays in force. When those optional packs are absent, the file still opens: required Latin text is painted with the shipped UA fonts; code points not in those fonts appear as the font's `.notdef` glyph (a visible missing-glyph box), not as an error, and not from system fonts, and with zero network requests (AT-49). Q-05 remains which packs ship in the first Play/App build.

**Android (v1 ship target, already decided for the reader).**

*   UniFFI `cdylib` + Kotlin, cargo-ndk, minSdk 26, ABIs `arm64-v8a` and `x86_64` (DD-23), Jetpack Compose.
*   Compose displays the Rust pixmap (for example `ImageBitmap` / `Bitmap` from RGBA) and exposes the text layer to TalkBack (AT-63). The default screen after open is the kite first view (manifest or header sheet), then the warehouse (AT-43). The warehouse tour does not use a WebView.
*   SAF / share-sheet open paths already in the app stay the way files arrive. Bytes are passed into `open` via the existing `MmapReader::from_bytes` path (no new scratch-file invention). The Kotlin deck plan is no longer the default (DD-24).
*   `s3a-reader-core` verification still runs first and is the engine result kite consumes (DD-19, §5.4 item 2). Quarantined files may be opened with a persistent banner; invalid tiles follow §5.4 item 2 (DD-20).
*   The app's current "no network" manifest remains the default. LRS requests are not made from the phone in v1 (Q-03 draft: leave tracing to the hub/PC host). Scholar **local** writes on the phone use a configured on-device working archive and do not depend on tracing consent (DD-39). If no working archive is configured, select/download fails closed with an error that names the missing archive; it does not silently succeed without a record. Consent-on does not attempt a phone LRS request in v1.

**iOS (feasible, not the first ship).**

*   Feasible because the paint path is portable `std` Rust plus CPU raster, not Metal. UniFFI already planned for Swift in the reader repo.
*   Follow-on, required once the pack links a `staticlib` for `aarch64-apple-ios` and `aarch64-apple-ios-sim` (AT-40, DD-25). SwiftUI (draft) shows the same RGBA frames and text layer.
*   Same font and no-GPU rules.

**Native-link work the pack must do (not optional, not a stub).** Kitesurf's workspace today pins `getrandom` 0.2/0.3 with `js` / `wasm_js` features, and `fonts.rs` uses `#[wasm_bindgen]`. The `native-host` feature already gates some exports. The pack's fork must compile `kitesurf-core` for `aarch64-linux-android`, `x86_64-linux-android`, and later Apple iOS targets **without** the Wasm `getrandom` JS backend and without requiring a JS host. That is a fork change in the pack, with Apache change notices on touched files.

**ESP32 / MCU.** Not a kite viewer target.

**Budgets (DD-06, Q-06).** The "measure on a named minSdk-26 device before adding a second renderer" trigger stays dormant until Ashley names the device and the numeric budgets (peak RSS, time to first manifest frame, APK size delta versus the current reader). This spec does not invent those numbers.

### 5.8 Render API, warehouse scale, and animation

Every paint function returns a frame, not only pixels (DD-38):

```text
RenderFrame {
  rgba: [u8],           # packed RGBA, length = width * height * 4
  width: u32,
  height: u32,          # pixmap pixels; at scale 1.0, 1 CSS px = 1 pixmap px
  text_runs: [...],     # visible text with origin and box, in pixmap coordinates
  semantics: [...],     # role + label + box (document, heading, button, "Continue", floors)
}
```

Acceptance tests that assert on "text" use `text_runs` / `semantics`, never OCR. TalkBack and VoiceOver read `semantics`.

`render_manifest(handle) -> RenderFrame` paints the first view (stored manifest, labeled absent/quarantined/not-UTF-8 state, or Edge derived header sheet).

`render_warehouse_frame(handle, t_seconds, viewport_width, viewport_height, instant) -> RenderFrame` paints the warehouse or a floor-detail transition. `viewport_*` is the pixmap size. The same API is used for hyper-tile and Edge floors.

**Scale (DD-37).** At reference scale 1.0, bays are square and bay *area* is linear in tile stride. A 4 KiB bay is 32×32 CSS px (area 1024 px²). An 8 KiB bay has area 2048 px² (side 32√2 CSS px). A 128 KiB bay has area 32768 px² (side 32√32 CSS px). AT-16 renders the 4 KiB and 8 KiB fixtures **separately** at this scale and compares measured areas; each linear dimension may be off by at most 1 CSS px. The warehouse is a scrollable grid. Floors that do not fit the viewport are reached by scrolling, not by automatically shrinking below the reference scale.

**Animation (DD-18, DD-37).** Without `instant`, the floor-to-detail card starts at t = 0 on the tile rectangle at opacity 0.2, and at t = 0.45 s is at `left = 0.06 * viewport_width`, `top = 0.08 * viewport_height`, `width = 0.88 * viewport_width`, `height = 0.84 * viewport_height`, opacity 1. Progress between those endpoints is the y-value of CSS `cubic-bezier(.2,.8,.2,1)` at unit time `t / 0.45` (at `t = 0.225` s, unit time 0.5, progress is approximately 0.946). Tests sample t = 0, 0.225, and 0.45 (AT-62). With `instant`, only the t = 0.45 end frame is produced. v1 tests do not require a particular capture frame rate beyond those samples.

### 5.9 Robustness

*   Fuzz: a `cargo-fuzz` target `open_and_render` on pack-owned code is required (AT-71). It must run against the AT-08 fixture, MICRO-VEC-01/02/03, and a malformed corpus without aborting the process.
*   Malformed headers, unsupported versions, inconsistent Edge flags, and mmap faults are errors or quarantine, as specified above, not crashes.

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

A generated third-party license inventory (for example `cargo-about` or an equivalent check against the resolved tree, including UniFFI, pulldown-cmark, Wasmtime, and other transitive crates) ships with the distribution and is shown on an in-app About / licenses screen (AT-02, AT-70).

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

Fixtures: hyper-tile archives built the same way as `crates/s3a-cli/tests/tour_tests.rs` (`write_manifest: true` for AT-08); Edge vectors `micro_valid_4kb`, torn and signed vectors from `s3a-spec-vectors`; an 8 KiB file produced with `EdgeWriter<_, 8192>`. Text assertions use the text layer in §5.8.

Mechanical helpers: AT-03 diffs the vendored Kitesurf tree against pin `235111c6fe10f2d63f3476bf5e3d19b4c7089415` and greps each changed file for the change notice. AT-36 uses `cargo deny` (or equivalent) bans plus a grep on pack-owned source only, not the entire vendored upstream tree.

### Pack and license

*   **AT-01** (fail-first). `cargo test -p s3a-pack-kite` exists and the crate's Cargo manifest does not cause `s3a-core` to depend on kite or kitesurf (`cargo tree -p s3a-core` has no such edge).
*   **AT-02** (fail-first). The pack's distribution directory, and the Android APK / iOS IPA when those artifacts are built, contain Apache-2.0 `LICENSE`, Kitesurf `NOTICE` (including the Cloudflare 2026 copyright line and the Stylo / PSL MPL notes), OFL text for every shipped font, and IJG text if jpeg-encoder is linked. A CI check lists these files; a missing NOTICE fails.
*   **AT-03** (fail-first). A diff of the vendored Kitesurf tree against `235111c6fe10f2d63f3476bf5e3d19b4c7089415` lists every modified file, and each of those files starts with or includes a prominent "changed from upstream kitesurf `235111c6fe10f2d63f3476bf5e3d19b4c7089415`" notice in addition to the original copyright (Apache §4).
*   **AT-70** (fail-first). A generated third-party license inventory covers the resolved tree (not only Kitesurf's NOTICE). The distribution includes an MPL source offer for Stylo / PSL. The app's About / licenses screen shows those notices.

### Host

*   **AT-04** (fail-first). `s3a-kite host --bind 127.0.0.1 --port <ephemeral> --auth-token-file <token>` plus the required root / working-archive / consent flags starts without Wrangler and without a Worker Loader, and `GET http://127.0.0.1:<port>/health` returns success without a token. Today there is no such binary.
*   **AT-05** (guard once the binary exists; skipped until then). That binary does not read `packages/engine/wrangler.jsonc` and does not call a `LOADER.load` API.
*   **AT-06** (fail-first). The same host, without a valid token, rejects every HTTP route other than loopback `GET /health`, and rejects every CDP upgrade (non-2xx / no session). An open listener is a failure.
*   **AT-07** (fail-first). Omitting `--port` or `--bind` in any mode exits non-zero with an error that names the missing flag. No implicit port is bound.
*   **AT-50** (fail-first). On a non-loopback bind with TLS (or `--behind-proxy` in the test harness), `GET /health` without a token is rejected; with a valid Bearer token it succeeds.
*   **AT-51** (fail-first). `s3a-kite host --bind 0.0.0.0 --port <ephemeral> --auth-token-file <token>` without `--tls-cert`/`--tls-key` and without `--behind-proxy` exits non-zero and binds nothing.
*   **AT-45** (fail-first). Startup with an empty `--auth-token-file`, or a token file shorter than 32 bytes of secret material, exits non-zero, names the token file, and binds nothing.
*   **AT-52** (fail-first). A UI-token request to open a path containing `../` that would escape `--corpus-root` or `--wiki-root` is rejected and opens no file.
*   **AT-53** (fail-first). A `file://` URL whose path is outside the configured roots is rejected.
*   **AT-54** (fail-first). A page fetch to `http://169.254.169.254/` (and to the configured LRS private address, when one is configured) is blocked; the request does not leave the host.
*   **AT-55** (fail-first). The page process cannot read the host working archive, token file, or a file under `--corpus-root` except through the host proxy's allowed-root rules.

### Hyper-tile `.s3a`

*   **AT-08** (fail-first). Open a sealed archive produced like `test_tour_displays_manifest_in_lobby_and_floor`. The first frame's text layer contains the exact stored manifest payload (including `THIS IS AN S3A FILE` and the seal title/creator). The warehouse floors are not the first frame.
*   **AT-09** (fail-first). Open a hyper-tile archive with no tile 78. The first frame is a labeled "manifest absent" state and does not contain invented title/creator text. After `proceed_to_warehouse` (or the Continue control), the warehouse tour is shown.
*   **AT-10** (fail-first). After the manifest view, the tour floor list matches `s3a-cli tour --json` on the same hyper-tile file: `index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`.
*   **AT-11** (fail-first). Opening floor *N* on a golden hyper-tile fixture whose `withheld_count` was hand-verified from core `ConsentRecord` data shows those records and that `withheld_count`. Fixtures that contain type 82 are included in this suite and must *not* treat type 82 as Edge consent. The CLI is not the `withheld_count` oracle when type 82 is present.
*   **AT-12** (fail-first). A manifest or record that contains `<script>`, `</script>`, or HTML angle brackets is painted as those characters (text layer). No script from the archive runs.
*   **AT-13** (fail-first). Without `instant`, the floor-to-detail transition matches §5.8 (AT-62 is the timed sampling). With `instant`, the same detail view appears as the end frame with no intermediate frames.
*   **AT-62** (fail-first). `render_warehouse_frame` at t = 0, 0.225, and 0.45 s places the card rectangle and opacity at the §5.8 start, bezier mid, and end values, within 1 CSS px (and 0.02 on opacity).
*   **AT-64** (fail-first). A committed tile 78 that is not valid UTF-8 yields a first frame labeled "manifest not UTF-8" and invents no replacement text.
*   **AT-65** (fail-first). Two committed valid tile-78 tiles: the first view uses the first tile's bytes; the second appears as a floor.
*   **AT-69** (fail-first). A file whose `FileHeader.version` is not 1 fails to open with `Unsupported file version: {n}`.
*   **AT-72** (fail-first). An unsealed hyper-tile with a valid tile 78 opens, the first view's text layer includes the manifest and an unsealed seal status, and no seal trailer is invented.

### Edge micro-tiles

*   **AT-14** (fail-first). Open `micro_valid_4kb.s3a` (MICRO-VEC-01). The reader reports `is_edge_archive` and `tile_stride == 4096`. The first frame is the derived header sheet, labeled as derived, and includes stride 4096 and the committed `tile_count`. It is not presented as tile 78.
*   **AT-15** (fail-first). The warehouse list for MICRO-VEC-01 has exactly the committed micro-tiles. Each floor `size_bytes == 4096`. Offsets follow `512 + i * 4096` for Variant A.
*   **AT-16** (fail-first). An 8 KiB `EdgeWriter` fixture shows `tile_stride == 8192` and `size_bytes == 8192`. Rendered alone at the §5.8 reference scale, its bay area is twice the bay area of a separately rendered 4 KiB fixture at the same scale, with each linear dimension allowed ±1 CSS px.
*   **AT-17** (fail-first). MICRO-VEC-02: only the valid tile(s) appear as floors; the torn tile is omitted and the header sheet reports it as ignored/torn.
*   **AT-18** (fail-first). MICRO-VEC-03: seal verification matches `s3a-engine`. A copy with a reordered or mutated tile fails verification, shows quarantine, and does not list the mutated tile (or any subsequent tile) as a valid floor.
*   **AT-19** (fail-first). A type-64 tile of 32-byte `EquipmentEventRecord`s paints timestamp, the EDGE §5.2.2 event name, severity, and the category label from §5.4. Severity 0 is green, 1 yellow, 2 and 3 red (pixel samples at the severity badge). A severity-0 maintenance event is green and still labeled "maintenance".
*   **AT-20** (fail-first). A type-64 tile whose record size is 96 bytes paints `DeviceIdentityRecord` fields (device id, hardware model), not event columns.
*   **AT-21** (fail-first). A compact-header Edge file (`FLAG_COMPACT_HEADER`) places tile 0 at offset 4096 (4 KiB stride) or 8192 (8 KiB stride), not at 512.
*   **AT-22** (guard; already passes today). `crates/s3a-edge/Cargo.toml` does not depend on kite or kitesurf. A `no_std` build of `s3a-edge` still succeeds.
*   **AT-23** (fail-first). When an Edge type-71 `EdgeConsentRecord` does not allow the research purpose, all kite views withhold records and report `withheld_count`. The implementation must use the Edge layout, not type 82 and not core `ConsentRecord`.
*   **AT-66** (fail-first). A file with `FLAG_PROFILE_EDGE` and a 128 KiB stride, and a file with `FLAG_TILE_SIZE_4KB` and stride 8192, each open into the "inconsistent Edge header" quarantine state and are not toured as a consistent Edge or hyper-tile file.
*   **AT-67** (fail-first). A type-64 tile whose record size is neither 32 nor 96, or whose `record_count` is 0, is shown as an "unknown layout" floor.
*   **AT-68** (fail-first). An otherwise valid Edge file with no `DeviceIdentityRecord` opens; the header sheet omits device fields and is labeled "device identity record absent".

### Scholar 3-pass and snapshots

*   **AT-24** (fail-first). Building the Pass 1 candidate list twice leaves the corpus archive, the LRS store, and the host log and data directories byte-identical to the start (no `HumanLrsRecord`, no xAPI statements, no new snapshot tiles, no paper ids or URLs in logs).
*   **AT-25** (fail-first). Dismissing a paper as off-topic / skip writes nothing (same byte-identity check, including host log and data directories).
*   **AT-61** (fail-first). After a consent-off select, host log and data directories contain no paper ids or source URLs. The working archive still has the inclusion record.
*   **AT-26** (fail-first). Selecting or downloading a paper, with tracing consent on and a configured LRS, writes one inclusion statement whose verb IRI is taken from `s3a-pack-scholar` `xapi.rs` (not a newly invented IRI) and a Scholar inclusion record for that paper id in the working archive, plus a snapshot when bytes were captured.
*   **AT-27** (fail-first). The same select with consent off: zero HTTP requests to the LRS, and the working archive still contains the inclusion record (and a snapshot when bytes were captured). The corpus and LRS store are unchanged.
*   **AT-48** (fail-first). Download with consent off still delivers the file and writes the local inclusion record and snapshot, with zero LRS requests.
*   **AT-28** (fail-first). An API call that tries to write include/exclude/score without a UI-token human select or download is rejected and writes nothing.
*   **AT-57** (fail-first). An agent-token session that synthesizes a select or a CDP-driven click on select is rejected and writes nothing.
*   **AT-29** (fail-first). The Pass 1 timer reaching 15 seconds does not create or delete any record.
*   **AT-30** (fail-first). Opening a configured wiki `index.md` paints its heading text from the file bytes (text layer). Opening a missing `papers/SRC-9999.md` reports missing and does not invent a dossier.
*   **AT-56** (fail-first). A wiki page whose Markdown contains a script tag that would set `document.title` and an image tag with an `http` URL paints those tags as text or omits them as raw HTML; no script runs; the render issues zero network requests.
*   **AT-31** (fail-first). After an inclusion (consent on or off), a sealed snapshot exists whose payload hash equals SHA-256 of the captured bytes; re-open verifies with `verify_sealed_archive`. Skip does not create a snapshot.
*   **AT-32** (fail-first). That snapshot's provenance record carries content SHA-256, capture time as RFC3339 seconds UTC, and, if the source had a URL, that URL. No other metadata fields are present.
*   **AT-46** (fail-first). Select against a working archive whose `addon_mask` lacks `ADDON_EDU` fails, names the missing bit, and leaves that file and the LRS store byte-identical.
*   **AT-47** (fail-first). After a successful snapshot write, `verify_sealed_archive` on the snapshot file succeeds.
*   **AT-58** (fail-first). Revoking tracing consent mid-session (consent file goes `on` → `off`) drops subsequent LRS requests to zero, including any retry queue; the already-written local inclusion record remains.
*   **AT-59** (fail-first). With consent on, an LRS 5xx does not roll back the local inclusion record or snapshot; a retry is queued; the corpus is unchanged.
*   **AT-60** (fail-first). Selecting the same paper twice in one working archive leaves a single inclusion record and a single snapshot.
*   **AT-73** (fail-first). A second `TileWriter` / host that cannot take `try_lock_exclusive` on the working archive fails the write and does not truncate the file.
*   **AT-74** (fail-first). Select with no `--working-archive` configured (host) or no on-device working archive (phone) fails closed and writes nothing.

### Web pages vs archives

*   **AT-33** (fail-first). Opening `https://example.com/` uses the HTML/CSS path in a sandboxed page process (Wasmtime + `kitesurf-core` Wasm, DD-26), not the `.s3a` tour path. A page script that sets `document.title` changes the title observed through the host's CDP `Runtime.evaluate` / document title surface.
*   **AT-34** (fail-first). Opening a `.s3a` file does not create a Boa context, a Wasmtime page isolate, or a page-script isolate for the tour. The tour is painted by the native `vello_cpu` renderer.

### Safety and scope

*   **AT-35** (fail-first). A hyper-tile or Edge file with a failed tile CRC or a failed Ed25519 check is labeled quarantined. The failing tile is not shown as a valid floor.
*   **AT-36** (guard once the pack crate exists; skipped until then). `cargo deny` (or equivalent) plus a grep on pack-owned source and tests (not vendored Kitesurf or Wasmtime) contain no Bitcoin, OpenTimestamps-write, payment, or token-transfer code paths. Reading Edge `seal_hash` values is allowed.
*   **AT-37** (guard; already passes today). No new `TileType` constant is added under `s3a-core`.
*   **AT-71** (fail-first). The `open_and_render` cargo-fuzz target exists and, on the AT-08 fixture, MICRO-VEC-01/02/03, and a malformed file, does not abort.

### Mobile library

*   **AT-38** (fail-first). The UniFFI library builds for `aarch64-linux-android` and `x86_64-linux-android` with minSdk 26. `open` + `render_manifest` + `render_warehouse_frame` return a `RenderFrame` (RGBA plus text layer). For the AT-08 fixture, the first frame's text layer contains `THIS IS AN S3A FILE`; for MICRO-VEC-01, the first frame's text layer includes `4096`.
*   **AT-39** (fail-first). The Android UI path that shows the tour does not instantiate `android.webkit.WebView` for that screen. A lint or unit test on the Compose screen's view tree fails if a WebView is the tour surface.
*   **AT-40** (fail-first). `cargo build --target aarch64-apple-ios -p s3a-pack-kite` produces a `staticlib` (and the `ios-sim` target also links). Shipping the iOS app remains a follow-on (DD-25); this test is the feasibility gate.
*   **AT-41** (guard once the pack crate exists; skipped until then). `cargo tree` for `s3a-pack-kite`, for Android and host targets, does not include `wgpu` or a Vulkan/Metal/GLES backend at all. The linked paint crate is `vello_cpu` / `anyrender_vello_cpu`.
*   **AT-42** (fail-first). With the required UA fonts removed from the test fixture, `render_manifest` returns an error that names missing fonts. It does not silently render with system fonts (`system_fonts` stays false).
*   **AT-43** (fail-first). In the Android app, opening the AT-08 fixture shows the kite surface as the default screen after open (first the manifest, then the warehouse after Continue). `MapLayout` is not the default.
*   **AT-44** (fail-first). `getrandom` for Android and iOS targets is not enabled with the `js` / `wasm_js` features. A build without a JavaScript runtime succeeds (the fork change required in §5.7).
*   **AT-49** (fail-first). A manifest containing CJK characters, with Noto Sans SC and Color Emoji absent, opens successfully; required Latin text is in the text layer; CJK code points are painted as `.notdef` (not from system fonts); zero network requests.
*   **AT-63** (fail-first). The first-view semantics tree includes the manifest heading (or the "manifest absent" label). An Android TalkBack (or equivalent accessibility) test reads that label from the kite surface.

--------------------------------------------------------------------------------

## 8. Draft decisions

Ashley may accept a draft, change it, or leave it open. Draft values follow. DD-01–DD-25 keep their topics. Changes of meaning are noted in the "Notes" column and in the round-1 disposition.

| ID | Topic | Draft value | Notes |
| --- | --- | --- | --- |
| **DD-01** | Pack location | `crates/s3a-pack-kite` in the s3a-storage workspace, with the Kitesurf Rust core vendored from `ashleycribb/s3a-Kite` at commit `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. | Pin updated from `4a29038`. Names in §5.1 are now fixed. |
| **DD-02** | Host compile mode | Link `kitesurf-core` and `renderer-core` with `native-host` as native rlibs. Wasmtime is not required for the `.s3a` tour. | Unchanged for the tour. Wasmtime *is* used for `http(s)` page JS (DD-26). |
| **DD-03** | Bind address and port | Required flags `--bind` and `--port`. No silent production default in any mode. Docs may show `127.0.0.1:8787` as an example because that is upstream Kitesurf's local port. | Clarified "any mode" (F-07). |
| **DD-04** | Visual metaphor | Call the UI a warehouse. Data model remains lobby + floors. Bays map 1:1 to floors. Bay area is linear in tile stride. | Reference scale is in DD-37; meaning of "linear in stride" is unchanged. |
| **DD-05** | Device-side stored manifest | Not written on the MCU in v1 (RAM and flash budget; no tile 78 in the Edge profile). The hub/PC/phone shows the derived header sheet. If firmware later stores UTF-8 manifest text, it must use a declared tile type, and the viewer must prefer those stored bytes. | Unchanged. |
| **DD-06** | Slimmed renderer / MCU | v1 uses the same `native-host` + `vello_cpu` renderer on hub, PC, and phone. No second renderer until a measured memory budget on a named device fails. Never on the microcontroller. | The trigger stays dormant until Q-06 is answered. No invented budgets. |
| **DD-07** | Event colors | Color by severity: 0 green, 1 yellow, 2 and 3 red. Print `event_type` as the EDGE §5.2.2 name, and show the non-color category label in §5.4. | **Overrides** EDGE §10 category colors. Color mapping itself is unchanged; the override and the label are new (F-24). |
| **DD-08** | Pass 1 timer | Show a 15-second skim timer. Expiry does nothing to records. | Unchanged. |
| **DD-09** | Snapshot and inclusion vs consent | Snapshots and inclusion records of selected papers are local working-archive writes and do **not** require tracing consent. Skip does not snapshot. Tracing consent gates only LRS xAPI. | **Meaning changed** by Ashley's standing rule (F-03). |
| **DD-10** | Device transport | v1 opens files already on the hub, PC, or phone. BLE/USB/SoftAP stay Edge Phase 2. | Unchanged. |
| **DD-11** | Host authentication | Fail closed. `--auth-token-file` points at a token that is not in the repository. Unauthenticated requests are rejected. | Transport, TLS, and token-file rules are in DD-29. |
| **DD-12** | LRS URL | Must be configured for LRS emission. No invented localhost default in production. If consent is on and the URL is missing, the LRS statement fails closed with an error; it does not silently skip the statement. Local inclusion and snapshot still write (DD-09). | **Narrowed** to the LRS path (F-03, F-04). |
| **DD-13** | Markdown | The pack parses CommonMark with `pulldown-cmark` (raw HTML disabled or escaped) to HTML, then `kitesurf-core` / Blitz lays out that HTML. No JS and no remote subresources on wiki/corpus pages. | Tightened for F-12; library choice unchanged. |
| **DD-14** | PDF pages | v1 does not rasterize PDF input. Skim shows title, abstract, and any markdown conversion already in the archive. If that conversion is missing, show "no text conversion in archive", not a generated summary. | Unchanged. |
| **DD-15** | New tile IDs | None in v1. Snapshots use existing BLOB + PROVENANCE. Inclusion uses Scholar tiles when `ADDON_EDU` is set. | Unchanged. |
| **DD-16** | Addon mask | No new addon bit in v1. Scholar writes require `ADDON_EDU`. Edge files already carry `ADDON_WEARABLE`. A missing bit on an existing working archive is a hard error (§5.6). | Behavior when missing is now specified (F-02). |
| **DD-17** | Upstream pin | Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`. | Pin updated from `4a29038`. |
| **DD-18** | Animation timing | Match `tour_html_template.html`: 0.45s `cubic-bezier(.2,.8,.2,1)` from the tile rect to 6vw/8vh/88vw/84vh, skipped when `instant` is set. | Frame API is in DD-37; timing unchanged. |
| **DD-19** | App relationship | s3a-kite is the viewer inside `s3a-reader`. Verification stays in `s3a-reader-core`, which is the `s3a-engine` result kite consumes. Kite does not replace the edge writer or the MCU firmware, and does not run a second verifier. | Clarified authority (F-23). |
| **DD-20** | Quarantine UX | Follow the existing reader: a quarantined file can be opened with a persistent banner. Failed-CRC, torn, and chain-broken tiles are not valid floors (§5.4 item 2). | Unified with the chain-break rule (F-27). |
| **DD-21** | GPU | v1 stays on `vello_cpu`. No wgpu, Vulkan, Metal, or GLES backend at all in the pack dependency tree. | Tightened (F-35). |
| **DD-22** | Mobile fonts | Ship Noto Sans, Italic, Serif, Mono, and Symbols with the app and register them as UA fonts. `system_fonts` remains false. Color Emoji and CJK SC are optional bundled asset packs, not required to open a file, and not downloaded over the network. | "On-demand" redefined (F-05). Missing-glyph behavior is DD-35. |
| **DD-23** | Android ABIs and UI | minSdk 26, `arm64-v8a` and `x86_64`, UniFFI + Kotlin, Jetpack Compose displays the Rust pixmap and text layer. No 32-bit ARM in v1. | Unchanged except text layer. |
| **DD-24** | Kotlin deck plan | The kite warehouse is the primary view and the default screen after open. `MapLayout` may remain as a secondary diagnostic screen, not the default. | Aligned with AT-43 (F-35). |
| **DD-25** | iOS | Feasible (CPU raster + UniFFI Swift). Ship Android first. iOS follows when AT-40 stays green and a SwiftUI surface shows the same frames. | Unchanged. |
| **DD-26** | Page JS on a native host | v1 runs `http(s)` page JS by loading the existing `wasm32` `kitesurf-core` (Boa) under Wasmtime 36 in one sandboxed OS process per page. The host replaces `packages/page-script` / `LOADER.load`. Native Boa is out of v1. JS-off is not the draft. | New (F-01). |
| **DD-27** | Write target | Inclusion records and snapshots go to a configured per-researcher `--working-archive`. The corpus is read-only to kite. | New (F-02). |
| **DD-28** | Write API, seal, lock | `TileWriter::create` / `open_append` + Scholar `create_human_lrs`; exclusive `try_lock_exclusive`; new archives get `ADDON_EDU`; existing archives without the bit fail closed; snapshots are separately sealed with `seal_archive` and `verify_sealed_archive`; the working archive stays live. | New (F-02). |
| **DD-29** | Health, token, TLS | `GET /health` as specified in §5.2; Bearer token; token-file rules; TLS or `--behind-proxy` for non-loopback binds. | New (F-08, F-09). |
| **DD-30** | Roots and CDP scope | `--wiki-root`, `--corpus-root`, `--working-archive`; canonicalize and reject escapes; UI vs agent tokens; CDP domain allowlist in §5.2. | New (F-10). |
| **DD-31** | Page sandbox and egress | One OS process per page, no inherited archive fds, host-proxied network, default-deny private/link-local/metadata/LRS, optional `--egress-allowlist`. | New (F-11). |
| **DD-32** | Human vs agent | UI token may write inclusion; agent token is read-only; no "trusted-gesture" heuristic beyond the token kind. | New (F-13). |
| **DD-33** | Tracing consent model | `--consent-state-file` (`on`/`off` + RFC3339 UTC timestamp); `--researcher-id` required for statements and `HumanLrsRecord`; revoke drops LRS and the retry queue, keeps local records. | New (F-14). |
| **DD-34** | Inclusion side effects | Order: local record, then snapshot, then LRS; LRS failure does not roll back local writes; idempotent per working archive + paper + verb; Pass 1 does not log paper ids. | New (F-15, F-16). |
| **DD-35** | Missing optional fonts | Absent CJK/emoji packs: open succeeds, `.notdef` for missing glyphs, no system fonts, no network. | New (F-05). |
| **DD-36** | Manifest-to-tour trigger | Explicit Continue / `proceed_to_warehouse` only; no timer; long text wraps and scrolls. | New (F-25). |
| **DD-37** | Warehouse scale and frame API | §5.8 reference scale, scrollable grid, `render_warehouse_frame(t, viewport, instant)`, bezier samples. | New (F-17, F-18). |
| **DD-38** | Text and semantics layer | Every frame returns `text_runs` and `semantics`; tests and TalkBack use it, not OCR. | New (F-19). |
| **DD-39** | Phone Scholar writes | v1 phone: local working-archive writes, no LRS, no network. Missing working archive fails closed. Tracing stays on the hub/PC (Q-03 draft). | New (F-04). |
| **DD-40** | Inconsistent Edge headers | Quarantine; do not guess a stride or profile. Missing identity: warn and open. Unknown type-64 layout: labeled floor. | New (F-28, F-29). |

--------------------------------------------------------------------------------

## 9. Open questions

*   **Q-01.** Lab host authentication: token file (DD-11, DD-29) or an existing lab SSO? Draft is the token file until Ashley names an SSO.
*   **Q-02.** Exact LRS base URL on the s3a-lrs VM, and the consent-copy shown before tracing.
*   **Q-03.** Whether phone builds should ever enable network for LRS, or keep the current no-network reader and leave Scholar tracing to the hub/PC host. Draft (DD-39): leave tracing to the hub/PC in v1.
*   **Q-04.** Preferred CommonMark library if not `pulldown-cmark` (DD-13).
*   **Q-05.** Whether the optional CJK and emoji font packs should ship in the first Play/App build or only as additional bundled / Feature Delivery packs. They are not downloaded over the network from the no-network app.
*   **Q-06.** Named minSdk-26 measurement device, and numeric budgets for peak RSS, time to first manifest frame, and APK size delta versus the current reader, so DD-06's second-renderer trigger can be tested. No numbers are invented here.
*   **Q-07.** Which existing verb IRI in `s3a-pack-scholar` `xapi.rs` is used for select and which (if different) for download. This spec will not invent a new IRI.
*   **Q-08.** How `--researcher-id` is provisioned in the lab (manual flag, existing Scholar session, or something Ashley names).

Lawyer review is not required for the fork itself (Apache-2.0 + MPL file-level terms are listed in §6), but distribution of the app's font files must keep OFL notices (AT-02, AT-70).

--------------------------------------------------------------------------------

## 10. Risks

1.  **`native-host` is a thin feature flag.** Much of Kitesurf's DOM and JS is `#[cfg(target_arch = "wasm32")]`. The tour path should use paint/layout entry points that already have native-host or non-Wasm coverage (`scene_replay`, `VelloCpuImageRenderer`). The `http(s)` path (DD-26) uses Wasmtime specifically because those JS entry points are wasm32-only. Additional native entry points may still be needed in the fork for the tour.
2.  **Wasm-oriented dependencies.** Workspace `getrandom` uses `js` / `wasm_js`. `fonts.rs` uses `wasm_bindgen`. Android and iOS builds fail until the pack forks those bits (AT-44). Wasmtime on the host also has to be confined to the page process.
3.  **Today's `s3a-cli tour` is not a drop-in Edge tour.** It steps 128 KiB tiles and has a type-82 consent *branch* (not only a comment) that does not match the Edge registry. Kite must use `MmapReader` edge APIs and `edge_display_layouts()`, and must not use the CLI as the `withheld_count` oracle on files that contain type 82.
4.  **Type-number reuse** (64, 71, 75) between hyper-tile and Edge layouts. Branching on `is_edge_archive` is mandatory.
5.  **Phone memory and binary size.** Full Stylo + Blitz + UA fonts is heavier than the current Kotlin map. DD-06 and DD-22 exist because this may force a later slim. The trigger cannot fire until Q-06 is answered.
6.  **No PDF input renderer** in the Kitesurf tree (Krilla is output). Pass 1 can look incomplete for PDF-only papers (DD-14).
7.  **Kitesurf is experimental** (README; last grounded commit 2026-11-16, `235111c`). Upstream will move. The pin is explicit (DD-17). `235111c` itself is TypeScript-only, but future upstream commits may touch Rust.
8.  **Host auth.** Shipping a fail-open CDP port would repeat upstream's warning. AT-06, AT-50, and AT-51 are gates.
9.  **License completeness.** Forgetting NOTICE, MPL source, the third-party inventory, or OFL/IJG texts is a release blocker (AT-02, AT-70).
10. **Working-archive durability.** Unsealing or locking incorrectly could damage a researcher's records. AT-46, AT-47, and AT-73 are gates. The pack must use the existing `TileWriter` unseal-on-write and lock APIs, not a new writer.

--------------------------------------------------------------------------------

## 11. Implementation notes (after tests exist and fail)

Order of work, when Ashley authorizes code. Do not push to s3a-Kite, s3a-storage, or s3a-reader until she says so.

1.  Land the fail-first acceptance tests in s3a-storage and s3a-reader so they fail for the documented reason. Land the guard tests so they stay green. Names in §5.1 are already fixed, so this step does not wait on "names as implemented."
2.  Vendor the Kitesurf Rust core from `ashleycribb/s3a-Kite` at `235111c`, add LICENSE/NOTICE, enable `native-host`, fix getrandom/wasm_bindgen for native and Android targets, and keep Apache change notices (AT-03).
3.  Implement hyper-tile manifest + warehouse paint (text layer, §5.8 animation) against `tour_file` data for hyper-tiles, and `MmapReader` for Edge. Do not copy the type-82 consent branch or the 128 KiB unsealed scanner.
4.  Host binary with required bind, port, token, TLS/proxy, health, roots, and working archive.
5.  Write path, Scholar 3-pass gates, snapshots, and consent/LRS split.
6.  Wasmtime page-process path for `http(s)` (DD-26) and egress/sandbox tests.
7.  UniFFI + Android Compose surface in `s3a-reader` (tests already landed in step 1); then the iOS `staticlib` link gate.

Do not "temporarily" call the old WebAssembly viewer, invent a manifest, walk Edge files with `HYPER_TILE_SIZE`, treat type 82 as Edge consent, gate local inclusion on tracing consent, or invent a new `TileType` or verb IRI.

--------------------------------------------------------------------------------

## 12. Document history

*   2026-11-16: v1. Tests-first pack spec from Ashley's 2026-11-16 direction, grounded in Kitesurf `4a29038`, s3a-storage `b7bdcda`, and s3a-reader `792742c`.
*   2026-11-16: v2. Rebases the Kitesurf pin to `235111c` (Ashley's `s3a-Kite` fork; TypeScript-only upstream commit). Addresses goldfish round 1 (F-01–F-38). Splits local inclusion from tracing consent (F-03). Adds the write path (F-02) and the native page-JS model (F-01). Acceptance tests: AT-01–AT-74 (AT-22 and AT-37 reclassified as guards that already pass; AT-05/AT-36/AT-41 are guards once their subject exists). Draft decisions: DD-01–DD-40. Open questions: Q-01–Q-08.
