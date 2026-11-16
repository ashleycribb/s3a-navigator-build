# Round 18 disposition — s3a-kite SPEC v19

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round18.md` (copied to `reviews/goldfish_s3a-kite_round18.md`).
22 findings (0 blockers, 3 majors, 19 minors). Archived v18: `reviews/SPEC-v18.md`.

**Counts:** 22 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-143, and DD-148 are unchanged (byte-identical to v18). DD-111, DD-112, DD-121, DD-145, DD-146, DD-147, DD-149, and DD-150 keep their prior intent and are tightened (coverage and `break_index`; select/download run-to-completion; test-crate split; later-than-latest-trusted-grant wording; shared safe-open sites, unique path-component prefix, YAML aliases; v19 is not frozen). DD-16 and DD-69 are also tightened to the same rules. No new open questions. No invented numeric defaults (no default N for sparse Edge signing, no second size cap, no numeric YAML alias budget).

This v19 is not frozen (DD-149): round 18 still had majors.

## How majors were resolved

**F-01** (`kite-acceptance` as a workspace member that depends on the pack, or a feature-gated pack dep, makes `cargo metadata` / `cargo test` fail while the pack is absent). Option (b) split: `kite-acceptance` remains a s3a-storage workspace member **with no pack dependency**, so AT-22, AT-37, host-spawn tests, and cargo-tree/guards that do not link the pack run today. Pack-linking library tests live in `kite-pack-tests`, an **out-of-workspace** crate with its own `Cargo.toml` and lockfile and path dependencies; a compile failure of that crate is the documented fail-first result until the pack exists (no skeleton pack crate). s3a-reader tests that need the pack (AT-38, AT-39, AT-43, AT-63, AT-165) live in a separate crate **outside** the reader workspace, not as a feature-gated member. AT-183 lives in the host crate's own tests at `s3a_kite::egress::egress_allowlist_matches`. The workspace stays green with the pack absent: `cargo metadata`, `cargo test`, and other workspace-wide cargo commands resolve. Tightened DD-121.

**F-02** (a tile after a sparse signed `tile_index` could be treated as trusted, and `break_index` was not aligned with coverage). A tile is trusted only if it is a valid floor **and** is covered by a verified seal record. An authentic record at *s* covers `0 ..= s`. Any tile not covered by a verified record is untrusted. On a failed record, `break_index` for `chain_break` and `signature_known_signer` is (the highest preceding authentic `tile_index`) + 1, or 0 if there is none — the first tile after the last verified coverage. Edge writers MAY sign every N tiles (sparse records); N is writer policy and this spec invents no default N. Extended AT-220 with sparse (*s*, *t*, edit at *s*+1 → `break_index` *s*+1) and dense (per-tile) fixtures; restated AT-201 cases (2), (7), (8), (9) and AT-165. Tightened DD-111, DD-145, and DD-150.

**F-03** (select/download could leave the in-flight fetch-registry entry and lose the LRS statement if the client disconnected). Once the request has passed candidate membership, the handler runs steps 0–3 to completion even if the starter's client disconnects (same run-to-completion idea as revoke). The registry entry is always removed; waiters are woken with the starter's defined outcome (they do not hang). Step 1 completes, or the defined `500` `write_failed` with `included: true` is produced. Step 3 still runs so a consented LRS statement is not lost. Fail-closed exit remains: a gone process has no registry in a new process. New AT-221. Tightened DD-112.

## Minors

**F-04.** Fixed: any **later** denial, withdrawal, or expiry, later than the latest trusted grant, restricts; an earlier expiry or withdrawal does not withhold a later trusted grant (AT-185). Tightened §5.3, DD-111, and DD-147.

**F-05.** Fixed: `chain_break` is in the total outcome list; rules 3/4 map verify-but-`seal_hash`-mismatch to `chain_break` at the F-02 `break_index`. Tightened DD-145.

**F-06.** Fixed: `withheld_count` is painted as the exact substring `withheld_count: N` — per-file on the first view / banner, per-floor on `render_floor_detail`. AT-11 and AT-23 require that substring.

**F-07.** Fixed: `payload_hash_short` is the first 8 lowercase hex digits of the SHA-256 of the tile payload (Edge: excluding the micro-tile header). The CLI is an oracle only for a sealed type-82-free hyper-tile (AT-10). AT-15 and AT-21 compute that function over the fixture bytes and do not use the CLI as an oracle.

**F-08.** Fixed: the floor-to-detail start rectangle is the `floor` `SemanticNode` rect of a `warehouse` frame rendered with the same `width`, `height`, `scroll_x`, `scroll_y`, and `device_pixel_ratio`. Restated AT-13 and AT-62.

**F-09.** Fixed by extending existing ATs (no new numbers): AT-202 adds mid-copy growth past `--max-open-bytes` → `download_error: "source_changed"`; AT-88 adds a max-redirects-exceeded case; AT-189 adds `revoke-pending` as a directory or other non-regular file at startup; AT-140 covers `Content-Encoding: deflate`.

**F-10.** Fixed: a `Link` href strips fragment and query from the raw attribute, then percent-decodes the path, then resolves. A root-relative href resolves against the allowed request root of the opened page, not the filesystem root. AT-160 adds `a%3Fb.md` and `/papers/SRC-0001.md` cases.

**F-11.** Fixed: lexically normalize the absolute path, then pick the unique allowed root that is a path-component prefix; no match or more than one match is `path_escape`. Normalize-then-match so `/wiki/../corpus/x` can match corpus. A non-canonical symlink prefix of a stored canonical root is `path_escape`. `page_path` is already canonical. Tightened DD-146.

**F-12.** Fixed: startup leftover-temp cleanup and config/state files (`--pinned-keys-file`, `--lrs-credential-file`, `--consent-state-file`) are sites of the shared safe-open rule. Cleanup skips a FIFO or symlink. Config and state files must be regular, opened with `O_NOFOLLOW|O_NONBLOCK`, and bounded by `--max-open-bytes` (no second cap). Tightened DD-146.

**F-13.** Fixed: AT-46 and AT-187 require host B to use its own `--bind`/`--port` and either omit `--lrs-url` or use a distinct `--consent-state-file`.

**F-14.** Fixed: shared safe-open sites are "not blocked indefinitely (no hang on a FIFO or symlink)"; a request may still wait for the write mutex for a bounded copy. Restated AT-215 and AT-217.

**F-15.** Fixed: AT-183 lives in the host crate's own tests at `s3a_kite::egress::egress_allowlist_matches` (not in `kite-acceptance` and not in `kite-pack-tests`).

**F-16.** Fixed: §4.2 / §4.7 point at the §5.3 trusted-authentication condition; the glossary Edge archive is the flag **or** stride 4096/8192; fail-closed exit also increments `epoch` in §5.2.1; `--behind-proxy` health cites AT-113 / AT-87, and AT-50 is non-loopback TLS.

**F-17.** Fixed: `/v1/frame` query-field types and ranges are stated (`w`/`h` integers ≥ 0; `t`/`x`/`y`/`scroll_*` finite f64, scroll may be negative; `dpr` finite f64; `instant` is the `true`/`false` literals). A malformed unused field is `invalid_param`. A well-typed non-positive `dpr`, a cap overflow, or `w` or `h` 0 is `invalid_frame_size`. Restated AT-141 and AT-171. Tightened DD-69.

**F-18.** Fixed: the §5.2.6 matrix and codes apply to pack FFI `proceed_to_warehouse` and `render_manifest` / `render_page` / `render_warehouse` / `render_floor_transition` / `render_floor_detail` (`KiteError` `unsupported`, `view_mismatch`, or `not_archive`).

**F-19.** Fixed: the YAML parser rejects aliases and anchors; a dossier that uses them is unparseable and is omitted the same way. No numeric alias budget is invented. Extended AT-188. Tightened DD-146.

**F-20.** Fixed: AT-165 includes a hyper-tile fixture with a partial trailing tile, and a fixture with a tile index ≥ header `tile_count`. Those are not Quarantined solely for that torn or uncommitted tail; `break_index` is the first such tile; the indices are in `torn_tile_indices` and `invalid_tile_indices`. Tightened DD-150.

**F-21.** Fixed: the `ADDON_EDU` check runs in the step-0/1 hold on every select/download, whether or not step 1 is skipped (not at startup). Extended AT-46. Tightened DD-16.

**F-22.** Fixed: AT-171's chmod-000 case requires a non-root runner, or is skipped and the skip is reported if the process is running as root. AT-24, AT-25, and AT-29 fixtures contain no omitted or unparseable dossier, so `--log-dir` byte-identity holds (candidate walks write identifier-free skip messages only when a skip actually occurs). AT-125 skips the window half if the host has no `DISPLAY`, `WAYLAND_DISPLAY`, or equivalent, and reports the skip.

## AT audit (v18 → v19)

- **Defined range:** AT-01–AT-221 (221 tests, no gaps).
- **Changed (26):** AT-11, AT-13, AT-15, AT-21, AT-23, AT-24, AT-25, AT-29, AT-46, AT-62, AT-88, AT-125, AT-140, AT-141, AT-160, AT-165, AT-171, AT-183, AT-187, AT-188, AT-189, AT-201, AT-202, AT-215, AT-217, AT-220.
- **Added (1):** AT-221 (select/download runs to completion after membership when the starter's client disconnects; registry removed; waiters complete; step 1 and step 3 still run).
- **Removed:** none.

## New and tightened decisions / questions

- No new DDs or Qs.
- Tightened: DD-16, DD-69, DD-111, DD-112, DD-121, DD-145, DD-146, DD-147, DD-149, DD-150.
