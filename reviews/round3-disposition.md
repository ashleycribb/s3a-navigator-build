# s3a-kite goldfish round 3 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round3.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round3.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v4 (self-contained copy at `/workspace/goldfish/s3a-kite.md`). v3 is archived as `reviews/SPEC-v3.md`.
*   **Totals:** 36 findings (2 Blocker, 13 Major, 21 Minor). 36 fixed in the spec; 0 rejected; 0 deferred. (Q-10 from round 2, exact severity RGB, remains an open question and was not a round-3 finding.)
*   **Grounding:** unchanged — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`).
*   **v4 counts:** AT-01–AT-125 (1–99 reworded in place, no gaps; AT-100–AT-125 new). DD-01–DD-78 (DD-01–DD-57 keep their topics; meaning changes noted below). Q-01–Q-13 (Q-12 source_url key name, Q-13 actor homepage flag vs fixed IRI).
*   Every "as in v2" / "unchanged from v2" rule was inlined. The goldfish copy is self-contained.
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | No page render function or `view=page` | Fixed. Added `render_page` and `view=page`. Select/Link hits defined; links are not auto-fetched. | §5.1, §5.2.1, §5.5 | DD-58; AT-30, AT-33, AT-56, AT-100, AT-101 |
| F-02 | Blocker | No fetch trigger, URL mapping, or fetch-failure rule | Fixed. Optional YAML `source_url`; fetch only on select/download of a current candidate; order and failure specified; on-disk papers are not snapshotted. | §5.2 item 4, §5.5, §5.6 | DD-59; AT-31, AT-54, AT-55, AT-82, AT-88, AT-102–AT-104 |
| F-03 | Major | Normative rules left as "as in v2" | Fixed. Tile-78 rules, header-sheet fields, category labels, `RenderFrame` / `text_runs` / `semantics` schema, animation endpoints, LRS fail-closed / unreachable state, fonts, and getrandom work are inlined. | §5.3–§5.8 | DD-07, DD-18, DD-38 |
| F-04 | Major | Native window bypasses token custody; headless hosts | Fixed. The window is a Host HTTP API client that sends the UI token. `--headless` is required for non-loopback/non-Unix binds. Pack FFI still has no write functions. | §1, §5.2 | DD-32 (amended), DD-60; AT-125 |
| F-05 | Major | 32-bit CRC collisions, dual identities, corpus walk | Fixed. Idempotency uses `--data-dir/inclusion-index.jsonl` (full identity string). SRC stem and decimal `paper_id` are distinct. Duplicate SRC paths: wiki-root wins. Recursive `*.s3a` walk of `--corpus-root`. `actor_hash` is `hash_iri` (`u64`). | §5.5, §5.6 | DD-46 (amended), DD-61 |
| F-06 | Major | `paper_id` need not be a candidate; no error codes | Fixed. Must be in the current candidate list; otherwise `404` `unknown_paper` and no write. | §5.2.1, §5.5 | AT-105 |
| F-07 | Major | Download filename undefined; re-download always collides | Fixed. Name is `{sha256_hex}`. Same bytes: success no-op. Planted different bytes: fail, no overwrite. | §5.6 | DD-62; AT-94, AT-106 |
| F-08 | Major | Snapshot-dir consent exemption is a drop box | Fixed. Four-part kite-written criterion. Third-party copies stay filtered. | §5.6 | DD-63; AT-107, AT-108 |
| F-09 | Major | Allowlist vs deny-list precedence | Fixed. Deny list always wins. Loopback fixtures use the `egress_test` feature and `--egress-test-loopback-port`, rejected without the feature. | §5.2.4 | DD-64; AT-88, AT-109 |
| F-10 | Major | LRS auth and statement contents undefined | Fixed. HTTP Basic, `POST {lrs-url}/xapi/statements`, `X-Experience-API-Version: 1.0.3`, statement schema in §5.5 (grounded in s3a-lrs-server and `xapi.rs`). | §5.5 | DD-65; AT-26, AT-110 |
| F-11 | Major | Retry queue can send after consent off | Fixed. Every send re-checks in-memory consent. Off at start deletes the queue. Failed revoke delete is visible and the queue is not sendable. | §5.5 | DD-66; AT-111, AT-124 |
| F-12 | Major | Hyper-tile no-consent rule inconsistent | Fixed. Zero core `ConsentRecord`s means unrestricted, matching `tour.rs`. Create still writes a grant so later restrictions do not hide the owner's records. | §4.2, §5.3, §5.6 | DD-67; AT-112, AT-79 |
| F-13 | Major | §11 step 3 builds on `tour_file` (type-82, stride-blind) | Fixed. Step 3 uses `MmapReader` for every file type, reuses only manifest bytes and floor-list fields, and computes `withheld_count` in kite. | §11 | — |
| F-14 | Major | Raw token bytes vs Bearer / trailing LF | Fixed. Header-safe charset, optional single trailing newline stripped at load, Bearer is the secret without that newline. | §5.2.2 | DD-68; AT-45, AT-81 |
| F-15 | Major | AT-31 requires a snapshot for on-disk papers | Fixed. AT-31 / AT-27 / AT-48 are pinned to a remote-fetched fixture. AT-104 covers on-disk-only inclusion (no snapshot). | AT-31, AT-27, AT-48, §5.6 | AT-104 |
| F-16 | Minor | §4.1 still said Boa is used for http(s) pages | Fixed. Boa is not used in v1. | §4.1 | DD-44 |
| F-17 | Minor | Cross-reference errors; actor_hash wording | Fixed. Create-time consent cites AT-79. AT-44 cites §5.7 "Native-link work." `actor_hash` is `hash_iri` in both §5.5 and DD-46. Category labels live in §5.4. | §5.6, §5.7, AT-23/AT-86 | DD-46 |
| F-18 | Minor | `device_pixel_ratio` missing from signatures; silent defaults | Fixed. Parameter is on every `render_*` and `/v1/frame`. Contract defaults are listed (DD-69) as an explicit exception. | §5.1, §5.2.1, §5.8 | DD-69; AT-96 |
| F-19 | Minor | "Linear area" is false at integer sides; no 128 KiB bay | Fixed. Area is approximately linear. Sides 32, 45, and 181. Fixtures rendered separately. | §5.8 | DD-70; AT-16, AT-92, AT-120 |
| F-20 | Minor | vw/vh and start rect undefined | Fixed. `1vw = width/100`, `1vh = height/100`. Start rect is the bay rect at the current scroll, including off-screen bays. | §5.8 | DD-18; AT-62 |
| F-21 | Minor | 401 vs 404 on unknown unauthenticated paths | Fixed. Authentication first: 401 without a token; 404 with a valid token on an unknown path. | §5.2.1 | DD-71; AT-06, AT-75, AT-115 |
| F-22 | Minor | POST /v1/consent vs AT-76; on without LRS | Fixed. Table says 409 `viewer_only`. `{state:on}` without LRS is 409 `lrs_not_configured`. | §5.2.1, §5.2.3 | AT-76, AT-116 |
| F-23 | Minor | Consent file `on` without LRS; DD-12 unreachable | Fixed. That combination is a startup error. DD-12's fail-closed clause is marked unreachable. | §5.2.3 | DD-12, DD-50; AT-114 |
| F-24 | Minor | Unix-socket bind and behind-proxy health | Fixed. `--bind unix:/path`; `--port` forbidden. With `--behind-proxy`, `/health` requires the token. | §5.2.2 | DD-52, DD-72; AT-87, AT-113 |
| F-25 | Minor | Token permissions, owner, logging | Fixed. Mode `0600`, owner is euid, v1 host is Unix-only. Token / Authorization never logged. | §5.2.2 | DD-73; AT-117 |
| F-26 | Minor | Handle lifetime, cap, hit_test concurrency | Fixed. `/v1/close`, required `--max-open-handles`, 429 when exceeded, per-handle lock, `409` `no_frame`. | §5.2.1 | DD-74; AT-118, AT-119 |
| F-27 | Minor | "fail or quarantine" for other strides | Fixed. `/v1/open` returns 200 `kind: "unsupported"`, `first_view: "banner"`; proceed is 409. | §5.3 item 1 | AT-97 |
| F-28 | Minor | Ed25519 "meant to cover" not operational | Fixed. Coverage is `DeviceSealRecord.tile_index` and every subsequent tile, in increasing `tile_index` order. | §5.4 item 2 | DD-57; AT-35 |
| F-29 | Minor | Deny-list gaps, proxy env, no fetch size cap | Fixed. All non-global unicast including mapped/translated forms; proxy env ignored; required `--max-fetch-bytes`. | §5.2.4 | DD-76; AT-122 |
| F-30 | Minor | Remote HTML subresources unspecified | Fixed. v1 HTML layout fetches no CSS, images, or fonts. | §5.2 item 5, §5.5 | DD-77; AT-121 |
| F-31 | Minor | Pass 1 timer has no test surface | Fixed. Timer is client-side. AT-29 is a 15-second wait after only candidate/frame/open reads. | §5.5 | DD-08; AT-29 |
| F-32 | Minor | "No Boa context" is unfalsifiable on native | Fixed. AT-34 is a guard on `cargo tree` (no `boa_engine` / `wasmtime`). | AT-12, AT-33, AT-34 | DD-44 |
| F-33 | Minor | AT-55 vector undefined | Fixed. The vector is a candidate `source_url` of `file://` to the token file, working archive, or a corpus file. | AT-55 | AT-123 |
| F-34 | Minor | Control-select vs one-archive / viewer-only | Fixed. AT-46 uses a second host process. AT-76 is exempt from the same-process control requirement. | §7 preamble | AT-46, AT-76 |
| F-35 | Minor | AT-39 already true; AT-98 vacuous mmap | Fixed. AT-39 is a guard once the tour Compose screen exists. Path mmap is forbidden in v1; AT-98 asserts `from_bytes` only. | §5.3 item 7 | DD-75; AT-39, AT-98 |
| F-36 | Minor | Viewer `--data-dir`; durable disk; locked open | Fixed. `--data-dir` is rejected in viewer-only mode. Durable disk is Scholar-only. Open of the locked working archive uses a `from_bytes` copy. | §1, §5.2.1, §5.2.2 | DD-48, DD-55, DD-78 |

## Blockers, in short

*   **F-01.** The pack API now has `render_page(handle, width, height, scroll_y, device_pixel_ratio)`. `/v1/frame` and `/v1/hit-test` accept `view=page`. Wiki and HTML first views are `page`. `hit_test` may return `Select { paper_id }` on a bound paper page or `Link { href }`. A Select hit does not write; the host or native window must `POST /v1/select` with the UI token. Links are not auto-fetched; a path that canonicalizes inside an allowed root may later be opened with `/v1/open`. `http(s)` hrefs are not opened from the page view in v1.
*   **F-02.** A paper's remote URL comes from the optional dossier YAML key `source_url` (draft name; not present in the grounded 3Pass tree, which uses `drive_path` — Q-12). `AcademicPaperRecord` has no URL string field. Fetch is triggered only by `POST /v1/select` or `POST /v1/download` of a **current candidate** that has a `source_url` and no successful snapshot yet. Side-effect order is (1) inclusion + index, (2) fetch + snapshot, (3) LRS. A fetch failure keeps the inclusion record, sets `fetch_error`, and writes no snapshot. On-disk-only papers are not snapshotted; download copies their bytes to `--download-dir/{sha256}`. There is no open-by-URL. Egress and snapshot ATs are pointed at this select/download path.

## Meaning changes to existing draft decisions

*   **DD-12:** the "consent on and URL missing" fail-closed state is unreachable.
*   **DD-32 / DD-42:** the native window is an HTTP client that sends the UI token; it does not call write FFI.
*   **DD-46:** `actor_hash` is `hash_iri` (`u64`), not CRC32C; idempotency uses the inclusion index, not `object_hash` alone.
*   **DD-48 / DD-55:** durable disk and `--data-dir` are Scholar-mode only.
*   **DD-52:** with `--behind-proxy`, `/health` requires the token.
*   **DD-53:** no-consent unrestricted applies to hyper-tile files as well as Edge; snapshot exemption is DD-63.
*   **DD-56:** download names are content SHA-256; same-bytes re-download is a success no-op.

## New draft decisions and questions

*   **DD-58–DD-78:** page render and links; source model and fetch trigger; native window / headless; identity and inclusion index; download filename; kite-written snapshot criterion; deny-list precedence; LRS client; retry re-checks consent; hyper-tile no-consent rule; token charset; frame-parameter defaults; bay sides; 401/404 order; Unix-socket bind; Unix host and token privacy; handles; no path mmap; fetch size and deny-list completeness; no HTML subresources; viewer data-dir and locked reads.
*   **Q-12.** Confirm the optional dossier YAML key name (`source_url` is the draft).
*   **Q-13.** Whether `--lrs-actor-homepage` should later be a fixed Scholar IRI rather than a required flag.

