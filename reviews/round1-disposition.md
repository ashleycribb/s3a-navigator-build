# s3a-kite goldfish round 1 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round1.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round1.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v2 (self-contained copy at `/workspace/goldfish/s3a-kite.md`).
*   **Totals:** 38 findings (3 Blocker, 21 Major, 14 Minor). 37 fixed in the spec (several with a draft value recorded for Ashley); 0 rejected; 1 deferred (F-36, numeric device budgets left to Q-06 / DD-06).
*   **Pin rebase:** grounding moved from Kitesurf `4a29038` to `235111c6fe10f2d63f3476bf5e3d19b4c7089415` (Ashley's [s3a-Kite](https://github.com/ashleycribb/s3a-Kite) `main`, same as upstream `main`). That commit is TypeScript-only; Rust core claims are unchanged.
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | No native-host JS model for `http(s)` pages | Fixed; draft model recorded for Ashley | §5.2 item 4; §4.1 wasm32 Boa gates | DD-26; AT-33 |
| F-02 | Blocker | Write target, API, seal, ADDON_EDU, locking undefined | Fixed; draft path recorded for Ashley | §5.6 | DD-16, DD-27, DD-28; AT-46, AT-47, AT-73, AT-74 |
| F-03 | Blocker | Tracing consent also gated local inclusion / snapshots | Fixed (Ashley's standing rule) | §2 Inclusion / Tracing consent; §5.5 | DD-09 (meaning changed), DD-12 (narrowed); AT-27, AT-48 |
| F-04 | Major | Phone tracing rule contradicts DD-12, AT-27, no-network app | Fixed | §5.7 Android; §5.5 | DD-12, DD-39; Q-03 |
| F-05 | Major | On-demand font downloads impossible without network | Fixed | §5.7 Fonts | DD-22, DD-35; AT-49; Q-05 |
| F-06 | Major | AT-10/AT-11 use a defective CLI as oracle | Fixed. The type-82 path is a real `build_consent_state` branch, not only a comment. | §4.2; §5.3 item 4 | AT-10 (floor list still CLI), AT-11 (golden fixtures) |
| F-07 | Major | AT-07 "non-documented production mode" untestable | Fixed | §5.2 item 2; §8 DD-03 | AT-07 |
| F-08 | Major | Health URL vs fail-closed auth undefined | Fixed | §5.2 Health | DD-29; AT-04, AT-06, AT-50 |
| F-09 | Major | TLS and token handling missing | Fixed | §5.2 TLS / tokens | DD-29; AT-45, AT-51 |
| F-10 | Major | CDP scope and filesystem roots unbounded | Fixed | §5.2 roots and CDP | DD-30, DD-32; AT-52, AT-53 |
| F-11 | Major | "Sandbox" and "egress policy" undefined | Fixed | §5.2 sandbox and egress | DD-31; AT-54, AT-55 |
| F-12 | Major | Wiki Markdown unsanitized; remote subresources | Fixed | §5.5 | DD-13; AT-56 |
| F-13 | Major | Human vs agent select unenforceable over CDP | Fixed | §5.2 CDP; §5.5 | DD-32; AT-28, AT-57 |
| F-14 | Major | Actor, consent storage, revocation missing | Fixed; provisioning left as a question | §5.5 consent model | DD-33; AT-58; Q-02, Q-08 |
| F-15 | Major | Inclusion side-effect failure / idempotency unspecified | Fixed | §5.5 side effects | DD-34; AT-59, AT-60 |
| F-16 | Major | Host logs/caches outside "writes nothing" | Fixed | §5.5 logs; AT-24/AT-25 extended | DD-34; AT-61 |
| F-17 | Major | Linear bay area not testable across strides | Fixed | §5.8 | DD-37; AT-16 |
| F-18 | Major | Animation has no frame model | Fixed | §5.8 | DD-18, DD-37; AT-13, AT-62 |
| F-19 | Major | Text assertions and accessibility need a text layer | Fixed | §5.8 | DD-38; AT-08, AT-38, AT-63 |
| F-20 | Major | "Every test fails today" is false for some ATs | Fixed by honest reclassification, not by inventing failures | §7 preamble | AT-22, AT-37 guards (already pass); AT-05, AT-36, AT-41 skipped until the subject exists |
| F-21 | Major | Names "as implemented" and two-repo test locations | Fixed | §5.1 names, test locations, pins | DD-01; AT-01, AT-38, AT-40 |
| F-22 | Major | "Not forking on GitHub" conflicts with vendoring and the app | Fixed. The fork now exists; §3 splits document vs product non-goals. v1 distribution is in scope, when Ashley authorizes it, under §6. | §3; Status | — |
| F-23 | Major | Who verifies on mobile is ambiguous | Fixed | §5.4 item 2; §5.7 | DD-19, DD-20; AT-18, AT-35, AT-72 |
| F-24 | Major | Event colors contradict EDGE §10 | Fixed. DD-07 color mapping is unchanged and is an explicit override of EDGE §10; a non-color category label is required. | §4.3; §5.4 item 5 | DD-07; AT-19 |
| F-25 | Minor | Manifest-to-tour trigger unspecified | Fixed | §5.3 items 2–3 | DD-36; AT-09 |
| F-26 | Minor | Tile 78 UTF-8 / multiple / quarantined cases missing | Fixed | §5.3 item 2 | AT-64, AT-65 |
| F-27 | Minor | Torn vs CRC-failed vs chain-break inconsistent | Fixed | §5.4 item 2 | DD-20; AT-17, AT-18, AT-35 |
| F-28 | Minor | Missing identity record and odd type-64 sizes | Fixed | §5.4 items 3, 5 | DD-40; AT-67, AT-68 |
| F-29 | Minor | Inconsistent Edge flags unhandled | Fixed | §5.4 item 1 | DD-40; AT-66 |
| F-30 | Minor | AT-21 condition is always true | Fixed | — | AT-21 |
| F-31 | Minor | "Research-purpose views" undefined | Fixed | §4.2; §5.4 item 6 | AT-23 |
| F-32 | Minor | Snapshot bytes/metadata ambiguous; AT-32 vs §5.6 | Fixed | §5.6 | AT-32 |
| F-33 | Minor | AT-02 misses MPL offer, APK, in-app notices | Fixed | §6.1–6.2 | AT-02, AT-70 |
| F-34 | Minor | AT-03 and AT-36 have no mechanical check | Fixed | §7 preamble | AT-03, AT-36 |
| F-35 | Minor | Weak wording on AT-43, AT-41, AT-40 | Fixed | — | AT-40, AT-41, AT-43 |
| F-36 | Minor | No performance/memory budget, so DD-06 can't fire | Deferred to Q-06 / DD-06. Structure is named (peak RSS, first-frame time, APK delta, named minSdk-26 device). Numbers are not invented. | §5.7 Budgets | DD-06; Q-06 |
| F-37 | Minor | Malformed/malicious input robustness unspecified | Fixed. No extra size cap was invented. SAF uses existing `from_bytes`. | §5.3 item 7; §5.9 | AT-71 |
| F-38 | Minor | Cross-refs, KB vs KiB, timestamps, file versions | Fixed | Glossary; EDGE prefixes; §4.2; §5.3 item 1 | AT-69 |

## Blockers, in short

*   **F-01.** v1 page JS (draft DD-26): existing `wasm32` `kitesurf-core` (Boa) under Wasmtime 36 in one sandboxed OS process per page. Chosen because `boa_eval.rs` / `js.rs` are wasm32-only and `kitesurf-cli` already depends on Wasmtime 36 (scene playback only). The `.s3a` tour still does not use Wasmtime (DD-02). AT-33 checks that a page script can set `document.title`.
*   **F-02.** Writes go to a configured per-researcher working archive, not the corpus (DD-27). API is existing `TileWriter` + Scholar `create_human_lrs`, exclusive lock, `ADDON_EDU` fail-closed on an existing file, snapshots sealed separately with `seal_archive` / `verify_sealed_archive` (DD-28).
*   **F-03.** Local inclusion records and snapshots no longer depend on tracing consent (DD-09 meaning changed per Ashley). xAPI to the LRS stays consent-only. AT-27 was reworded; AT-48 covers download with consent off.

## Meaning changes to existing draft decisions

*   **DD-09:** snapshots/inclusion no longer require tracing consent.
*   **DD-12:** fail-closed missing-URL behavior now applies only to the LRS statement; local writes still happen.
*   **DD-01 / DD-17:** pin updated to `235111c`.
*   **DD-07:** color mapping unchanged; documented as an override of EDGE §10, with a required non-color label.
