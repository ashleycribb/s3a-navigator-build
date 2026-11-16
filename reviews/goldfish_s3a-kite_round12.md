# Goldfish review — s3a-kite spec v12 (round 12)

Reviewer: independent fresh read of `/workspace/goldfish/s3a-kite.md` only (no other files, wiki, prior reports, or repo consulted). Spec not edited; nothing committed or pushed.

**Summary: 0 blocker / 5 major / 15 minor (20 findings).** Not a clean round.

Majors:
- F-01 Edge: a winning valid-floor type-71 denial with no grant anywhere has no stated withholding rule (AT-23 / AT-86 depend on it)
- F-02 Hyper-tile: the "grant" / filter-mode trigger is undefined for expired or withdrawn records on authenticated floors (AT-161, DD-67, AT-184 inversion)
- F-03 `lrs_inhibit`: the latch representation and the check / clear / cancel-registration atomicity are unspecified, so a send can slip through after a revoke is received
- F-04 Interface gap: there is no pack accessor for the `KiteVerificationReport` of an `open`ed handle, so AT-165 / AT-137 field equality cannot be tested, and the mapping's "valid floor" is circular for the app
- F-05 §5.1 pack `invalid_tile_indices` uses `tile_count`; the mapping table says it must be `tiles_total` "not `tile_count`"

---

## F-01 — major — Edge valid-floor denial with no grant: no rule withholds anything

**Where:** §5.4 item 6; AT-23; AT-86; AT-85; DD-111.

**Text:**
- §5.4 item 6: "The host first considers type-71 records that sit on **valid floors** … the last of those wins … That record allows the research purpose when `(allowed_purposes & PURPOSE_RESEARCH_ANALYTICS) != 0` and it is not expired."
- "Filter mode (event-log payloads without a trusted grant are withheld) is on when the file is unauthenticated as a whole, or any parseable type-71 research grant exists in any tile."
- "Then any parseable type-71 **denial** … found in an invalid or unauthenticated tile … is applied as an extra restriction."
- "The tour is unrestricted only when … no parseable type-71 research grant exists in any tile, and no restrictive record appears in an invalid or unauthenticated tile."
- AT-23: "When an Edge type-71 `EdgeConsentRecord` does not allow the research purpose, kite withholds event-log payload records."

**Problem:** Take an authenticated Edge file whose only type-71 record is a denial on a valid floor. That is the natural AT-23 / AT-86 fixture.
- Filter mode is off, because no grant exists.
- The extra-restriction rule covers only invalid or unauthenticated tiles.
- The "unrestricted only when" sentence states a necessary condition, and this file meets it.

So no normative sentence withholds the payloads. A literal implementation shows denied event-log payloads and fails AT-23. That is a privacy leak on the core consent case. The phrase "that record allows …" hints that a denial should withhold, but it never says what happens outside filter mode.

**Fix:**
- (a) Add: "If the winning valid-floor type-71 record does not allow research (denied or expired), event-log payloads are withheld." Also add "or a winning authenticated denial exists" to the filter-mode trigger, and make the unrestricted condition "no type-71 record of any kind in any tile."
- (b) Match `tour.rs`' "when consent records exist" rule: filter mode is on whenever any parseable type-71 record exists in any tile.

## F-02 — major — Hyper-tile: "grant" and the filter-mode trigger are undefined for expired or withdrawn records on authenticated floors

**Where:** §5.3 item 4; DD-67; DD-111; AT-161; AT-184; AT-136; §4.2 (`tour.rs` "when consent records exist, records without active purpose=research are withheld").

**Text:**
- §5.3: filter mode "is on when the file is unauthenticated as a whole, **or** any parseable research grant exists in any tile."
- §5.3: unrestricted "only when … no parseable research grant exists in any tile, and no withdrawn marks or extra-restriction withdrawals apply."
- The untrusted-tile text: "A **grant** (`is_active` true)".
- DD-67: "Unrestricted only if … no withdrawals or marks apply."

**Problem:**
1. Take an authenticated, unsealed file whose only research `ConsentRecord` for subject X is an expired grant (AT-161 after the clock advances) or a withdrawal (`withdrawn_at != 0`).
   - If "grant" means `is_active == true`, no grant exists, so filter mode is off and nothing withholds X. AT-161 ("that subject's records are withheld") then fails for a single-record fixture.
   - DD-67 says that file is "not unrestricted", because a withdrawal applies. But §5.3 gives no rule that would withhold X, so the state is undefined.
2. Inversion: AT-184 withholds X for a withdrawal in an **invalid** tile, but the same withdrawal on an **authenticated valid** floor withholds nothing. Invalid evidence ends up more restrictive than valid evidence.
3. §4.2 grounds the rule in `tour.rs` ("when consent records exist"), which this rule does not follow.

**Fix:**
- (a) Define "research grant" for the filter-mode trigger as any parseable research `ConsentRecord` with `withdrawn_at == 0`, whatever `expires_at` says. Separately, state that a subject whose winning trusted record is inactive (withdrawn or expired) is withheld even outside filter mode.
- (b) Make filter mode trigger on any parseable research `ConsentRecord` in any tile, active or not. Keep AT-184 as the explicit exception for a lone invalid-tile withdrawal.

Either way, fix the DD-67 wording to match, and add an AT for a lone valid-floor withdrawal and a lone expired grant.

## F-03 — major — `lrs_inhibit`: representation and atomicity are underspecified, so an LRS POST can start after a revoke is received

**Where:** §5.2.1 "Consent transitions and in-flight LRS"; DD-144; DD-142; §5.5 step 3; AT-198; AT-156.

**Text:**
- "a lock-free **`lrs_inhibit` latch** (an atomic revoke epoch)"
- "sets `lrs_inhibit` (increments the epoch)"
- "`lrs_inhibit` is cleared only when a qualifying `POST {"state":"on"}` persists (under the mutex, and only if the epoch is still the epoch observed when that `on` was received)"
- "cancels every in-flight LRS POST through a shared cancellation token"

**Problem:**
1. A monotonic epoch cannot be "set" or "cleared". The spec does not say whether the latch is a boolean, an epoch compared against a "cleared-at" epoch, or both.
2. A revoke's increment is lock-free, so it can land between the `on` request's epoch check and its clear. Sequence:
   1. The `on` checks the epoch (unchanged).
   2. A revoke R increments the epoch and fires the cancel token.
   3. The `on` clears the latch and sets the flag `on`.
   4. A step-3 or replay send now passes the pre-send check before R gets the mutex.
   
   An LRS POST then goes out after the revoke was received, which breaks "off always wins".
3. Pre-send TOCTOU. Step 3 checks inhibit and the flag, then starts the POST. If the revoke's token cancellation fires between the check and the POST's registration with the token, the POST is never cancelled.

**Fix:**
- Specify that clearing is a single compare-and-swap on the epoch word (for example, inhibit is active iff `epoch != cleared_epoch`, and the `on` performs CAS(`cleared_epoch`: old → observed_epoch) only if the epoch still equals observed_epoch).
- Require every LRS sender to (1) register with the current cancellation token, (2) re-check inhibit, then (3) start the POST.
- Require a revoke to swap in a fresh token after cancelling.
- Add an AT hook that forces each interleaving.

## F-04 — major — No pack accessor for verification results; AT-165 / AT-137 cannot be tested as written, and the mapping is circular

**Where:** §5.1 FFI table; the phone export list; the DD-130 mapping table (`invalid_tile_indices` row); AT-165; AT-137; DD-115.

**Text:**
- AT-165: "pack `open` and a reader-core `verify` … yield identical `status`, `seal_status`, `invalid_tile_indices`, `break_index`, `torn_tile_indices`, and `quarantine_reason`."
- The §5.1 API returns only `Handle` from `open`. No function returns a `KiteVerificationReport` (or its fields) for a handle, and neither the host `/v1/open` response nor the phone UniFFI list exposes one.
- Mapping row: "`invalid_tile_indices`: Every index in `0 .. tiles_total-1` that is not a valid floor."

**Problem:**
- The reader-side test can only compare fields by scraping banner text. "Identical `invalid_tile_indices`" is not observable at all.
- The app is told to fill `invalid_tile_indices` from "valid floor", which is a pack concept defined by §5.3 / §5.4. Reader-core has no such concept, so the app would have to re-implement the pack algorithm (and DD-145 attribution) to build a report that AT-165 accepts.

**Fix:**
- (a) Add `verification_report(handle) -> KiteVerificationReport` to the pack API and to phone UniFFI (read-only, pure). AT-165 compares that to the mapped reader-core report.
- (b) Add `verify_bytes(ctx, bytes, pinned_keys) -> KiteVerificationReport`.

Also define the mapping's valid floors in reader-core terms, or have the app call (b) instead of hand-mapping.

## F-05 — major — `tile_count` vs `tiles_total` contradiction in the pack definition of `invalid_tile_indices`

**Where:** the §5.1 paragraph after the `KiteVerificationReport` fields; the mapping table `invalid_tile_indices` row; DD-130; AT-165; AT-195.

**Text:**
- §5.1: "When no tile is a valid floor, it is every index in `0 .. tile_count-1`."
- The mapping row: "`tiles_total` … may differ from the header `tile_count` … both pack text and AT-165 / AT-195 use this name (`tiles_total`), not `tile_count`."
- DD-130: "(all indices when no tile is valid …)", with no range named.

**Problem:** For a `signer_not_pinned` or `missing_signature` file with torn or uncommitted trailing tiles (the AT-165 set includes MICRO-VEC-02-like cases), the two ranges differ. Pack `open` would then disagree with the mapped report, and AT-165 / AT-195 fail depending on which sentence the implementer follows.

**Fix:** Change §5.1 to `0 .. tiles_total-1` and define `tiles_total` for the pack (the indices `MmapReader` enumerates, including torn and uncommitted trailing tiles). Name the range in DD-130 too.

---

## F-06 — minor — When is the `drive_path` fd opened, and what happens when it becomes unusable at step 2b?

**Where:** §5.5 source decision table; numbered steps 0–2b; DD-146; AT-197; AT-202; AT-129.

**Problem:**
- The table row ("usable drive_path" yes/no) decides whether to fetch, but the spec never says when the DD-146 open happens relative to step 0/1 or the membership check.
- Step 2b says a source that exceeds `--max-open-bytes` + 1 during the copy is "no usable on-disk source". By then step 1 is done and the no-fetch row was chosen. The response is undefined: which `download_error`? Does a `source_url` row-1 fetch now run?
- AT-202 (symlink swapped in after listing) has the same gap when the candidate also has a `source_url`.

**Fix:** State that the fd is opened and validated once per request before step 0, outside the mutex (`O_NONBLOCK`), and that row selection uses that result. A cap overflow during the 2b copy then yields `download_error: "fetch_failed"`, or a new `"source_changed"`, with the inclusion kept and the temp deleted.

## F-07 — minor — DD-146 walk vs canonicalization order and error mapping are underspecified

**Where:** §5.2.2 path rule; DD-146; AT-52; AT-160; AT-202.

**Problem:**
- The rule says request paths "must canonicalize strictly inside an allowed request root" and also says "walk each path component with `O_NOFOLLOW`". Canonicalizing first (realpath) resolves symlinks and defeats the walk; walking the raw path needs a lexical `..` policy (is an in-root `a/../b` allowed?).
- An in-root symlink at an intermediate or final component that points inside the root fits neither `path_escape` ("would leave a root") nor `not_regular_file` (no fd was obtained).
- The working-archive path is outside every root, so "staying beneath the allowed root" does not define its walk.

**Fix:** Define the walk as lexical normalization (reject a `..` that rises above the root, allow in-root `..`, or reject all `..`), then a component walk from the root's dirfd with `openat(O_NOFOLLOW)`. Map `ELOOP` / `ENOTDIR` from a symlink to one named code (for example `path_escape`). Give the working archive its own rule: its canonical parent dirfd, then `openat(O_NOFOLLOW)`.

## F-08 — minor — Host-managed names inside `--data-dir` can collide with user-supplied state paths

**Where:** §5.2.2 disjointness; DD-93; DD-129; DD-114; §5.2.1 revoke steps.

**Problem:** Nothing forbids these setups:
- `--consent-state-file` at `--data-dir/lrs-retry/x`, which revoke step 3 deletes.
- `--consent-state-file` at `--data-dir/revoke-pending` or `--data-dir/inclusion-index.jsonl`.
- `--working-archive` at `--data-dir/.lock` or `--data-dir/inclusion-index.jsonl`.

The working-archive parent is not required to be disjoint from `--data-dir`, and the consent file is only checked against request roots.

**Fix:** Reserve `--data-dir/{.lock, inclusion-index.jsonl, revoke-pending, lrs-retry/}` and the `*.lock` sidecar names. Make a consent-file or working-archive path equal to, or inside, a reserved name a startup error that names both flags. Or require the consent file and the working archive to be outside `--data-dir`.

## F-09 — minor — Does the create-time `ConsentRecord` also apply to snapshot archives?

**Where:** §5.6 "API and locking" ("On `create`, kite also writes one core `ConsentRecord`"); DD-28; AT-107; DD-63 note.

**Problem:** Snapshots are also new archives (sealed BLOB + PROVENANCE) and are presumably made with `TileWriter::create`. The sentence is not scoped to the working archive, but AT-107 asserts that snapshots have "no `ConsentRecord`".

**Fix:** Scope the sentence: "On `create` of the **working archive** only …". State that snapshots get no `ConsentRecord` and say which addon bits they carry.

## F-10 — minor — Whether `AcademicPaperRecord` carries a `subject_id` is never stated

**Where:** §5.5 candidate item 2; DD-117; AT-161 (second half); AT-166; §5.3 ("A payload record **without** a `subject_id` is not withheld by `ConsentRecord`").

**Problem:** AT-161 and AT-166 require consent-filtered `AcademicPaperRecord` candidates. If the record has no `subject_id` (§4.7 lists its fields without one), only marks can withhold it, and the clock-advance half of AT-161 cannot pass.

**Fix:** State the field `AcademicPaperRecord` uses for consent (or that it is mark-only), and adjust AT-161 / AT-166 fixtures to match.

## F-11 — minor — Relative `Link` href is undefined for handles opened from bytes

**Where:** §5.1 `Link { href }` ("resolved against the **parent directory of the opened page**"); the phone `open` of SAF bytes; `open(ctx, bytes, name_hint, …)`.

**Problem:** `wiki` / `html` handles from `open` (the phone, or any bytes caller) have no path, so a relative href's value is undefined.

**Fix:** For byte-opened handles, return the raw attribute with the fragment stripped (or `None` / no `Link` hit) and say so. Add one AT on the pack.

## F-12 — minor — AT-198 asserts internal state

**Where:** AT-198 ("`lrs_inhibit` remains set").

**Problem:** A black-box host test cannot observe the latch.

**Fix:** Replace it with observable consequences (zero LRS requests for a subsequent select and replay, and no retry file), or name a test-only introspection hook behind a cargo feature.

## F-13 — minor — What happens to a replay's `.claimed` file when its POST is aborted (`skipped`)?

**Where:** §5.5 queue file names; DD-134; AT-156.

**Problem:** The defined outcomes are success (delete), queued (rename back to `.json`), and permanent (rename to `.permanent`). An aborted or skipped replay has no defined disposition. If it renames back to `.json` after the revoke deleted the directory, it either fails or recreates a statement file after the revoke.

**Fix:** An aborted or skipped replay leaves the claimed file for the revoke delete and performs no rename. If the directory is gone, it does nothing. State that it never recreates `lrs-retry/`.

## F-14 — minor — Download when a snapshot exists and the `drive_path` is now usable

**Where:** §5.5 table row 3 ("prefer on-disk"); §5.6 ("A later download of that identity reassembles bytes from the snapshot BLOB tiles"); DD-61.

**Problem:** An earlier select fetched a snapshot while the `drive_path` was unusable; the path later becomes usable. The two rules then prescribe different bytes for the download.

**Fix:** Pick one, for example "if any snapshot exists for `(file_uuid, identity)`, deliver from it", and state the precedence in the table.

## F-15 — minor — Idempotent select can return a dangling snapshot path

**Where:** §5.5 index ("returns `200` … `snapshot` set to `--snapshot-dir/{snapshot_sha256}.s3a`"); §5.2.1 select row.

**Problem:** If the snapshot file was removed or replaced since it was indexed, select reports a path that does not exist or fails the reuse checks. Only download has `snapshot_invalid`.

**Fix:** On a repeat select, run the DD-103 reuse checks. On failure, return `snapshot: null` plus a code (for example `snapshot_invalid`), or allow a re-fetch.

## F-16 — minor — `--lrs-url` trailing-slash normalization

**Where:** §5.5 LRS client ("`--lrs-url` is the origin … the host appends `/xapi/statements`"); AT-54 and DD-140 use `http://127.0.0.1:<lrs-port>/`.

**Problem:** It is unclear whether a trailing `/` (or any path) is accepted, rejected, or normalized. A naive join produces `//xapi/statements`.

**Fix:** Accept exactly `scheme://host[:port]` with an optional single trailing `/` that is stripped. Make any other path a startup error that names `--lrs-url`.

## F-17 — minor — Error precedence for the non-frame JSON routes is unspecified

**Where:** §5.2.1 error catalogue (precedence is given only for `/v1/frame` and `/v1/hit-test`); AT-76; AT-105.

**Problem:**
- A viewer-only host receiving malformed JSON on `/v1/select` could return `400` or `409` `viewer_only`.
- On a Scholar host, the order of `missing_param` vs `unknown_paper` vs `addon_missing` is not given.
- In `/v1/hit-test`, it is unclear whether `"floor": null` is "omitted" or `invalid_param` (the response uses `null`).

**Fix:** Add one ordered list for every route (auth → 405 → 413/415 → `viewer_only` → JSON / param → route-specific). Define a JSON `null` as equal to omission for optional fields.

## F-18 — minor — Stale DD text vs the normative sections

**Where:** DD-101; DD-94; DD-74; DD-99.

**Problem:**
- DD-101 says "`register_ua_fonts` takes the font bytes … Tests pass an empty set" and DD-94 says "Registration is injectable". The API is `KiteContext::new(ua_fonts)` (§5.1, DD-121).
- DD-74 says "`hit_test` uses the last frame of that handle and view", without floor or alias normalization (superseded by DD-125).
- DD-99's field list omits `tiles_crc_failed`.

**Fix:** Update those DD rows, or mark them "superseded by DD-121 / DD-125".

## F-19 — minor — `paper_identity` for the corpus-root copy of a dossier that the wiki root wins

**Where:** §5.1 `paper_identity` ("`wiki_root` wins if the same relative path exists under both"); §5.5 duplicate rule.

**Problem:** It is unclear whether opening `{corpus_root}/papers/SRC-0001.md` returns `Some("SRC-0001")`. If it does, Select on that page includes the wiki candidate, whose `drive_path` and `source_url` come from a different file. If it returns `None`, the page has no Select.

**Fix:** State that `paper_identity` is `None` for the losing duplicate (or `Some`, with the candidate's attributes always taken from the winning file), and add an AT.

## F-20 — minor — Edge torn vs committed-CRC distinction is not defined, and the mapping may conflate them

**Where:** §5.4 per-tile outcomes; the mapping row `torn_tile_indices` ("reader `tile_at` is `Err` or the tile is flagged torn"); AT-17; AT-35; AT-165.

**Problem:**
- A torn tile does not quarantine; a committed CRC failure does.
- The spec never defines how the pack tells a torn Edge tile from a committed CRC-failed one.
- If reader-core's `tile_at` returns `Err` on a CRC failure, the mapping classes a CRC-failed tile as torn, with no `crc` reason and no Quarantine, so it disagrees with pack `open` under AT-165.

**Fix:** Define "torn / uncommitted" for Edge by a concrete predicate (for example a header / commit-marker state from `s3a-edge`). Make the `torn_tile_indices` mapping use that predicate, not `tile_at` `Err`.
