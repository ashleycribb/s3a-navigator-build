# Goldfish review — s3a-kite spec v5 (round 5)

Reviewer: independent, no prior context. Source read: `/workspace/goldfish/s3a-kite.md` only (946 lines, v5, 2026-11-16).
Scope rules applied: DDs, Qs, deliberate exclusions and listed external requirements are not findings unless the spec contradicts itself about them.

## Summary

| Severity | Count |
| --- | --- |
| Blocker | 2 |
| Major | 12 |
| Minor | 19 |
| **Total** | **33** |

Blockers are places where two normative parts cannot both be satisfied, so the fail-first suite can't go green "without editing the tests" (§7 preamble).

---

## Blockers

### F-01 — Blocker — AT-04 vs §5.2.2 / AT-77 (required Scholar flags)
**Quote (AT-04):** "`s3a-kite host` with every Scholar-mode required flag in §5.2.2 (`--bind 127.0.0.1`, `--port <ephemeral>`, … `--max-fetch-bytes`, `--max-open-handles`, `--headless`) starts …"
**Quote (§5.2.2):** Scholar required flags include "`--max-redirects`, `--max-open-handles`, `--max-frame-pixels`, `--handle-idle-secs`". §5.2 item 2: "Missing required flags are an error in every mode."
**Problem:** AT-04's explicit flag list leaves out `--max-redirects`, `--max-frame-pixels`, and `--handle-idle-secs`. Under §5.2.2 and AT-77 that command line has to fail at startup, while AT-04 says it starts and serves `/health`. Both tests can't pass. AT-04 also calls the program `s3a-kite host`, but §5.1 names only the binary `s3a-kite` and never defines a `host` subcommand.
**Fix:** Have AT-04 point at the §5.2.2 list ("every flag in §5.2.2 Scholar mode") rather than copy it, or add the three missing flags. Define the CLI shape (`s3a-kite host …` vs `s3a-kite …`) in §5.1 or §5.2.2.

### F-02 — Blocker — §5.2.5 / AT-130 / AT-148 vs §5.3 item 1 / AT-95 (non-S3A bytes)
**Quote (§5.2.5):** "`open(bytes)` without a name hint, and without S3A magic, is therefore `other`." AT-130: "no hint (and no magic) → `other` / `page`."
**Quote (AT-95):** "`open` of a byte string that is not an S3A file returns `Err` and the process does not abort."
**Quote (§5.3 item 1):** "If it is not an S3A file, fail with engine `Invalid S3A file magic`, or on mobile the existing `This is not an S3A file`."
**Problem:** The same input (no-magic bytes passed to `open`) is required to return `Ok(handle)` with kind `other` and also `Err`. §5.3 also leaves open whether a `foo.s3a` with bad magic shows "no text conversion in archive" (per §5.2.5) or the mobile "This is not an S3A file" error.
**Fix:** Pick one. Since kind detection was the v5 change, the likely fix is to reword AT-95 as "`open` of non-S3A, non-hinted bytes returns `Ok` with `kind: other`; malformed S3A bytes (magic present, bad header) return `Err`". Then cut §5.3 item 1's "not an S3A file" clause back to "magic present but header unopenable". Say what the phone shows for a `.s3a`-named file without magic.

---

## Major

### F-03 — Major — §5.2.3 / DD-50 (consent-file bootstrap)
**Quote:** "A missing file with `--lrs-url` set, or a malformed file, is a startup error (no invented `on` default)." / "The host is the only writer." / "`POST /v1/consent` is the only supported toggle."
**Problem:** You can't start a host with `--lrs-url` until the consent file exists, and only a running host may write that file. The first LRS-enabled deployment therefore needs someone to hand-write `off\n<timestamp>` with mode 0600 outside the host. That breaks "host is the only writer". No AT covers first run.
**Fix:** Let the host create a missing consent file as `off` + timestamp (0600) when `--lrs-url` is set. `off` is the privacy-safe state, so this isn't an invented `on`. Or keep the startup error and document an `s3a-kite consent init` command. Add an AT either way.

### F-04 — Major — §5.3 item 4 / DD-82 vs error catalogue (omitted `floor`)
**Quote (§5.3 item 4):** "A request whose `floor` is omitted (where required) or is not the tile index of a valid floor returns `409` `invalid_floor`." DD-82 says the same.
**Quote (catalogue):** "400 | `missing_param` | A required query field (`w`, `h`, `handle`, `view`, `floor`, `t`) is absent."
**Problem:** Omitting `floor` is specified as both 400 `missing_param` and 409 `invalid_floor`. DD-83 makes the catalogue normative, and so do §5.3 and DD-82.
**Fix:** Omitted → 400 `missing_param`; present but not a valid tile index → 409 `invalid_floor`. Update §5.3 item 4 and DD-82, and add the omitted case to AT-138.

### F-05 — Major — §5.1 `hit_test` vs §5.3 item 3 / DD-36 / AT-09 (does a Continue hit change state?)
**Quote (§5.3 item 3):** "The first view stays until `proceed_to_warehouse(handle)` or a Continue `hit_test` on that view." AT-09: "After `proceed_to_warehouse` (or a Continue `hit_test`), `render_warehouse` is the next view."
**Quote (§5.1):** "`hit_test` … Map a point in the last frame of `view` for this handle to a `Hit`." A Select hit "does not write"; the UI must call the write route.
**Problem:** It's unclear whether `hit_test` returning `Continue` also moves the handle into the warehouse, which would make hit-testing state-changing. If it does, `POST /v1/hit-test` is a hidden proceed and `view_mismatch` behaviour depends on where the user happened to click. If it doesn't, AT-09's "(or a Continue `hit_test`)" can't be tested. HTTP `/v1/proceed` and `/v1/hit-test` currently disagree.
**Fix:** State that `hit_test` is pure and that the client calls `proceed_to_warehouse` / `POST /v1/proceed` after a `Continue` hit. Reword §5.3 item 3, DD-36 and AT-09 to match.

### F-06 — Major — §5.1 / §5.7 / DD-80 (pinned keys have no way into `open` / `open_path`)
**Quote (§5.7):** "If the app calls `open` instead, kite runs the same pinned-key algorithm as the host (`--pinned-keys-file` / the list the app passes)."
**Quote (§5.1):** "`open(bytes, name_hint, max_frame_pixels)`", "`open_path(path, max_frame_pixels)` … The host fills verification from `--pinned-keys-file`." §5.4: "the pack does not read a host flag."
**Problem:** Neither `open` nor `open_path` takes a pinned-key list, and the pack is barred from reading host flags. So the "list the app passes" has no parameter, and it's undefined how the host-side `open_path` gets the pinned list. Pinned-key behaviour on these paths (AT-133, AT-134) can't be implemented as signed.
**Fix:** Add `pinned_keys: Vec<[u8;32]>` to `open` and `open_path`, or specify that the host always runs verification and calls `open_verified`, and drop the "list the app passes" clause.

### F-07 — Major — §5.3 item 6, §5.4 item 2, §5.1 `KiteVerificationReport` (verification label model is incomplete)
**Quote:** "UI labels … are exactly: **Verified**, **Self-signed**, **Unsigned**, **Quarantined**." / "`status` (`verified` / `self_signed` / `unsigned` / `quarantined`)". §4.5 lists reader-core `Unverified`. AT-72 and AT-147 require an "unsealed" seal status.
**Problem:**
- (a) There's no label or mapping for a sealed-OK hyper-tile file, which uses seal-trailer verification with no signer. Is it Verified, Unsigned, or something else?
- (b) Reader-core `Unverified` has no `KiteVerificationReport` value, so the app's mapping is undefined.
- (c) "Unsealed" is a seal status, but there's no field for it in the report, and its relation to the four labels is unstated.
- (d) A non-empty pinned list plus a file with no signatures and `FLAG_ED25519_SIGNED` clear could be Unsigned or Quarantined. Not stated.
**Fix:** Add a table from (file kind × seal state × signature result × pinned-list state) to (label, seal-status text, quarantined?). Add `unverified` and a separate `seal_status` field to `KiteVerificationReport`, or define how they map.

### F-08 — Major — §5.4 item 2 / DD-57 / DD-79 / AT-35 / AT-17 (per-tile invalidation vs file-level Quarantine)
**Quote:** "The first failed signature … invalidates the tile at that `tile_index` and every subsequent tile; earlier tiles that already verified remain valid floors." / "If the pinned list is non-empty, the signer must be in it or … the file is Quarantined." AT-35: "A hyper-tile or Edge file with a failed tile CRC … is labeled quarantined." AT-17: "the torn tile is omitted and the header sheet reports it as ignored/torn."
**Problem:**
- It's unclear whether a mid-chain signature failure makes the whole file Quarantined or a labeled partial file, and whether it's Self-signed up to the break.
- When the signer isn't pinned, are the tiles before any failure still valid floors?
- A torn tile is a failed CRC. AT-17 (ignored, not quarantined, per EDGE "torn tiles are ignored") conflicts with AT-35 (failed CRC → quarantined).
**Fix:** Define the file-level label as a function of the per-tile outcomes. Explicitly separate torn or uncommitted tiles (ignored, no quarantine) from CRC failures on committed tiles (quarantine).

### F-09 — Major — DD-94 / §5.7 Fonts vs AT-42 (host test can't be built as written)
**Quote:** "The **host** binary embeds the same required UA font bytes with `include_bytes!`." AT-42: "With the required UA fonts removed from the test fixture, `render_manifest` returns a `KiteError` … This assertion runs against the **host** binary."
**Problem:** Fonts compiled into the binary with `include_bytes!` can't be "removed from the test fixture" at runtime, so the host half of AT-42 can't be set up without a special build. The spec doesn't define that build.
**Fix:** Route font registration through an injectable font set (the shipped binary passes the embedded bytes, the test passes an empty set). Or define a test-only cargo feature that compiles without the font bytes, and assert that `register_ua_fonts` with an empty set yields `KiteError`.

### F-10 — Major — §5.2.1 Write mutex / DD-84 / §5.5 LRS client (mutex covers network I/O; no LRS timeout; revoke delayed)
**Quote:** "serialized by one process-wide mutex, held from the start of the request through index and archive step 1, snapshot rename, download delivery, and any LRS send begun by that request."
**Problem:**
- (a) The mutex is held across the source fetch (up to `--fetch-timeout-secs`; it isn't said whether that covers all redirect hops) and across the LRS POST. The LRS POST has **no timeout at all**, so one stalled LRS connection blocks every select, download, skip and consent call indefinitely.
- (b) A `POST /v1/consent {off}` sent during an in-flight select has to wait for the lock. That select's step 3 then sends a statement after the researcher has asked to revoke. This is a privacy gap against "consent-only xAPI".
- (c) Skip "writes nothing" but still takes the lock.
**Fix:**
- Add a required `--lrs-timeout-secs`, or reuse the fetch timeout explicitly.
- State that the fetch timeout bounds the whole redirect chain.
- Have consent-off take effect immediately: set an atomic flag before acquiring the mutex, and make step 3 re-check it right before sending.
- Drop skip from the mutex, or justify keeping it.
- Add an AT: revoke during a slow fetch → zero LRS requests.

### F-11 — Major — §5.6 / DD-27 / DD-62 (content-addressed snapshot names collide across identities)
**Quote:** "Snapshots go in `--snapshot-dir` as `{lowercase hex SHA-256 of the captured bytes}.s3a`" … "published with an atomic rename." The PROVENANCE document holds `url=` and `captured=`.
**Problem:** Two identities, or two `source_url`s, that return identical bytes map to the same snapshot filename but different provenance documents. The second rename silently replaces the first identity's sealed snapshot, which has different provenance. AT-32 for the first identity then fails, and an earlier idempotent `snapshot` path now points at someone else's provenance. Nothing says what happens when the target already exists.
**Fix:** Name snapshots by the SHA-256 of the snapshot file, or by (content hash + identity). Or specify "if `{sha}.s3a` exists, don't overwrite; reuse it" and accept a single provenance. Either way, state it and add an AT.

### F-12 — Major — §5.5 Inclusion index (integrity and recovery unspecified)
**Quote:** "a later successful step 2 updates that field (rewrite of that one line, still under the write mutex)." / "If the index file is missing at startup, the host creates an empty one."
**Problem:**
- An in-place rewrite of one JSONL line isn't atomic. A crash can truncate or corrupt the whole index, and the method (temp + rename of the whole file?) isn't given.
- Startup behaviour for a **malformed** index (partial last line, bad JSON) isn't specified: error, skip, or truncate.
- A crash between the tile-10 write and the index append in step 1 isn't addressed, and neither is which error comes back.
**Fix:**
- Specify rewrite by temp file + fsync + atomic rename, or make the index append-only (a new line with the snapshot hash supersedes the old one).
- Define malformed-index handling (draft: startup error naming the file, no invented recovery).
- State the step-1 ordering: tile-10, then index; if the index append fails, return `500 write_failed` with the record kept.

### F-13 — Major — §5.6 auto `ConsentRecord` / DD-28 / DD-53 / AT-79
**Quote:** "On `create`, kite also writes one active core `ConsentRecord` with `purpose = Research` and `scope = 0` (all), `granted_by` derived from `--researcher-id`, so that once any consent record exists, the owner's inclusion records remain explicitly granted." AT-79: "`withheld_count` does not hide them all."
**Problem:**
- `subject_id`, `expires_at`, the timestamps, and the derivation function for `granted_by` (hash_iri? CRC32C?) aren't specified.
- The §5.3 filter is keyed **per `subject_id`**, and §4.7's `HumanLrsRecord` field list has no `subject_id`. So inclusion records aren't withheld by `ConsentRecord`s anyway, and the stated rationale doesn't connect.
- AT-79's "does not hide them all" passes even if some inclusion records are hidden.
**Fix:**
- Specify every `ConsentRecord` field written on create.
- State whether tile-10 records carry a `subject_id`, and fix the rationale.
- Make AT-79 require `withheld_count == 0` for the owner's records.

### F-14 — Major — §4.2 consent paragraph vs §5.3 item 4 (withholding rule restated differently)
**Quote (§4.2):** "When at least one applicable consent record exists, kite withholds records that are not granted for the research purpose."
**Quote (§5.3 item 4):** "A payload record **without** a `subject_id` is not withheld by `ConsentRecord`; it is withheld only by marks."
**Problem:** §4.2 says every record not granted for research is withheld once any applicable record exists. §5.3 exempts records without a `subject_id`. An implementer following §4.2 fails AT-11's golden fixtures. §4.2 is labeled "grounded facts", but this sentence is kite behaviour.
**Fix:** Delete or align the kite-behaviour sentence in §4.2 and point to §5.3 item 4 as the only normative rule.

---

## Minor

### F-15 — Minor — §5.5 step text (unreachable branch)
**Quote:** "If the fetch fails … 2b is skipped unless an earlier snapshot already exists."
**Problem:** Per the source table and §5.5, a fetch happens only when no snapshot exists for the identity, so "earlier snapshot already exists" can't coincide with a failed fetch.
**Fix:** Delete the clause, or say what "earlier" means (a different identity with the same bytes? see F-11).

### F-16 — Minor — DD-09 vs §5.6 / AT-27
**Quote (DD-09):** "Snapshots and inclusion records of selected papers are local working-archive writes."
**Problem:** AT-27 says the snapshot is "not a tile in the working archive", and DD-27 sends snapshots to `--snapshot-dir`.
**Fix:** Reword as "local writes (working archive and `--snapshot-dir`)".

### F-17 — Minor — DD-53 cites retired DD-63
**Quote:** "Snapshot exemption is DD-63."
**Problem:** DD-63 is retired in v5, and §5.6 says no snapshot exemption exists.
**Fix:** Replace with "No snapshot exemption (DD-63 retired; DD-67 applies)."

### F-18 — Minor — §5.8 Animation / AT-62 (opacity curve and `t` range)
**Quote:** "Progress between those endpoints is the y-value of CSS `cubic-bezier(.2,.8,.2,1)`…"; AT-62 checks opacity "within 0.02" at t=0.225.
**Problem:** The spec doesn't say whether opacity follows the same eased progress (≈0.957 at mid) or a linear ramp (0.6), so the mid-point opacity check has no defined expected value. Behaviour for `t < 0` or `t > 0.45` (clamp or error) isn't specified. It's also unclear whether the end frame shows floor-detail content or an empty card.
**Fix:** State "opacity = 0.2 + 0.8 × progress" (matching CSS `transition: all`), clamp `t` to [0, 0.45] or return 400, and define the card contents.

### F-19 — Minor — AT-143 vs §5.2.1 `/v1/frame` response
**Quote (AT-143):** "(`width`/`height` in the JSON, `rgba` length 80000)."
**Problem:** The HTTP response has `png_base64`, not `rgba`. The assertion only makes sense on the FFI.
**Fix:** Split it: FFI asserts `rgba.len()==80000`; HTTP asserts the decoded PNG is 200×100.

### F-20 — Minor — Weak or ambiguous negative ATs (AT-132, AT-51, AT-52, AT-53)
**Quote (AT-132):** "any Scholar-only flag listed in §5.2.2 (`--data-dir`, `--wiki-root`, `--lrs-url`, or `--researcher-id`)".
**Problem:**
- AT-132 tests 4 of the 14 rejected flags.
- AT-51 (bind `0.0.0.0` with no TLS) omits `--headless`, so it can pass because headless is missing rather than because TLS is.
- AT-52 and AT-53 don't assert the catalogue code `409 path_escape`, which DD-83 says ATs must assert.
- AT-53 doesn't name the request field that carries the `file://` URL.
**Fix:** Parameterize AT-132 over all 14 flags. Add `--headless` to AT-51 and assert the error names TLS. Assert `409 path_escape` in AT-52 and AT-53, and name the field.

### F-21 — Minor — §5.2.2 (no complete flag inventory)
**Problem:** `--egress-allowlist`, `--tls-cert`/`--tls-key`, `--behind-proxy`, `--egress-test-loopback-port`, `--pinned-keys-file` and `--headless` are defined in different places. Their acceptance in viewer-only mode isn't stated (for example, is `--egress-allowlist` rejected in viewer-only mode like the other fetch flags?), and there's no single table of type, mode and validation per flag. That weakens AT-07, AT-77 and AT-132.
**Fix:** Add one flag table: name, type/range, required/optional/forbidden per mode, dependent flags.

### F-22 — Minor — `/v1/candidates` `title`
**Quote:** "`title`: `<from the file or record, or empty>`"
**Problem:** §2 and §4.7 say `AcademicPaperRecord` stores only hashes and has no string fields, so there's no title "from the record".
**Fix:** "From dossier YAML `title`; empty for `AcademicPaperRecord` candidates."

### F-23 — Minor — §5.1 `Link { href }` resolution base
**Quote:** "If `href` is a relative or absolute filesystem path that canonicalizes inside an allowed root, the UI may then call `POST /v1/open` with that path."
**Problem:** The base for a relative href (page directory? process CWD? root?) isn't specified, so wiki cross-links can't be tested. Fragment and query handling (`foo.md#sec`) aren't specified either.
**Fix:** Resolve against the opened page's parent directory, strip the fragment, and add an AT.

### F-24 — Minor — View validity matrix (`view_mismatch`)
**Problem:** The spec doesn't say which `view` values are valid per kind and state. For example: is `view=header` on a hyper-tile allowed, or `manifest` on Edge (AT-66 uses `manifest` on unsupported)? Is `floor-detail` or `floor-transition` allowed before `proceed`? What does `hit_test` return on `floor-detail`, `floor-transition` or `banner`? Without this, `409 view_mismatch` isn't testable.
**Fix:** Add a kind × state × view table with the outcome for each cell.

### F-25 — Minor — §5.5 candidate recomputation cost and walk policy
**Quote:** "every live (non-tombstone) `AcademicPaperRecord` in every regular `*.s3a` file found by a recursive walk of `--corpus-root`", recomputed on every candidates, select, download and skip request.
**Problem:** Every request does unbounded I/O (open and parse every archive). There's no symlink-following policy for the walk (loops, escapes) and no depth or file-count limit. A large corpus could make select slow while it holds the write mutex (see F-10).
**Fix:** State the symlink policy (draft: don't follow), and either allow a cache keyed on mtime or acknowledge the cost. Do membership checks before taking the mutex.

### F-26 — Minor — Transport hardening gaps
**Problem:** The spec doesn't specify any of these: Unix-socket file mode and owner, or stale socket handling at startup; minimum TLS version; a maximum request-body size for JSON routes; caps on query values like `floor` and `t`. These are gaps in a fail-closed security posture.
**Fix:** Socket created 0600 by euid; stale socket left in place → startup error; TLS ≥1.2; request bodies capped (a flag or a fixed contract constant, listed like DD-69).

### F-27 — Minor — §5.7 phone `open_path`
**Quote:** "v1 UniFFI exports `open`, `open_verified`, `open_path` …" while "SAF / share-sheet open paths already in the app stay the way files arrive … Bytes are passed into `open`."
**Problem:** `open_path` on the phone has no authorization root ("a path the caller has already authorized"), and the app is said to use bytes anyway. Exporting it widens the surface for no stated reason.
**Fix:** Drop `open_path` from the phone UniFFI surface, or state the app-side authorization rule.

### F-28 — Minor — Consent evaluation details
**Problem:**
- Edge: the rule for several type-71 records (latest wins? by what key?) isn't stated.
- Hyper-tile: "Latest-wins per `subject_id`" has no ordering key (a timestamp field or tile order).
- `now` in `is_active` uses an unspecified clock with no test injection, so AT-136 and expiry fixtures aren't deterministic.
**Fix:** Name the ordering key for both, and require an injectable clock in pack tests.

### F-29 — Minor — Preamble "no fallbacks" vs fallbacks in the spec
**Quote:** "No stubs, fallbacks, or invented runtime defaults, except the query defaults listed …"
**Problem:** §5.2.2 falls back from `$XDG_RUNTIME_DIR` to `/tmp`, and the host auto-creates a missing inclusion index. Both are reasonable but break the stated rule.
**Fix:** List them as explicit contract exceptions next to DD-69.

### F-30 — Minor — §5.2.3 consent GET/POST body without `--lrs-url`
**Quote:** "`GET /v1/consent` without `--lrs-url` returns `200` `{ "state": "off", "timestamp": null }`." `POST off` returns "the same body as GET after the host writes the consent file."
**Problem:** With `--consent-state-file` set and no `--lrs-url`, a POST `off` writes a timestamp, but GET/POST still report `timestamp: null`. The response contradicts the file.
**Fix:** Return the file's timestamp when a consent file is configured, or forbid `--consent-state-file` without `--lrs-url`.

### F-31 — Minor — AT-125 vs §5.2 / DD-60
**Quote (AT-125):** "A native-window build (not `--headless`) … the test passes the required TLS flags when the bind would need them."
**Problem:** A window is only allowed on loopback or Unix binds, which never need TLS, so that clause is dead. "Native-window build" suggests a compile-time feature, but §5.2 treats the window as a runtime choice (`--headless`).
**Fix:** Drop the TLS clause, and say whether the window is a cargo feature, a runtime option, or both.

### F-32 — Minor — AT-10 oracle scope
**Quote:** "the tour floor list matches `s3a-cli tour --json` on the same hyper-tile file."
**Problem:** §4.2 says the CLI's unsealed fallback is stride-blind and the type-82 branch is buggy. AT-10 doesn't limit its fixture to a sealed, type-82-free hyper-tile, which is the only case where the CLI is a valid oracle.
**Fix:** Restrict the AT-10 fixture explicitly, or use golden fixtures.

### F-33 — Minor — AT-96 / AT-06 omissions
**Quote (AT-96):** "The pack exports `open`, `open_path`, `render_manifest`, …"
**Problem:** AT-96 leaves out `open_verified`, a required export (§5.1, AT-99). AT-06's exception names only "loopback `GET /health`" and misses the Unix-socket tokenless case, so it conflicts with AT-113.
**Fix:** Add `open_verified` to AT-96, and use the defined term "tokenless health case" in AT-06.
