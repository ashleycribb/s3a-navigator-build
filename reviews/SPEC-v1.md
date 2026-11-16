# s3a-kite: S3A Browser Pack and Viewer

*   **Author:** Grok Bot (elephant) for Ashley Cribb
*   **Status:** v1 — spec only. Not approved for implementation until Ashley accepts or revises the draft decisions. No code, no GitHub fork, and no push is authorized by this document.
*   **Date:** 2026-11-16 (America/New_York)
*   **Ashley direction (chat, 2026-11-16):** fork Cloudflare Kitesurf as an S3A add-on pack; first use is Scholar's wiki browser for Keshav's 3-pass method; sealed snapshots; opening a `.s3a` file shows the plain-text manifest first, then a warehouse-based animated tour; later the same day, s3a-kite *is* the S3A tool and viewer (it reads `.s3a` in Rust and draws the manifest and tour itself; it does not host the old WebAssembly viewer); it also reads Edge micro-tile files (4–8 KB, `s3a-edge`); the browser runs on a hub, PC, or phone, not on the microcontroller; and s3a-kite replaces the existing S3A viewer and becomes the mobile viewer inside the planned S3A mobile app (Android phone reader, Edge Phase 4).
*   **Grounded commits (read, not modified):**
    *   Kitesurf (shallow clone at `/tmp/kitesurf`): `4a2903813a22b08d4f3cc64a4b35339d0293a5d1` (2026-11-13, `page-script: match leading comments before a doctype in linear time`), upstream [cloudflare/kitesurf](https://github.com/cloudflare/kitesurf).
    *   s3a-storage (local `/workspace/s3a-storage`): `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6` (2026-11-15).
    *   s3a-reader (local `/workspace/s3a-reader`): `792742ca9c1659cb5fd4766237d13c2f5ae24404` (2026-10-28), engine pin `ff89564`.

This is a tests-first pack contract. Acceptance tests are listed before any implementation guidance. Each test names a concrete fixture and an expected result that does not hold in today's trees: there is no `s3a-kite` / `s3a-pack-kite` crate, `s3a-cli tour` does not walk Edge strides, and the Android app's map is Kotlin `MapLayout`, not this renderer.

No stubs, fallbacks, or invented runtime defaults. Anything that needs Ashley's judgment is a **DD** with a draft value, or an open question.

--------------------------------------------------------------------------------

## 1. Overview

`s3a-kite` is an S3A **add-on pack** that forks Kitesurf's Apache-2.0 Rust core and replaces Kitesurf's Cloudflare TypeScript host (Dynamic Workers, Durable Objects, Wrangler) with a host that runs on an Aura Rack, the LRS VM, or Cloud Run, and with an embedded library that runs inside the S3A mobile app.

s3a-kite **is** the S3A viewer. It is not a wrapper around the existing Scholar / `s3a-wasm-snapshot` / harness WebAssembly viewer. It reads `.s3a` files and Edge micro-tile files directly with `s3a-engine` and `s3a-edge`, and it paints the plain-text manifest (or the Edge header sheet) and the warehouse tour with Kitesurf's Rust layout and CPU rasterizer.

First product use: Scholar's wiki browser for Keshav's 3-pass literature method. The Markdown wiki and the S3A research corpus are rendered in the same engine. Pass 1 is a human skim of about 15 seconds. Off-topic papers are never recorded. Selecting or downloading a paper is the inclusion decision. Tracing is consent-only xAPI to `s3a-lrs-server` on the s3a-lrs VM. AI and the browser only check that things exist and record what the researcher did; humans make the judgments.

S3A core stays database-only. No money and no cryptocurrency. Everything lives in the pack.

--------------------------------------------------------------------------------

## 2. Glossary

| Term | Meaning in this spec |
| --- | --- |
| Pack | A domain crate that extends S3A without changing `s3a-core`. See `docs/pack-contract.md`. |
| Kitesurf core | Rust crates `packages/core` (`kitesurf-core`), `packages/renderer-core`, and `packages/kitesurf-cli`, kept and forked. |
| Cloudflare layer | TypeScript `packages/engine` plus `wrangler.jsonc`, the Worker Loader binding `LOADER`, and Dynamic Workers used to start a page isolate. Replaced, not required at runtime. |
| Host | The s3a-kite process that listens for users or agents on a hub, Aura Rack, LRS VM, or Cloud Run. |
| Mobile library | The same Rust renderer, exposed through UniFFI, linked into the S3A Android (and later iOS) app. |
| Hyper-tile archive | A standard S3A file whose tiles have a 128 KB stride. |
| Edge archive | An S3A file with `FLAG_PROFILE_EDGE` and/or `tile_stride` 4096 or 8192, written by `#![no_std]` `s3a-edge`. Still a valid S3A file (`S3A-EDGE-PROFILE` §1.2). |
| Micro-tile | One 4,096-byte or 8,192-byte Edge tile (`MICRO_TILE_4KB_SIZE` / `MICRO_TILE_8KB_SIZE` in `crates/s3a-edge/src/header.rs`). |
| Lobby | `TourLobby` in `crates/s3a-cli/src/tour.rs`: file version, footer, tile count, seal status, consent summary, schema names, optional UTF-8 `manifest`. |
| Floor | One tile in the tour directory (`TourFloorItem`). The HTML tour zooms a floor into detail. |
| Warehouse tour | Ashley's name for the animated walk of those floors. The data model stays lobby + floors. The UI labels it a warehouse (DD-04). |
| Stored manifest | The UTF-8 payload of `TileType::MANIFEST` (78), shown by `s3a-cli tour` under `--- PLAIN-TEXT MANIFEST ---`. |
| Derived header sheet | For Edge files, which have no tile 78 in the Edge profile, a plain-text readout of `FileHeader` and `DeviceIdentityRecord` fields that are actually present, labeled as derived. |
| Pass 1 | The researcher's own skim, about 15 seconds per paper. Listing candidates writes nothing. |
| Inclusion | The researcher selects or downloads a paper for Pass 2/3. That is the recorded Pass 1 event, and only with tracing consent. |

--------------------------------------------------------------------------------

## 3. Non-goals (v1)

*   Implementing the pack in this commit. This document is spec only.
*   Forking or publishing Kitesurf on GitHub.
*   Changing `s3a-core` record layouts or adding kite tile IDs to core (DD-15).
*   Running s3a-kite, Kitesurf, or a GPU renderer on the microcontroller. `s3a-edge` stays `no_std` and heapless.
*   BLE / USB / SoftAP transport (Edge Phase 2 / `S3A-EDGE-PROFILE` §10.1). The mobile app already opens local files; v1 kite renders those files (DD-10).
*   Hosting or embedding the existing `s3a-wasm-snapshot` / harness WebAssembly viewer.
*   Rasterizing PDF input. Kitesurf's Krilla dependency is PDF *output*. Pass 1 uses title, abstract, and any markdown conversion already in the corpus (DD-14).
*   Money, cryptocurrency, or writing Bitcoin / OpenTimestamps anchors. Reading an existing on-device SHA-256 seal chain is required for Edge verification.
*   Cloudflare Workers Paid, Wrangler, or Dynamic Workers as a runtime dependency.

--------------------------------------------------------------------------------

## 4. Grounded facts

Claims about Kitesurf, S3A, Edge, and the phone reader are taken from the trees named above. File paths below are in those trees.

### 4.1 Kitesurf

*   Experimental browser engine that parses HTML, applies CSS, runs page JavaScript, and renders screenshots without Chrome (`/tmp/kitesurf/README.md`). Licensed Apache-2.0 (`LICENSE`); bundled fonts and crates keep their licenses (`NOTICE`).
*   Cloudflare deployment requires Dynamic Workers (Workers Paid) and `wrangler dev` locally (`README.md` "IMPORTANT" block). `packages/engine/wrangler.jsonc` declares `"worker_loaders": [{ "binding": "LOADER" }]`. Page isolates are started with `env.LOADER.load({...})` in `packages/engine/src/eval/page-script.ts` (around line 376).
*   The engine has no authentication of its own. A person who has the URL can open DevTools, connect Puppeteer, and send outbound requests (`README.md`, Cloudflare deployment warning).
*   Stack, from `README.md` "Technology stack": Blitz (HTML/DOM/layout/paint, MIT OR Apache-2.0); Stylo (CSS, **MPL-2.0**); Boa (page JavaScript inside WebAssembly, Unlicense OR MIT); Linebender libraries (Parley, Vello, Kurbo, Peniko, Apache-2.0 OR MIT); Krilla (PDF output, MIT OR Apache-2.0).
*   `packages/core/Cargo.toml` describes `kitesurf-core` as "Wasm core for kitesurf: DOM data plane + Stylo + Blitz render." `crate-type` is `["cdylib", "rlib"]`. Feature `native-host` is declared and is used to drop `wasm_bindgen` exports (for example `packages/core/src/paint/scene_replay.rs`, `tty/grid.rs`).
*   **Paint backend is CPU, not GPU.** Workspace pins `vello_cpu` 0.3 with features `std`, `text`, `u8_pipeline`, `poly-pnorm`, and `anyrender_vello_cpu` 0.18, both `default-features = false` (`Cargo.toml`). Screenshots use `anyrender_vello_cpu::VelloCpuImageRenderer` (`packages/core/src/paint/screenshot.rs`, `scene_replay.rs`). There is no `wgpu`, Vulkan, Metal, or GLES dependency in those manifests. Raster output is a CPU `vello_cpu::Pixmap` / RGBA buffer, then PNG or JPEG.
*   **Fonts are bundled, not system fonts.** `make_empty_font_context` sets `CollectionOptions { shared: true, system_fonts: false }` (`packages/core/src/resources/fonts.rs`). UA fonts live under `packages/engine/public/fonts/` (the directory is about 19 MB, 55 files, SIL OFL 1.1 per `NOTICE`). Production registration is `register_ua_fonts` from host-supplied bytes; tests `include_bytes!` Noto Sans, Italic, Serif, Mono, and Symbols. Noto Color Emoji (about 1.9 MB) and Noto Sans SC (about 5.9 MB) exist as separate assets. Blitz's `system_fonts` feature is deliberately off in `packages/core/Cargo.toml`.
*   `kitesurf-cli` is a terminal client. It downloads a Wasm scene rasterizer and runs it in **Wasmtime** 36 (`packages/kitesurf-cli/Cargo.toml`, `packages/kitesurf-cli/README.md`). That is a client playback path, not the Cloudflare host.
*   Boa is used to run page `Runtime.evaluate` expressions in the core Wasm (`packages/core/Cargo.toml`). Page `WebAssembly.*` is shimmed in `packages/page-script/src/globals.ts`. **s3a-kite does not use Boa or that shim to run an old S3A viewer.** Opening a `.s3a` file does not instantiate the page JavaScript runtime. Boa remains relevant only for ordinary `http(s)` pages the Scholar browser may open.

### 4.2 S3A tour and manifest

*   `s3a-cli tour [--json] [--floor N] [--slice] [--cross] [--history] <archive>` and `s3a-cli tour --html <archive> <out.html>` are implemented in `crates/s3a-cli/src/tour.rs`, `tour_html.rs`, and `tour_html_template.html`.
*   `tour_file` builds a `TourReport` with a `TourLobby`. If a floor has `type_id == TileType::MANIFEST` (`s3a-core`: `MANIFEST = 78`), the UTF-8 payload is `lobby.manifest`. Plain-text output prints `--- PLAIN-TEXT MANIFEST ---` then that payload (`print_tour_report`). The test `test_tour_displays_manifest_in_lobby_and_floor` expects `THIS IS AN S3A FILE` and the seal title/creator.
*   Default mode lists the floor directory. `--floor N` is floor detail (schema fields, records, `withheld_count`). Consent: records without active `purpose=research` are withheld (`tour.rs` header comment and `evaluate_consent_summary`).
*   `tour --html` embeds the same JSON, escaped so archive text cannot break out of a `<script type="application/json">` block, and renders with `textContent`, never `innerHTML` (`tour_html.rs`).
*   The HTML animation: clicking a tile opens `#card`. The card is first placed on the tile's bounding rect at opacity 0.2 with `transition: none`, then on a double `requestAnimationFrame` the transition is restored (unless the hash has `instant`) and the card moves to `left: 6vw; top: 8vh; width: 88vw; height: 84vh; opacity: 1`. CSS: `#card { transition: all .45s cubic-bezier(.2,.8,.2,1); }` (`tour_html_template.html`).
*   The unsealed fallback in `tour.rs` walks tiles at `FILE_HEADER_SIZE + i * HYPER_TILE_SIZE` (512 + *i* × 128 KiB). It does **not** consult `tile_stride`. Edge 4 KB / 8 KB archives are therefore not toured correctly by today's CLI scanner. s3a-kite must use `s3a_engine::MmapReader` (`is_edge_archive()`, `tile_stride()`, `header_extent()`, edge tile getters) instead of copying that loop.
*   A comment in `tour.rs` treats `type_id == 82` as `EdgeConsentRecord`. In `s3a-core`, 82 is `VCS_RESOLUTION`. Edge consent is `TILE_TYPE_CONSENT = 71` (`s3a-edge/src/header.rs`), with a different record layout than core consent. `s3a-engine/src/display_layouts.rs` warns that hyper-tile and edge archives reuse type numbers (64, 71, 75) and must be selected with `MmapReader::is_edge_archive`. s3a-kite must not copy the type-82 mistake.

### 4.3 Edge profile

From `docs/spec/S3A-EDGE-PROFILE.md` and `crates/s3a-edge`:

*   Phase 1 writer is `#![no_std]`, heapless, for MCUs with ≤ 32 KB SRAM. Micro-tiles are exactly 4,096 or 8,192 bytes. Dual-slot header Variant A is 512 bytes; tile *i* is at `512 + i * tile_stride`. Variant B (`FLAG_COMPACT_HEADER`) uses extent `P = max(256, tile_stride)`.
*   Flags: `FLAG_PROFILE_EDGE`, `FLAG_TILE_SIZE_4KB`, `FLAG_TILE_SIZE_8KB`, `FLAG_COMPACT_HEADER`, `FLAG_SEAL_CHAIN_ACTIVE`, `FLAG_ED25519_SIGNED`. Detection: `FLAG_PROFILE_EDGE` **or** `tile_stride` 4096 or 8192. Torn or unsealed micro-tiles are ignored, never treated as committed history.
*   Equipment event log is type 64, `EquipmentEventRecord` (32 bytes) with `event_type` 0x0001–0x0007 and `severity` 0=Info, 1=Warn, 2=Alarm, 3=Emergency (§5.2). Type 64 may also hold a 96-byte `DeviceIdentityRecord` (distinguish by `payload_len / record_count`). Every Edge archive MUST contain at least one `DeviceIdentityRecord` (§6.3).
*   On-device trust is a SHA-256 seal chain (`S3AEDGE1` domain separation) and optional Ed25519 signatures in type 70. There is **no** tile 78 manifest in the Edge tile list (§12.2).
*   Conformance vectors `MICRO-VEC-01` (`micro_valid_4kb`), `MICRO-VEC-02` (torn tile), `MICRO-VEC-03` (signed seal chain) are generated in `crates/s3a-spec-vectors`.
*   §10 Phone Reader (Android first): verify CRC32C, payload hash, seal chain, Ed25519; store raw `.s3a`; show an event timeline (faults in red, maintenance in yellow, normal operations in green); optional later LRS sync and in-app consent writes. Phase 2 transports are not implemented.
*   Primary invariant (§1.2): every Edge file remains a valid S3A file readable by `s3a-engine` without transcoding.

### 4.4 3-pass and LRS

From `/workspace/3Pass/README.md` (repo [ashleycribb/3Pass](https://github.com/ashleycribb/3Pass)):

*   Pass 1 is the researcher's own skim. The AI only checks whether a record exists.
*   A paper gets a researcher record only when the researcher selects it for Pass 2/3.
*   `triage_wave.py` lists candidates; it does not score, rank, include/exclude, or record anything. Skipped papers are never recorded. The heading is "Pass 1: Your Skim (~15 s per paper)".
*   AI-extracted text is an AI draft and needs researcher judgment.

`s3a-pack-scholar` already defines the Scholar xAPI profile and tiles 10–15. `s3a-lrs-server` is the LRS (`POST /xapi/s3a/batch`). A unit test documents the client contract: consent off emits zero requests (`crates/s3a-lrs-server/src/main.rs`, `test_batch_sync_consent_off_client_contract`). The s3a-lrs VM is part of Ashley's lab and is outside the storage repo (`docs/design/research-lab-scenario.md`).

### 4.5 S3A mobile app (`s3a-reader`)

[ashleycribb/s3a-reader](https://github.com/ashleycribb/s3a-reader) at `792742c`, with Ashley's 2026-10-27 decisions already recorded (UniFFI + cargo-ndk, Kotlin, Jetpack Compose, minSdk 26, Android first, later iPhone via UniFFI Swift). The Rust crate `s3a-reader-core` opens bytes, lists tiles, verifies CRC and signatures, and can leave a quarantined file viewable with a label. The Android shell has no network permission; files come from SAF. Visualization today is a Kotlin "deck plan" (`MapLayout.kt`), not Kitesurf.

s3a-kite **replaces that visualization** and becomes the in-app viewer. Verification APIs in `s3a-reader-core` stay. BLE/USB remain out of kite v1.

### 4.6 Pack contract

`docs/pack-contract.md`: browser and WebAssembly integrations stay out of `s3a-core` and `s3a-engine`. Addon bits already allocated: EDU, FINTECH (reserved), ROBOTICS, WEARABLE, GIS, DA, ONTOLOGY. Future packs that need new tile IDs start at 64+ with a new bit; the Clean pack uses existing core tiles and needs no new bit. v1 kite follows the Clean-pack pattern (DD-15, DD-16).

--------------------------------------------------------------------------------

## 5. Architecture

### 5.1 Pack layout (draft: DD-01)

A new workspace crate `crates/s3a-pack-kite` in `s3a-storage` (or a clearly named `s3a-kite` crate in that workspace). It vendors the forked Kitesurf Rust core (`packages/core`, `packages/renderer-core`, and the `kitesurf-cli` library as needed) at pin `4a2903813a22b08d4f3cc64a4b35339d0293a5d1`. The TypeScript engine, Wrangler config, and Worker Loader are not a runtime dependency.

`s3a-core` MUST NOT depend on this pack. The pack depends on `s3a-engine`, `s3a-edge`, `s3a-pack-scholar` (for 3-pass records and the xAPI registry), and the forked Kitesurf crates.

### 5.2 Native host (hub, Aura Rack, LRS VM, Cloud Run)

Replace Cloudflare isolates with a host process that:

1.  Compiles `kitesurf-core` and `renderer-core` with feature `native-host` and links them as native `rlib`s (DD-02).
2.  Requires an explicit `--port` and `--bind` (DD-03). Missing values are an error. There is no silent production default.
3.  Rejects unauthenticated CDP and HTTP (DD-11). Upstream Kitesurf is fail-open; s3a-kite is fail-closed.
4.  For `http(s)` pages: one sandboxed process (or equivalent isolate) per page, with egress policy. This is the replacement for `LOADER.load`.
5.  For `.s3a` and Edge files: do **not** start Boa or page-script. The host calls the pack's Rust reader and paints frames with `vello_cpu`.

### 5.3 Opening a hyper-tile `.s3a` file

1.  Open with `s3a_engine::MmapReader` (or an equivalent read-only engine API). If it is not an S3A file, fail with a clear error (the reader already uses "This is not an S3A file").
2.  **First view:** if tile 78 is present and UTF-8, paint that exact stored manifest text (the same bytes as `lobby.manifest`, trimmed only as `print_tour_report` already trims). If tile 78 is absent, paint a labeled "manifest absent" state. Do not invent manifest text.
3.  **Then** the warehouse tour: floors from the same data `tour_file` would produce for a hyper-tile archive (index, type, record count, size, hash). Zoom animation matches `tour_html_template.html` (0.45s cubic-bezier from the tile rect to the 6vw/8vh/88vw/84vh viewport), unless an explicit `instant` flag is set.
4.  Floor N detail matches `s3a-cli tour --floor N`, including `withheld_count`.
5.  Archive bytes are painted as text, never executed as HTML or JavaScript (same policy as `tour_html.rs` escaping).

### 5.4 Opening an Edge micro-tile file

1.  Detect with `MmapReader::is_edge_archive()` (flag or stride 4096/8192).
2.  Verify header CRC, per-tile `header_crc32` / `data_crc32` / `payload_hash`, and the seal chain. Failed signatures or broken chains produce a quarantine banner (DD-20). Torn tiles are omitted from the floor list and counted as ignored on the header sheet (`MICRO-VEC-02`).
3.  **First view:** a derived header sheet, clearly labeled as derived from the file header and `DeviceIdentityRecord`, not as a stored tile 78 manifest. Fields are only those present: magic, flags, `tile_stride`, `tile_count`, generation, timestamps, `file_uuid`, device id, hardware model, firmware version. Missing fields are omitted, not invented.
4.  **Warehouse tour scaled to micro-tiles:** each committed micro-tile is a floor. `size_bytes` is 4096 or 8192. Offsets follow Variant A or B as declared. Visual footprint of a floor is linear in stride, so a 4 KB bay, an 8 KB bay, and a 128 KB hyper-tile bay are distinguishable (DD-04).
5.  Type 64 with 32-byte records renders as an equipment event log: timestamp, `event_type` name from §5.2.2, severity. Color by severity (DD-07): 0 green, 1 yellow, 2 and 3 red. Type 64 with 96-byte records renders as `DeviceIdentityRecord` using `edge_display_layouts()`.
6.  Edge consent uses the Edge layout of type 71, not core `ConsentRecord` and not type 82.
7.  No transcoding into a 128 KB hyper-tile file is required in order to view (Edge §1.2). Import via `import_edge_archive` is a separate, existing engine operation, not the viewer path.

The microcontroller continues to run only `s3a-edge`. It does not link kitesurf or the host (AT-22, DD-06).

### 5.5 Scholar wiki browser (3-pass)

*   Render wiki Markdown (for example `index.md`, `papers/SRC-*.md`) from the corpus or the wiki tree that the host is configured to open. A missing page is reported as missing.
*   Pass 1 candidate list writes nothing (no HumanLrsRecord, no xAPI, no snapshot).
*   Skip / off-topic writes nothing.
*   Select or download, with tracing consent, emits the inclusion statement to the configured `s3a-lrs-server` and writes the Scholar inclusion record from the existing registry. Consent off: zero LRS requests and no inclusion record.
*   The pack cannot itself write include/exclude/score. Only the researcher's select or download creates the inclusion record.
*   A 15-second skim timer may be shown; expiry does not change records (DD-08).

### 5.6 Sealed snapshots

A snapshot of a selected source is a sealed write of the captured bytes into existing core BLOB + PROVENANCE tiles (DD-15), with content SHA-256, source URL if any, and capture time. Re-open must verify the hash. Snapshots of selected papers are research activities and require tracing consent (DD-09). Skip does not snapshot.

### 5.7 Mobile build (Android, then iOS)

s3a-kite is the viewer inside [s3a-reader](https://github.com/ashleycribb/s3a-reader).

**Why this is feasible without a mobile GPU.** Kitesurf already rasterizes on the CPU through `vello_cpu` / `VelloCpuImageRenderer` to an RGBA pixmap. The mobile library paints the manifest and warehouse into a CPU buffer and hands pixels to the UI toolkit. v1 does not add a wgpu or platform GPU backend (DD-21).

**Fonts.** Keep `system_fonts: false`. The app ships UA font bytes and registers them with the same `register_ua_fonts` / `populate_ua_fonts` path. Missing UA fonts is an error, not a fallback to device fonts (AT-42, DD-22). Draft subset: Noto Sans, Italic, Serif, Mono, and Symbols in v1 (the set already used in core font tests). Noto Color Emoji and Noto Sans SC stay optional on-demand assets so the first APK does not have to absorb the full 19 MB font directory.

**Android (v1 ship target, already decided for the reader).**

*   UniFFI `cdylib` + Kotlin, cargo-ndk, minSdk 26, ABIs `arm64-v8a` and `x86_64` (DD-23), Jetpack Compose.
*   Compose displays the Rust pixmap (for example `ImageBitmap` / `Bitmap` from RGBA). The warehouse tour does not require a WebView.
*   SAF / share-sheet open paths already in the app stay the way files arrive. The Kotlin deck plan is no longer the primary viewer (DD-24).
*   `s3a-reader-core` verification still runs first. Quarantined files may be opened with a persistent banner; failed-CRC and torn tiles are not drawn as valid floors (DD-20).
*   The app's current "no network" manifest remains the default. LRS sync on the phone is not a kite v1 requirement (Edge §10.4 stays later). Scholar tracing on the phone happens only if the user has enabled consent *and* an LRS endpoint is configured; otherwise select records locally or errors closed (DD-12).

**iOS (feasible, not the first ship).**

*   Feasible because the paint path is portable `std` Rust plus CPU raster, not Metal. UniFFI already planned for Swift in the reader repo.
*   Follow-on, required once the same crate links for `aarch64-apple-ios` and `aarch64-apple-ios-sim` (AT-40, DD-25). SwiftUI (draft) shows the same RGBA frames.
*   Same font and no-GPU rules.

**Native-link work the pack must do (not optional, not a stub).** Kitesurf's workspace today pins `getrandom` 0.2/0.3 with `js` / `wasm_js` features, and `fonts.rs` uses `#[wasm_bindgen]`. The `native-host` feature already gates some exports. The pack's fork must compile `kitesurf-core` for `aarch64-linux-android`, `x86_64-linux-android`, and later Apple iOS targets **without** the Wasm `getrandom` JS backend and without requiring a JS host. That is a fork change in the pack, with Apache change notices on touched files.

**ESP32 / MCU.** Not a kite viewer target.

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

### 6.2 MPL-2.0 (Stylo, and the Public Suffix List if that engine file is kept)

Stylo is MPL-2.0 (`README.md` Technology stack; `NOTICE` lists stylo among patched crates). The Public Suffix List snapshot is MPL-2.0 (`NOTICE`, `licenses/MPL-2.0-PSL.txt`).

MPL is file-level copyleft:

1.  Modified MPL-covered files (Stylo, Stylo patches under `scripts/patches/`, and the PSL if bundled) must remain MPL-2.0, and their source must be provided.
2.  The larger work (the pack, Apache-2.0) may link them.
3.  Do not remove MPL notices.

### 6.3 Other NOTICE items the pack must retain if the corresponding assets ship

*   SIL OFL 1.1 for Noto, Inter, and related fonts (`packages/engine/public/fonts/OFL.txt`, `licenses/OFL-1.1-*`).
*   jpeg-encoder: MIT OR Apache-2.0, **and** IJG terms (`licenses/IJG-jpeg-encoder-0.7.0.txt`); "This software is based in part on the work of the Independent JPEG Group."
*   WPT / Webref / TodoMVC / wasm-memprof / anti-slop, if those files remain in the fork.

A release artifact that omits NOTICE, LICENSE, Stylo/PSL source availability, or OFL texts fails AT-02.

--------------------------------------------------------------------------------

## 7. Acceptance tests

These tests are written first. Each one is a concrete check that **fails on today's code** (no `s3a-pack-kite` / UniFFI kite renderer, `s3a-cli tour` mis-walks Edge strides, Android map is Kotlin). Implementation must make them pass **without editing the tests** except as listed later with reasons.

Fixtures: hyper-tile archives built the same way as `crates/s3a-cli/tests/tour_tests.rs` (`write_manifest: true` for AT-08); Edge vectors `micro_valid_4kb`, torn and signed vectors from `s3a-spec-vectors`; an 8 KB file produced with `EdgeWriter<_, 8192>`.

### Pack and license

*   **AT-01.** `cargo test -p s3a-pack-kite` (name as built, DD-01) exists and the crate's Cargo manifest does not cause `s3a-core` to depend on kite or kitesurf (`cargo tree -p s3a-core` has no such edge).
*   **AT-02.** The pack's distribution directory contains Apache-2.0 `LICENSE`, Kitesurf `NOTICE` (including the Cloudflare 2026 copyright line and the Stylo / PSL MPL notes), OFL text for every shipped font, and IJG text if jpeg-encoder is linked. A CI check lists these files; a missing NOTICE fails.
*   **AT-03.** Every forked Kitesurf source file the pack modifies starts with or includes a prominent "changed from upstream kitesurf `4a2903813a22b08d4f3cc64a4b35339d0293a5d1`" notice in addition to the original copyright (Apache §4).

### Host

*   **AT-04.** `s3a-kite host --bind 127.0.0.1 --port <ephemeral> --auth-token-file <token>` starts without Wrangler and without a Worker Loader, and a configured health URL returns success. Today there is no such binary.
*   **AT-05.** That binary does not read `packages/engine/wrangler.jsonc` and does not call a `LOADER.load` API.
*   **AT-06.** The same host without a valid token rejects HTTP and CDP (non-2xx / no session). An open listener is a failure.
*   **AT-07.** Omitting `--port` or `--bind` in a non-documented production mode exits non-zero with an error that names the missing flag. No implicit port is bound.

### Hyper-tile `.s3a`

*   **AT-08.** Open a sealed archive produced like `test_tour_displays_manifest_in_lobby_and_floor`. The first painted frame's text contains the exact stored manifest payload (including `THIS IS AN S3A FILE` and the seal title/creator). The warehouse floors are not the first frame.
*   **AT-09.** Open a hyper-tile archive with no tile 78. The first frame is a labeled "manifest absent" state and does not contain invented title/creator text. The tour may follow.
*   **AT-10.** After the manifest view, the tour floor list matches `s3a-cli tour --json` on the same hyper-tile file: `index`, `type_id`, `record_count`, `size_bytes`, `payload_hash_short`.
*   **AT-11.** Opening floor *N* shows the same records and the same `withheld_count` as `s3a-cli tour --floor N`.
*   **AT-12.** A manifest or record that contains `<script>`, `</script>`, or HTML angle brackets is painted as those characters. No script from the archive runs.
*   **AT-13.** Without `instant`, the floor-to-detail transition uses the 0.45s cubic-bezier expand from the tile rectangle to the 6vw/8vh/88vw/84vh viewport. With `instant`, the same detail view appears with no transition.

### Edge micro-tiles

*   **AT-14.** Open `micro_valid_4kb.s3a` (MICRO-VEC-01). The reader reports `is_edge_archive` and `tile_stride == 4096`. The first frame is the derived header sheet, labeled as derived, and includes stride 4096 and the committed `tile_count`. It is not presented as tile 78.
*   **AT-15.** The warehouse list for MICRO-VEC-01 has exactly the committed micro-tiles. Each floor `size_bytes == 4096`. Offsets follow `512 + i * 4096` for Variant A.
*   **AT-16.** An 8 KB `EdgeWriter` fixture shows `tile_stride == 8192` and `size_bytes == 8192`. In a frame that also includes a 4 KB bay (or a comparison fixture), painted bay area is linear in stride (8 KB bay is twice a 4 KB bay, a 128 KB bay is 32 times a 4 KB bay), within a one-pixel rounding tolerance.
*   **AT-17.** MICRO-VEC-02: only the valid tile(s) appear as floors; the torn tile is omitted and the header sheet reports it as ignored/torn.
*   **AT-18.** MICRO-VEC-03: seal verification matches `s3a-engine`. A copy with a reordered or mutated tile fails verification, shows quarantine, and does not list the mutated tile as a valid floor.
*   **AT-19.** A type-64 tile of 32-byte `EquipmentEventRecord`s paints timestamp, the §5.2.2 event name, and severity. Severity 0 is green, 1 yellow, 2 and 3 red (pixel samples at the severity badge).
*   **AT-20.** A type-64 tile whose record size is 96 bytes paints `DeviceIdentityRecord` fields (device id, hardware model), not event columns.
*   **AT-21.** A compact-header Edge file (`FLAG_COMPACT_HEADER`) places tile 0 at `P = max(256, tile_stride)`, not at 512 if *P* is not 512.
*   **AT-22.** `crates/s3a-edge/Cargo.toml` does not depend on kite or kitesurf. A `no_std` build of `s3a-edge` still succeeds.
*   **AT-23.** When an Edge type-71 `EdgeConsentRecord` does not allow the research purpose, research-purpose views withhold records and report `withheld_count`. The implementation must use the Edge layout, not type 82 and not core `ConsentRecord`.

### Scholar 3-pass and snapshots

*   **AT-24.** Building the Pass 1 candidate list twice leaves the corpus archive and the LRS store byte-identical to the start (no HumanLrsRecord, no xAPI statements, no new snapshot tiles).
*   **AT-25.** Dismissing a paper as off-topic / skip writes nothing (same byte-identity check).
*   **AT-26.** Selecting or downloading a paper, with tracing consent on and a configured LRS, writes one inclusion statement whose verb IRI is taken from `s3a-pack-scholar` `xapi.rs` (not a newly invented IRI) and a Scholar inclusion record for that paper id.
*   **AT-27.** The same select with consent off: zero HTTP requests to the LRS and no inclusion record.
*   **AT-28.** An API call that tries to write include/exclude/score without a human select or download is rejected and writes nothing.
*   **AT-29.** The Pass 1 timer reaching 15 seconds does not create or delete any record.
*   **AT-30.** Opening a configured wiki `index.md` paints its heading text from the file bytes. Opening a missing `papers/SRC-9999.md` reports missing and does not invent a dossier.
*   **AT-31.** After a consented inclusion, a sealed snapshot exists whose payload hash equals SHA-256 of the captured bytes; re-open verifies. Skip does not create a snapshot.
*   **AT-32.** That snapshot record carries content SHA-256 and, if the source had a URL, that URL. No other metadata fields are invented.

### Web pages vs archives

*   **AT-33.** Opening `https://example.com/` uses the HTML/CSS path in a sandboxed page process, not the `.s3a` tour path.
*   **AT-34.** Opening a `.s3a` file does not create a Boa context or a page-script isolate for the tour. The tour is painted by the native `vello_cpu` renderer.

### Safety and scope

*   **AT-35.** A hyper-tile or Edge file with a failed tile CRC or a failed Ed25519 check is labeled quarantined. The failing tile is not shown as a valid floor.
*   **AT-36.** The pack crate's source and tests contain no Bitcoin, OpenTimestamps-write, payment, or token-transfer code paths. Reading Edge `seal_hash` values is allowed.
*   **AT-37.** No new `TileType` constant is added under `s3a-core`.

### Mobile library

*   **AT-38.** The UniFFI library builds for `aarch64-linux-android` and `x86_64-linux-android` with minSdk 26. `open` + `render_manifest` + `render_warehouse_frame` (names as implemented) return an RGBA or PNG buffer. For the AT-08 fixture, OCR or a packed-in text layer test finds `THIS IS AN S3A FILE` in the first frame; for MICRO-VEC-01, the first frame's text layer includes `4096`.
*   **AT-39.** The Android UI path that shows the tour does not instantiate `android.webkit.WebView` for that screen. A lint or unit test on the Compose screen's view tree fails if a WebView is the tour surface.
*   **AT-40.** `cargo check --target aarch64-apple-ios -p s3a-pack-kite` (or the UniFFI crate) succeeds. Shipping the iOS app remains a follow-on (DD-25); this test is the feasibility gate.
*   **AT-41.** `cargo tree` for the kite render crate, for Android and host targets, does not include `wgpu` or a Vulkan/Metal/GLES backend used by the tour path. The linked paint crate is `vello_cpu` / `anyrender_vello_cpu`.
*   **AT-42.** With UA fonts removed from the test fixture, `render_manifest` returns an error that names missing fonts. It does not silently render with system fonts (`system_fonts` stays false).
*   **AT-43.** In the Android app, opening the AT-08 fixture shows the kite manifest-then-warehouse path as the primary view, not only `MapLayout` as the main surface (DD-24).
*   **AT-44.** `getrandom` for Android and iOS targets is not enabled with the `js` / `wasm_js` features. A build without a JavaScript runtime succeeds (the fork change required in §5.7).

--------------------------------------------------------------------------------

## 8. Draft decisions

Ashley may accept a draft, change it, or leave it open. Draft values follow.

| ID | Topic | Draft value |
| --- | --- | --- |
| **DD-01** | Pack location | `crates/s3a-pack-kite` in the s3a-storage workspace, with the Kitesurf Rust core vendored in the pack at commit `4a2903813a22b08d4f3cc64a4b35339d0293a5d1`. |
| **DD-02** | Host compile mode | Link `kitesurf-core` and `renderer-core` with `native-host` as native rlibs. Wasmtime is not required for the `.s3a` tour. |
| **DD-03** | Bind address and port | Required flags `--bind` and `--port`. No silent production default. Docs may show `127.0.0.1:8787` as an example because that is upstream Kitesurf's local port. |
| **DD-04** | Visual metaphor | Call the UI a warehouse. Data model remains lobby + floors. Bays map 1:1 to floors. Bay area is linear in tile stride. |
| **DD-05** | Device-side stored manifest | Not written on the MCU in v1 (RAM and flash budget; no tile 78 in the Edge profile). The hub/PC/phone shows the derived header sheet. If firmware later stores UTF-8 manifest text, it must use a declared tile type, and the viewer must prefer those stored bytes. |
| **DD-06** | Slimmed renderer / MCU | v1 uses the same `native-host` + `vello_cpu` renderer on hub, PC, and phone. No second renderer until a measured memory budget on a named device fails. Never on the microcontroller. |
| **DD-07** | Event colors | Color by severity: 0 green, 1 yellow, 2 and 3 red. Print `event_type` as the §5.2.2 name. |
| **DD-08** | Pass 1 timer | Show a 15-second skim timer. Expiry does nothing to records. |
| **DD-09** | Snapshot consent | Snapshots of selected papers require tracing consent. Skip does not snapshot. |
| **DD-10** | Device transport | v1 opens files already on the hub, PC, or phone. BLE/USB/SoftAP stay Edge Phase 2. |
| **DD-11** | Host authentication | Fail closed. `--auth-token-file` points at a token that is not in the repository. Unauthenticated requests are rejected. |
| **DD-12** | LRS URL | Must be configured. No invented localhost default in production. If consent is on and the URL is missing, select fails closed with an error; it does not silently skip the statement. |
| **DD-13** | Markdown | The pack parses CommonMark with `pulldown-cmark` to HTML, then `kitesurf-core` / Blitz lays out that HTML. |
| **DD-14** | PDF pages | v1 does not rasterize PDF input. Skim shows title, abstract, and any markdown conversion already in the archive. If that conversion is missing, show "no text conversion in archive", not a generated summary. |
| **DD-15** | New tile IDs | None in v1. Snapshots use existing BLOB + PROVENANCE. Inclusion uses Scholar tiles when `ADDON_EDU` is set. |
| **DD-16** | Addon mask | No new addon bit in v1. Scholar writes require `ADDON_EDU`. Edge files already carry `ADDON_WEARABLE`. |
| **DD-17** | Upstream pin | Kitesurf `4a2903813a22b08d4f3cc64a4b35339d0293a5d1`. |
| **DD-18** | Animation timing | Match `tour_html_template.html`: 0.45s `cubic-bezier(.2,.8,.2,1)` from the tile rect to 6vw/8vh/88vw/84vh, skipped when `instant` is set. |
| **DD-19** | App relationship | s3a-kite is the viewer inside `s3a-reader`. Verification stays in `s3a-reader-core`. Kite does not replace the edge writer or the MCU firmware. |
| **DD-20** | Quarantine UX | Follow the existing reader: a quarantined file can be opened with a persistent banner. Failed-CRC, torn, and chain-broken tiles are not valid floors. |
| **DD-21** | GPU | v1 stays on `vello_cpu`. No wgpu, Vulkan, Metal, or GLES backend for the tour. |
| **DD-22** | Mobile fonts | Ship Noto Sans, Italic, Serif, Mono, and Symbols with the app and register them as UA fonts. `system_fonts` remains false. Color Emoji and CJK SC are optional on-demand assets, not required to open a file. |
| **DD-23** | Android ABIs and UI | minSdk 26, `arm64-v8a` and `x86_64`, UniFFI + Kotlin, Jetpack Compose displays the Rust pixmap. No 32-bit ARM in v1. |
| **DD-24** | Kotlin deck plan | The kite warehouse is the primary view. `MapLayout` may remain as a secondary diagnostic screen, not the default. |
| **DD-25** | iOS | Feasible (CPU raster + UniFFI Swift). Ship Android first. iOS follows when AT-40 stays green and a SwiftUI surface shows the same frames. |

--------------------------------------------------------------------------------

## 9. Open questions

*   **Q-01.** Lab host authentication: token file (DD-11) or an existing lab SSO? Draft is the token file until Ashley names an SSO.
*   **Q-02.** Exact LRS base URL on the s3a-lrs VM, and the consent-copy shown before tracing.
*   **Q-03.** Whether phone builds should ever enable network for LRS, or keep the current no-network reader and leave Scholar tracing to the hub/PC host.
*   **Q-04.** Preferred CommonMark library if not `pulldown-cmark` (DD-13).
*   **Q-05.** Whether the optional CJK and emoji font packs should ship in the first Play/App build or only as downloads.

Lawyer review is not required for the fork itself (Apache-2.0 + MPL file-level terms are listed in §6), but distribution of the app's font files must keep OFL notices (AT-02).

--------------------------------------------------------------------------------

## 10. Risks

1.  **`native-host` is a thin feature flag.** Much of Kitesurf's DOM and JS is `#[cfg(target_arch = "wasm32")]`. The tour path should use paint/layout entry points that already have native-host or non-Wasm coverage (`scene_replay`, `VelloCpuImageRenderer`). Additional native entry points will be needed in the fork. This is the main engineering risk, not "can Boa run the old viewer."
2.  **Wasm-oriented dependencies.** Workspace `getrandom` uses `js` / `wasm_js`. `fonts.rs` uses `wasm_bindgen`. Android and iOS builds fail until the pack forks those bits (AT-44).
3.  **Today's `s3a-cli tour` is not a drop-in Edge tour.** It steps 128 KB tiles and has a type-82 consent comment that does not match the Edge registry. Kite must use `MmapReader` edge APIs and `edge_display_layouts()`.
4.  **Type-number reuse** (64, 71, 75) between hyper-tile and Edge layouts. Branching on `is_edge_archive` is mandatory.
5.  **Phone memory and binary size.** Full Stylo + Blitz + UA fonts is heavier than the current Kotlin map. DD-06 and DD-22 exist because this may force a later slim. Measure on a named minSdk-26 device before adding a second renderer.
6.  **No PDF input renderer** in the Kitesurf tree (Krilla is output). Pass 1 can look incomplete for PDF-only papers (DD-14).
7.  **Kitesurf is experimental** (README; last grounded commit 2026-11-13). Upstream will move. The pin is explicit (DD-17).
8.  **Host auth.** Shipping a fail-open CDP port would repeat upstream's warning. AT-06 is a gate.
9.  **License completeness.** Forgetting NOTICE, MPL source, or OFL/IJG texts is a release blocker (AT-02).

--------------------------------------------------------------------------------

## 11. Implementation notes (after tests exist and fail)

Order of work, when Ashley authorizes code:

1.  Land the acceptance tests in the pack so they fail.
2.  Vendor the Kitesurf Rust core, add LICENSE/NOTICE, enable `native-host`, fix getrandom/wasm_bindgen for native and Android targets.
3.  Implement hyper-tile manifest + warehouse paint against `tour_file` data for hyper-tiles, and `MmapReader` for Edge.
4.  Host binary with required bind, port, and token.
5.  Scholar 3-pass gates and snapshots.
6.  UniFFI + Android Compose surface in `s3a-reader`; then the iOS link gate.

Do not "temporarily" call the old WebAssembly viewer, invent a manifest, walk Edge files with `HYPER_TILE_SIZE`, or treat type 82 as Edge consent.

--------------------------------------------------------------------------------

## 12. Document history

*   2026-11-16: v1. Tests-first pack spec from Ashley's 2026-11-16 direction, grounded in Kitesurf `4a29038`, s3a-storage `b7bdcda`, and s3a-reader `792742c`.
