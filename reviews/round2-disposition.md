# s3a-kite goldfish round 2 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round2.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round2.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v3 (self-contained copy at `/workspace/goldfish/s3a-kite.md`).
*   **Totals:** 42 findings (5 Blocker, 17 Major, 20 Minor). 41 fixed in the spec (several by narrowing v1 scope via a new DD, which is simpler than inventing a missing engine); 0 rejected; 1 deferred (F-24, exact severity RGB left to Q-10; AT-19 still asserts pairwise-different colors and labels).
*   **Grounding:** unchanged from v2 — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`).
*   **v3 counts:** AT-01–AT-99 (existing 1–74 reworded in place, no gaps; AT-75–AT-99 new). DD-01–DD-57 (DD-01–DD-40 keep their topics; meaning changes noted below). Q-01–Q-11 (Q-09 retry backoff, Q-10 severity RGB, Q-11 product name).
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | No host API or UI surface | Fixed. Native window in the host process; Host HTTP API is the complete v1 wire contract. | §5.2, §5.2.1 | DD-42; AT-06, AT-75 |
| F-02 | Blocker | CDP server rebuild unscoped | Fixed by taking CDP and agent tokens **out of v1** (narrower than specifying six TS domains in Rust). | §3.2; §5.2 item 3 | DD-41; AT-06, AT-57 |
| F-03 | Blocker | Wasmtime host ABI for wasm32 core undefined | Fixed by taking page JS / Wasmtime **out of v1**. HTML is host-fetched and laid out with JS disabled. | §3.2; §5.2 items 1, 4, 5 | DD-26 (amended), DD-44; AT-33, AT-34 |
| F-04 | Blocker | Render API cannot express viewport, floor, scroll, taps, Continue | Fixed. FFI list in §5.1 includes `render_manifest` / `render_warehouse` / `render_floor_transition` / `render_floor_detail` / `proceed_to_warehouse` / `hit_test`, with explicit size, scroll, floor, `instant`. | §5.1, §5.8 | DD-37 (amended); AT-09, AT-13, AT-62, AT-93, AT-96 |
| F-05 | Blocker | Working-archive vs snapshot path rules contradict | Fixed. Canonicalize the working-archive **parent** (file may be missing at start). `--snapshot-dir` must exist and is an allowed open root. | §5.2.2, §5.6 | DD-27, DD-30; AT-31, AT-78, AT-80 |
| F-06 | Major | Phone Scholar writes (DD-39) cannot be built | Fixed by taking phone Scholar writes **out of v1**. Phone is a viewer only. | §3.2; §5.1, §5.7 | DD-39 (amended), DD-45; AT-74, AT-99; Q-03 |
| F-07 | Major | "Human select" is ambiguous | Fixed. A write is a call authenticated with the UI token; custody of the token file is the control. No extra gesture bit. | §1; §5.5 | DD-32 (amended); AT-28 |
| F-08 | Major | Agent sessions can toggle consent and observe Pass 1 | Fixed by DD-41: no agent token and no CDP in v1. Consent toggle is `POST /v1/consent` with the UI token only. | §5.2.1, §5.5 | DD-41, DD-50; AT-57, AT-58 |
| F-09 | Major | Consent state semantics underspecified | Fixed. File required when `--lrs-url` is set; missing/malformed is a startup error; host is the only writer; mid-session outside edits are not honored. | §5.2.3 | DD-50; AT-58, AT-84 |
| F-10 | Major | Retry-queue persistence unspecified | Fixed. On-disk `--data-dir/lrs-retry/`, survives restart, deleted on revoke. Numeric backoff / max attempts are Q-09 (not invented). | §5.5 | DD-51; AT-59, AT-91; Q-09 |
| F-11 | Major | Paper identity, candidates, inclusion layout undefined | Fixed. Identity is `SRC-NNNN` stem, or decimal `paper_id`, or URL. Candidates are `papers/SRC-*.md` plus `AcademicPaperRecord`s. `object_hash` / `actor_hash` are CRC32C. Q-07 draft: one existing verb for select and download. | §2, §5.5 | DD-46; AT-26, AT-60, AT-83; Q-07 |
| F-12 | Major | Snapshot encoding undefined for real (incl. >128 KiB) payloads | Fixed. Registry / `chunk_tiles` 40-byte prefix; `blob_key` = SHA-256 of full bytes; max chunk `HYPER_TILE_PAYLOAD_SIZE - 40`; AT-82 uses ≥200 KiB. | §4.7, §5.6 | DD-47; AT-31, AT-32, AT-82 |
| F-13 | Major | Log/data directories undefined; caches vs byte-identity | Fixed. `--log-dir` and (Scholar) `--data-dir` are required and must exist. Render caches live in an ephemeral dir deleted on exit, not under those roots. | §5.2.2, §5.5 | DD-55; AT-24, AT-25, AT-61, AT-77 |
| F-14 | Major | No type-71 behavior and identity vs consent | Fixed. No type-71 means unrestricted, matching `tour.rs`. Header sheet and `DeviceIdentityRecord` are exempt from research filtering; event-log payloads are not. | §5.4 items 3, 6 | DD-53; AT-23, AT-85, AT-86 |
| F-15 | Major | Researcher's own working archive may be unviewable | Fixed. On `create`, kite writes an active research-purpose `ConsentRecord`. Snapshots in `--snapshot-dir` are exempt when opened as kite-written snapshots. | §5.6 | DD-53; AT-79 |
| F-16 | Major | Build-order step 5 vs 6; reader pin; "land" | Fixed. HTTP snapshots stay in the write-path step; there is no Wasmtime step in v1. Reader tests are feature-gated until the pin moves off `ff89564`. "Land" means local branches. | §5.1, §11 | DD-44 |
| F-17 | Major | Cloud Run vs single-researcher / durable disk | Fixed by taking Cloud Run **out of v1**. One process, one `--researcher-id`, durable local disk. | §1, §3.2, §5.2 | DD-48 |
| F-18 | Major | Sandbox and egress not concrete enough | Fixed for v1 host fetches (no page process). Deny at resolved IP and every redirect hop, including mapped/`0.0.0.0`/CGNAT. Timeout is required `--fetch-timeout-secs` (no invented seconds). | §5.2.4 | DD-31 (amended), DD-49; AT-54, AT-55, AT-88 |
| F-19 | Major | `--behind-proxy` breaks the TLS guarantee | Fixed. `--behind-proxy` is accepted only with a loopback or Unix-socket bind. | §5.2.2 | DD-52; AT-87 |
| F-20 | Major | Viewer cannot open an arbitrary file | Fixed. Viewer-only mode: `--viewer` + `--view-root`. Scholar write routes return 409. | §2, §5.2.2 | DD-30; AT-76 |
| F-21 | Major | Partial-failure and idempotency underspecified | Fixed. Step 1 failure skips 2 and 3; idempotency applies only to successful artifacts; download-dir collisions do not overwrite; a delivered file is kept if the record write fails, with an error shown. | §5.5, §5.6 | DD-34, DD-56; AT-89, AT-94 |
| F-22 | Major | Hyper-tile seal failure and Ed25519 range incomplete | Fixed. Seal-trailer failure: persistent banner; CRC-valid tiles may list, not as a verified seal. Failed Edge Ed25519 invalidates the signed chain from that point. | §5.3 item 6, §5.4 item 2 | DD-57; AT-35, AT-90 |
| F-23 | Minor | No device-pixel-ratio or scale parameter | Fixed. `device_pixel_ratio` is accepted and defaults to 1.0; v1 tests use 1.0. | §5.8 | DD-37; AT-16, AT-92 |
| F-24 | Minor | Color assertions have no values | Deferred to Q-10. AT-19 asserts three pairwise-different badge colors plus labels; exact RGB is not invented. | §5.4, §8 | DD-07; AT-19; Q-10 |
| F-25 | Minor | Weak text assertions | Fixed. Text checks use contiguous substrings of concatenated `text_runs`, not pixmap OCR. | §7 preamble; AT-08, AT-30, AT-38 | DD-38 |
| F-26 | Minor | Negative tests can pass vacuously | Fixed. Named negative write tests must first run a successful control select on a valid `ADDON_EDU` archive. | §7 preamble | AT-28, AT-46, AT-57, AT-73, AT-76, AT-89, AT-94 |
| F-27 | Minor | "No script runs" is unobservable | Fixed. AT-12 and AT-56 also require that no JS context is created. AT-33 checks a scripted `document.title` does not appear. | AT-12, AT-33, AT-56 | DD-44 |
| F-28 | Minor | `instant` semantics in a pull API | Fixed. `instant=true` returns only the end frame and ignores `t_seconds`. | §5.8 | AT-13, AT-93 |
| F-29 | Minor | Render signature has no error type | Fixed. Every `render_*` returns `Result<RenderFrame, KiteError>`. | §5.1 | DD-43; AT-42, AT-95 |
| F-30 | Minor | AT-37 "new" has no baseline | Fixed. Baseline is the `s3a-core` `TileType` set at `b7bdcda`. | AT-37 | — |
| F-31 | Minor | Other strides treated how? | Fixed. A stride that is not 4096, 8192, or 128 KiB is unsupported / inconsistent, not a hyper-tile. | Glossary, §5.3 item 1 | DD-40; AT-97 |
| F-32 | Minor | mmap fault: no approach or platforms | Fixed. Portable path is `from_bytes`. Path mmap is optional Unix; a fault is `KiteError`, not an abort. | §5.3 item 7 | DD-43; AT-98 |
| F-33 | Minor | Token-file edge cases | Fixed. Comparison is constant-time on raw file bytes, no trim, at least 32 bytes, used in full. | §5.2.2 | DD-29; AT-45, AT-81 |
| F-34 | Minor | Health "success" undefined | Fixed. HTTP 200, `Content-Type: text/plain`, body exactly `ok\n`. | §5.2.1 | AT-04, AT-50, AT-75 |
| F-35 | Minor | AT-33 depends on the live Internet | Fixed. AT-33 uses a local HTML fixture under `--view-root`. Loopback egress stays denied unless allowlisted. | AT-33 | DD-44 |
| F-36 | Minor | AT-74 phone half vs location | Fixed. AT-74 is host-only. Phone UniFFI has no select/download (AT-99). | §5.7 | DD-45; AT-74, AT-99 |
| F-37 | Minor | "No other metadata" vs PROVENANCE layout | Fixed. The 40-byte chunk prefix is the tile layout. The provenance *document* after it is the three-line UTF-8 text. `seal_archive` is used; a seal manifest is not counted as provenance metadata. | §5.6 | DD-47; AT-32 |
| F-38 | Minor | Product name vs Apache trademark clause | Fixed as a draft: keep the name `s3a-kite`; marks are used only to describe origin and reproduce NOTICE. Rename is Ashley's (Q-11). | §6.1, §8 | DD-54; Q-11 |
| F-39 | Minor | "Wasmtime 36" pins a major version | Fixed / moot in v1: Wasmtime is not a dependency (DD-44). The cli fact in §4.1 is unchanged. | §4.1, §4.7, DD-44 | DD-02, DD-44 |
| F-40 | Minor | AT-04 required-flag list incomplete | Fixed. AT-04 and AT-77 name every Scholar required flag in §5.2.2. | §5.2.2 | AT-04, AT-77 |
| F-41 | Minor | Download has no destination | Fixed. Required `--download-dir` must exist; collisions fail and do not overwrite. | §5.5, §5.6 | DD-56; AT-48, AT-94 |
| F-42 | Minor | AT-16 tolerance is loose | Fixed. AT-16 / AT-92 assert integer bay sides **32** and **45** CSS px at scale 1.0. | §5.8 | DD-37; AT-16, AT-92 |

## Blockers, in short

*   **F-01.** The human UI is a native window in `s3a-kite` that calls the pack API directly (DD-42). The Host HTTP API in §5.2.1 is the complete v1 HTTP surface (health, open, proceed, frame, hit-test, candidates, select, download, skip, consent). AT-06 is "every documented route other than loopback `GET /health` rejects a missing or invalid token." There is no CDP upgrade.
*   **F-02.** Reimplementing TypeScript CDP (`packages/engine/src/cdp/`) in v1 would have been a new server. DD-41 takes CDP and agent tokens out of v1. A later phase may add them under a new DD.
*   **F-03.** `boa_eval.rs` / `js.rs` are wasm32-only, and defining a Wasmtime import ABI (including `getrandom` / `wasm_bindgen` JS hosts) would have been a new engine. DD-26 is amended and DD-44 takes page JS and Wasmtime out of v1. AT-33 uses a local HTML fixture and asserts JS does not run and Wasmtime is not started.
*   **F-04.** The pack API is explicit: viewport (`width`, `height`, `device_pixel_ratio`), `scroll_x` / `scroll_y`, `floor_index`, `instant`, `proceed_to_warehouse`, and `hit_test` (`Continue`, `Floor(n)`, `Select`, `None`). `instant=true` returns only the end frame and ignores `t`.
*   **F-05.** At startup the host canonicalizes the **parent directory** of `--working-archive` (the parent must exist; the file may be missing so `TileWriter::create` can run). `--snapshot-dir` must exist as a directory and is in the allowed-path set, so AT-31's re-open is allowed. Scholar mode requires the working-archive flag; viewer-only mode has no Scholar write flags.

## Meaning changes to existing draft decisions

*   **DD-02:** Wasmtime is not used in v1 at all (was: not required for the tour only).
*   **DD-07:** exact RGB is no longer asserted; colors must be pairwise different; values are Q-10.
*   **DD-26:** v1 does not run page JavaScript (was: Wasmtime + wasm32 Boa).
*   **DD-30:** CDP scope and agent tokens removed; snapshot/download/view roots and working-archive parent canonicalization added.
*   **DD-31:** no page subprocess; egress applies to the host's own fetches, at connect time and every redirect hop.
*   **DD-32:** no agent token in v1; UI-token custody is the human-control point.
*   **DD-37:** `render_warehouse_frame` is retired in favor of the §5.1 functions; bay sides are 32 and 45 CSS px at scale 1.0.
*   **DD-39:** phone Scholar writes are out of v1 (was: local writes, no LRS).

## New draft decisions and questions

*   **DD-41–DD-57:** no CDP/agent tokens; native UI + Host HTTP API; `KiteError`; page JS off; phone viewer-only; paper identity; snapshot chunking; one researcher / durable disk / no Cloud Run; required `--fetch-timeout-secs`; consent-file writer; LRS retry queue; `--behind-proxy` bind rule; consent filtering / owner `ConsentRecord`; keep the `s3a-kite` name; required dirs and ephemeral temp; download-dir collisions; Edge signature / hyper-tile seal-failure rules.
*   **Q-09.** LRS retry backoff and maximum attempt count (no numbers invented).
*   **Q-10.** Exact RGB / palette for the three severity colors.
*   **Q-11.** Whether to rename `s3a-kite` (draft: keep the name).
*   **Q-07** gains a v3 draft: one existing `xapi.rs` verb for both select and download (AT-83).

