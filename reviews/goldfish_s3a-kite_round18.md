# Goldfish review: s3a-kite spec v18 (round 18)

Reviewer: an independent, fresh reviewer. I read only `/workspace/goldfish/s3a-kite.md` (v18, 1383 lines). I did not read prior reports, dispositions, the repo, or the web.

**Summary: 0 blockers / 3 majors / 19 minors.**

| ID | Severity | Area |
| --- | --- | --- |
| F-01 | Major | DD-121 test layout breaks every cargo command in the workspace before the pack exists |
| F-02 | Major | Edge break localization: tiles with no signature of their own, before the first failing record, stay trusted |
| F-03 | Major | A select/download whose client disconnects can leave an in-flight registry entry behind for good (hang), lose a consented LRS statement, or half-finish step 1 |
| F-04 – F-22 | Minor | wording, oracles, untested edges, missing codes |

---

## Majors

### F-01 (Major): `kite-acceptance` excluded from `default-members` does not keep the workspace green; the guard ATs in it cannot run today

**Refs:** §5.1 "Test locations and pins (DD-121)"; §7 preamble; DD-121; §11 step 1; AT-01, AT-22, AT-37, AT-42, AT-95, AT-96, AT-161, AT-183, AT-205, AT-211; reader tests AT-38/39/43/63/165.

**Problem.** §5.1 puts every pack AT, including the library-level ones (AT-42 `KiteContext::new`, AT-95/96 exports, AT-161/205 `new_with_clock`, AT-211 `open_verified`, AT-183 matcher), in a `kite-acceptance` crate. That crate is a member of the s3a-storage workspace but is left out of `default-members`. The spec then says that "`cargo test` at the workspace root does not build that crate, so the workspace stays green before `s3a-pack-kite` exists." Cargo does not work that way. It loads and resolves the dependency graph of **every** workspace member, for `Cargo.lock` and `cargo metadata`, whatever `default-members` says. A member with a path (or git) dependency on a crate that does not exist makes every cargo command in the workspace fail with "failed to read …/s3a-pack-kite/Cargo.toml", and that includes a plain `cargo test` at the root. Optional (feature-gated) dependencies still have to resolve. So the s3a-reader option "feature-gated … until the reader pin moves" fails the same way. Only the "separate test crate outside the workspace" option works there.

There is a second problem. AT-22 and AT-37 are guards that "already pass today", but they sit in a crate that cannot compile until the pack exists. The §7 preamble also contradicts §5.1: it says "Pack unit and library tests live in the s3a-storage workspace", which is not the same as all pack ATs living in `kite-acceptance`.

**Why it matters.** §11 step 1 (land fail-first tests before the pack) cannot be done without breaking the storage workspace. The obvious workaround, a skeleton `s3a-pack-kite` crate, is a stub, which the spec forbids, and it would also make AT-01 pass vacuously.

**Fix.** Pick one approach and state it:
- (a) Put `kite-acceptance` in `[workspace] exclude`, with its own `Cargo.lock` and a path dependency into the storage tree. CI builds it explicitly. "Fail first" then means a compile failure of that crate, documented as the expected failure.
- (b) Split it in two. Host-spawn and `cargo tree` ATs go in a member crate with **no** dependency on the pack. Library ATs go in an excluded crate (or into `s3a-pack-kite`'s own tests, created in the same change as the pack).

Move AT-22 and AT-37 to a crate that builds today. For s3a-reader, delete "feature-gated" and keep only "separate test crate outside the reader workspace". Make the §7 preamble agree with §5.1.

### F-02 (Major): seal-hash mismatches and signature failures are localized to the record's `tile_index`, so tampered tiles before that index stay trusted when they have no record of their own

**Refs:** §5.4 item 2 (DD-145 rule 2; chain-break, chain-inactive mismatch, and `signature_known_signer` rows of the DD-150 table; unsigned-tail rule); §4.7 (`edge_seal_chain`); DD-150; AT-201 (8)/(9); AT-220; AT-194; AT-210.

**Problem.** The Edge chain is cumulative. `compute_seal_hash` at tile *t* depends on every tile ≤ *t*. A mismatch can only be **observed** at a tile that has a `DeviceSealRecord`: "stored `seal_hash` ≠ recomputed … at that `tile_index`". The spec sets `break_index` to "the first mismatch tile", and to "that `tile_index`" for `signature_known_signer`. Valid floors are every index below `break_index`, and they stay authenticated for consent (AT-194 depends on this).

The spec never requires one record per tile. It explicitly expects tiles that have no record of their own: the unsigned tail beyond the highest authentic `tile_index`, and container tiles that "may sit after the `tile_index` [they cover]". It also never says how to localize a break inside a gap between records.

Consider a file whose authentic records sit at tiles *s* and *t* (*s* < *t*), and an attacker who edits tile *s+1* (recomputing its CRC) to plant a type-71 grant. The recomputed hash first mismatches at *t*, so `break_index = t`. Tile *s+1* is then a valid, **trusted** floor, and the planted grant releases event-log payloads. The chain-inactive row's note ("Earlier tiles whose hashes matched and whose records are authentic may remain valid floors") points toward the safe reading. The normative `break_index` / valid-floor rule implements the unsafe one.

**Why it matters.** This is a tamper path that releases withheld research data while following the spec. It is the threat the spec defends against in AT-178, AT-194 and AT-220. It would be a Blocker if the EDGE profile allows sparse records (an MCU that signs every *N* tiles, for example). The spec does not settle that, so I rate it Major.

**Fix.** Pick one rule and state it:
- (a) Define the break position for `chain_break` and `signature_known_signer` as **(highest `tile_index` of an authentic record that precedes the failing record) + 1**, or 0 if there is none. Tiles not covered by an authentic cumulative record are never valid floors.
- (b) Require one `DeviceSealRecord` per committed non-container tile. Treat a data tile with no own record, before the highest authentic `tile_index`, as a seal-hash mismatch at that tile.

Add both the sparse and the per-tile fixtures to AT-220 and AT-165, and update the reader-core `break_index` mapping row.

### F-03 (Major): a select/download handler cancelled by a client disconnect can strand the in-flight registry entry, lose a consented LRS statement, or leave step 1 half done

**Refs:** §5.2.1 write mutex and in-flight registry; §5.5 numbered steps 0–3; DD-112; DD-120; AT-126, AT-212–AT-214. Compare §5.2.1 revoke: "the handler is not cancelled by a dropped connection" (AT-198).

**Problem.** The registry entry is removed only "in the same mutex hold as the snapshot publish and index append, or as the fetch-failure, `snapshot_conflict`, or … `write_failed` outcome". Waiters "never start a GET in the same request" and wait for the starter's outcome. v18 adds a run-to-completion rule for revoke only. Common Rust HTTP stacks (hyper/axum) drop the handler future when the client disconnects. A starter cancelled during the GET never reaches any of the listed removal holds, so:
- (1) every current waiter hangs;
- (2) every later select/download of that `(file_uuid, identity)` finds the entry "running" and waits forever, until the host restarts;
- (3) a starter cancelled after step 1 but before step 3 never sends the consented statement, and later requests skip step 3 because step 0 matches, so the statement is lost for good;
- (4) a starter cancelled between the tile-10 write and the index append leaves the AT-158 state without the `500` response.

**Why it matters.** This is a stuck state that a correct-by-the-letter implementation will hit. Nothing in the spec forbids cancelling these handlers, and no AT covers it.

**Fix.** Add a normative rule: "once a select/download request has passed candidate membership, its handler runs to completion (steps 0–3, registry removal included) even if the client disconnects". Alternatively, require an RAII guard that removes the registry entry and wakes waiters with a `fetch_error: "aborted"` outcome on drop, and say what happens to step 3 then. Add an AT: the starter's client closes during a slow GET; a concurrent waiter and a later select both complete (no hang); with consent on, exactly one statement is sent or queued.

---

## Minors

### F-04 (Minor): "Any denial, withdrawal, or expiry from any parseable tile restricts" is broader than clause 2 and AT-185

**Refs:** §5.3 item 4, the per-subject decision and the sentence after it; DD-147; DD-111; AT-185 (unsealed variant); AT-136.

Clause 2 restricts only on a **later** denial, withdrawal, or expiry. The next sentence ("Any denial, withdrawal, or expiry from any parseable tile therefore restricts that subject") and DD-111/DD-147 drop "later". Read literally, a subject who re-consents (earlier expiry or withdrawal, later trusted grant) is withheld forever. The AT-185 unsealed variant expects the opposite. **Fix:** say "any **later** denial … from any parseable tile (valid or invalid) restricts" in all three places. Also name the anchor: later than the latest trusted grant.

### F-05 (Minor): the DD-145 "total" outcome list leaves out `chain_break`

**Refs:** §5.4 "Signer attribution" lead-in and rule 2; DD-145.

The algorithm is claimed to map every record to "exactly one of authentic, `signature_known_signer`, `signer_not_pinned`, or `missing_signature`". Rule 2 then assigns a fifth outcome, `chain_break`, to a record that verifies but whose hash mismatches. Rules 3 and 4 never handle "verifies under a candidate key but the hash mismatches", including when it is the first record. **Fix:** add `chain_break` to the list, and add a rule-3/4 branch for verify-but-mismatch (→ `chain_break` at the F-02 position).

### F-06 (Minor): the scope and wire format of `withheld_count` are undefined

**Refs:** §5.1 `render_floor_detail`; §5.3 item 4 counting rule; AT-79, AT-134, AT-173, AT-184, AT-195, AT-204, AT-205.

It is not stated whether `withheld_count` is per floor (floor-detail, as in `tour --floor N`) or per file (header/banner), or how it appears in `text_runs` (label and number format). AT-184 and AT-204 ("counts X and Y") only pass under the per-file reading, or if the fixture puts X and Y on one floor. **Fix:** define a per-floor value on floor-detail and a per-file value on the first view or banner. Give an exact text-layer form (for example `withheld_count: N`) or a `SemanticNode` label. Tie each AT to one of the two.

### F-07 (Minor): `payload_hash_short` is undefined for Edge, so AT-15 and AT-21 have no oracle

**Refs:** AT-10 (CLI oracle, hyper-tile only); AT-15; AT-21; §4.2 (CLI is stride-blind).

For Edge the CLI is explicitly not an oracle, and the spec never gives the hash algorithm, the prefix length, or the input. AT-15 says "tile bytes" while hyper-tile uses "payload". **Fix:** define it, for example "the first 8 lowercase hex digits of SHA-256 over the tile payload (excluding the micro-tile header)", matching `tour.rs`. Restate AT-15 and AT-21 in those terms.

### F-08 (Minor): AT-62 and AT-13 start rectangle has no oracle, because warehouse bay positions are not specified

**Refs:** §5.8 Scale/Animation; AT-62; AT-13.

The spec fixes bay sides but not grid origin, gaps, or wrapping. "Within 1 CSS px of the §5.8 start rectangle" is therefore not computable from the spec. **Fix:** define the start rectangle as the `floor` `SemanticNode` rect of a `warehouse` frame with the same `w`, `h`, `scroll_x`, `scroll_y`, and `device_pixel_ratio`, and say AT-62 derives it that way.

### F-09 (Minor): normative rules with no covering AT

- `download_error: "source_changed"` (file grows past the cap mid-copy). The catalogue cites AT-202, but AT-202 tests symlink and FIFO replacement only.
- `--max-redirects` exceeded → `fetch_error`, no snapshot (§5.2.4). No AT.
- `revoke-pending` present as a directory or other non-regular file → startup error that names the path (§5.2.1 step 1). AT-189 deliberately avoids this.
- `deflate` decoding (AT-140 covers gzip only).

**Fix:** add an AT or an AT clause for each.

### F-10 (Minor): `Link` href order of operations and root-relative base

**Refs:** §5.1 `Link { href }`; AT-160.

"Percent-decoded before path resolution" does not say whether decoding happens before or after stripping `?query` / `#fragment`. Decoding first turns `a%3Fb.md` into `a?b.md`, which then loses everything after `a`. It is also not said what a root-relative `/x.md` resolves against: the filesystem `/`, the page's request root, or the page's parent. **Fix:** "strip the fragment and query from the raw attribute, then percent-decode the path, then resolve". Also "a root-relative href resolves against the allowed request root of the opened page" (or the filesystem root). Add one AT case for each.

### F-11 (Minor): picking the request root for a `/v1/open` path is underspecified

**Refs:** §5.2.2 path rule; DD-146; AT-52; AT-53.

The host "lexically normalizes the path relative to the allowed root", but the spec does not say which root, or how, when:
- (a) the client sends a non-canonical prefix of a root. Roots are stored canonicalized, so a flag value through a symlinked directory, or `/tmp` vs `/private/tmp`, would turn a legitimate path into `path_escape`;
- (b) a `..` crosses from one root into another (`/wiki/../corpus/x`): normalize then match, or match then reject.

**Fix:** "Normalize the absolute path lexically, then pick the unique root that is a path-component prefix of the normalized path. No match → `path_escape`." Say whether clients must use canonical prefixes (`page_path` already is canonical).

### F-12 (Minor): safe-open gaps at startup cleanup and on config/state reads

**Refs:** §5.2.2 shared safe-open rule (closed list of sites); §5.6 DD-122 startup cleanup; token, credentials, pinned-keys, consent, and index loads.

The DD-146 site list is closed. It leaves out the startup open and `flock` of stale `s3a-kite-tmp-*` names in `--snapshot-dir` and `--download-dir`, and AT-215/216 treat those directories as places files can be planted. A planted FIFO (or a symlink to one) with that name hangs startup in `open`. A symlink makes the host lock and inspect its target. The token, LRS credentials, pinned-keys and consent files, and `inclusion-index.jsonl`, also have no regular-file / `O_NONBLOCK` / size rule (a FIFO passes the owner+0600 check). **Fix:** add "startup temp cleanup: `lstat`/`openat(O_NOFOLLOW|O_NONBLOCK)`, regular files only, else skip". Also require config and state files to be regular files, opened with `O_NONBLOCK`, with a stated size cap (or "bounded by `--max-open-bytes`").

### F-13 (Minor): AT-46 and AT-187 second hosts also need a distinct consent file

**Refs:** AT-46; AT-187; DD-114; AT-208.

Both tests list only four distinct directories for host B. If the harness copies host A's `--lrs-url` / `--consent-state-file`, B fails at the `--consent-state-file.lock` sidecar (AT-208) and AT-46's "Host B starts" cannot hold. **Fix:** say that B either omits `--lrs-url` or uses a distinct `--consent-state-file` (and should also use its own bind/port).

### F-14 (Minor): "a concurrent `GET /v1/consent` is not blocked" overstates what the mutex allows

**Refs:** §5.2.2 site bullets; AT-215; AT-217; step 0 (mutex).

The working-archive copy and the step-0 DD-103 reuse checks (`verify_sealed_archive` plus BLOB reassembly and hashing, up to `--max-open-bytes`) run under the process-wide mutex. Consent GET/POST takes that same mutex, so it **is** blocked for that bounded time. **Fix:** say "not blocked indefinitely (no hang on a FIFO or symlink)", or move the reuse checks out of the mutex.

### F-15 (Minor): AT-183's `egress_allowlist_matches` has no specified home

AT-183 is a unit test of a named function, and it lives in `kite-acceptance` (§5.1). The function belongs to the host binary, and §5.1 names no host library crate or module that exports it. **Fix:** name the crate and path (for example `s3a-pack-kite::egress::egress_allowlist_matches`, or a `s3a-kite` lib target), or move AT-183's unit half into the host crate's own tests.

### F-16 (Minor): leftover contradictions with the normative rules

- §4.2 and §4.7 say kite "follows" the no-records-unrestricted rule. §5.3 row 3 default-withholds an unauthenticated-as-a-whole file with zero records.
- The glossary "Edge archive" requires `FLAG_PROFILE_EDGE`, but detection (§4.3, §5.4 item 1) is flag **or** stride 4096/8192. That leaves a flagless 4096-stride file ambiguous.
- DD-144 says fail-closed exit also increments `epoch`; §5.2.1 does not.
- §5.2.1 cites AT-50 for `--behind-proxy` health (AT-50 is a TLS bind).

**Fix:** align the wording, and point §4.x to §5.3 explicitly ("subject to the trusted-authentication condition").

### F-17 (Minor): frame query field types and malformed unused fields

**Refs:** §5.2.1 frame defaults; catalogue rows for `invalid_param` and `invalid_frame_size`; AT-141; AT-171.

Unspecified:
- Whether a non-numeric `device_pixel_ratio` is `invalid_param` (param phase) or `invalid_frame_size` (route tail).
- The types and ranges of `scroll_x` / `scroll_y` (integer or decimal; negative allowed?).
- The accepted `instant` literals.
- Whether a **malformed** field that the view ignores (`t=abc` on `view=warehouse`) is ignored or `invalid_param`.

**Fix:** give a type and range per query field and one rule for malformed-but-unused fields.

### F-18 (Minor): FFI error codes for wrong-state calls

**Refs:** §5.1 table; §5.2.6.

HTTP defines `409` `unsupported` for proceed on an unsupported handle and `view_mismatch` / `not_archive` for frames. The FFI defines only `not_archive` for `proceed_to_warehouse` on a page. The `KiteError` codes for `proceed_to_warehouse` on `unsupported`, and for `render_warehouse` / `render_floor_*` / `render_page` in the wrong kind or state, are not stated. **Fix:** say the §5.2.6 matrix applies to the `render_*` and `proceed_to_warehouse` FFI with the same codes.

### F-19 (Minor): dossier YAML parsing can expand without bound

**Refs:** §5.5 item 1; `paper_identity`; AT-188.

Front matter is bounded by `--max-open-bytes`, but YAML anchors and aliases can expand exponentially (a "billion laughs" file). Every request-time candidate walk (no cache, DD-143) would re-trigger it. **Fix:** require a YAML parser setting that rejects or limits aliases (or a flat key/value subset), and treat a violation as "unparseable" (silent omit, as in AT-188).

### F-20 (Minor): hyper-tile torn or uncommitted tail has no AT

**Refs:** §5.4 per-tile outcomes ("a crash during a working-archive append can leave that tail"); DD-150 row "Hyper-tile: torn or uncommitted trailing tile only".

AT-17 and AT-165 cover Edge torn tiles only. **Fix:** add a hyper-tile fixture with a partial trailing tile and with an index ≥ `tile_count`: not Quarantined, `break_index` at the first torn tile, listed in `torn_tile_indices`, and `status` / `seal_status` taken from the other rows.

### F-21 (Minor): `addon_missing` when step 0 skips step 1

**Refs:** catalogue `addon_missing` ("checked at each step 1"; "evaluated after `unknown_paper` and before any inclusion or snapshot write"); DD-16.

If an index line matches, step 1 is skipped. It is then ambiguous whether the `ADDON_EDU` check still runs before step 2 / 2b (snapshot and download writes) on an archive whose bit has since been cleared. **Fix:** "the `ADDON_EDU` check runs in the step-0/1 hold on every select/download, whether or not step 1 is skipped".

### F-22 (Minor): AT environment assumptions

- AT-171's `403` `unreadable` via chmod-000 passes trivially when CI runs as root. Require a non-root runner, or skip and report.
- AT-24, AT-25 and AT-29 require `--log-dir` to be byte-identical. If a fixture contains an omitted or bad dossier, an implementation that logs an identifier-free "skipped dossier" error (allowed: "error messages that obey the no-identifier rule") fails them. Say that candidate walks do not log, or keep the fixtures clean.
- AT-125's display check names only `DISPLAY` (Wayland-only hosts).
