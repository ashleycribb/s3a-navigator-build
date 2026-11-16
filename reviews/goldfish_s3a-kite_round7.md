# Goldfish review — s3a-kite v7 (round 7)

Reviewer: fresh, independent; read only `/workspace/goldfish/s3a-kite.md`.
Date: 2026-11-16 (America/New_York).

**Summary: 1 blocker / 5 major / 16 minor (22 findings).** This is not a clean round.

---

## Blockers

### F-01 — Consent records from invalid or untrusted tiles can *grant*, and the Edge rule contradicts DD-111 (blocker)
- **Where:** §5.3 item 4 ("Consent and mark records are still collected from a tile that is parseable but invalid as a floor … a withdrawal in such a tile still withholds (DD-111, F-21)"); §5.4 item 2 per-tile bullets ("consent and mark records are still collected from parseable invalid tiles (F-21)"); §5.4 item 6 ("If the file has **no** CRC-valid type-71 record … unrestricted … If several CRC-valid type-71 records exist … the **last** one wins"); DD-111; AT-35; AT-23; AT-85.
- **Problem:**
  1. **Hyper-tile:** the stated reason is that withdrawals in invalid tiles still count. But the text collects *all* records from those tiles into the latest-wins map, so a later **grant** (`is_active` true) in a CRC-failed tile overwrites an earlier valid withdrawal for the same `subject_id`. Here is the attack: valid tile 3 withdraws subject X; a tampered, CRC-failed tile 5 holds an active grant for X. X's payload records in valid floors 0–4 are then **released**. A tampered tile can override consent, which is a privacy hole.
  2. **Edge:** §5.4 item 2 and DD-111 say consent records are collected from parseable invalid tiles. §5.4 item 6 counts only **CRC-valid** type-71 records. So a CRC-failed type-71 withdrawal is ignored, and the file can fall back to "unrestricted." That is the opposite of F-21. Separately, item 6 counts a CRC-valid type-71 record that sits *after* a seal-chain break, or in a file Quarantined because its signer is not pinned. That record is unauthenticated, yet as "last wins" it can grant research purpose over an earlier, authenticated denial.
- **Fix:** add one rule, stated in §5.3 item 4, §5.4 item 6, and DD-111: *records from invalid or unauthenticated tiles can only restrict, never release.* Pick one way to apply it:
  - (a) Two-pass rule. Build the latest-wins map from **valid-floor** records only. Then apply any withdrawal or denial found in parseable invalid tiles (or tiles after a break or unpinned signature) as an extra restriction. A grant in an invalid tile is ignored.
  - (b) Fail closed. If any consent or mark record sits in an invalid tile, withhold every subject-bearing payload record in the file (Edge: withhold all event-log payloads).
  
  Either way, make §5.4 item 6 say the same thing as DD-111. Add ATs: a hyper-tile with a valid withdrawal plus a later CRC-failed grant (still withheld); an Edge file with a CRC-failed denial (withheld, not unrestricted); an Edge file with a post-break grant after a valid denial (withheld).

---

## Majors

### F-02 — HTTP `t` must be an integer, but the animation needs fractional `t` and clamps out-of-range values (major)
- **Where:** §5.2.1 error catalogue, `invalid_param` ("non-integer or negative `floor`, `w`, `h`, or `t`"); §5.8 Animation ("`t` is clamped to `[0, 0.45]`; values outside that range are not an error"; samples t = 0.225, 0.45); AT-62; AT-13; §5.2.1 `/v1/frame` ("`t` is required for `floor-transition` when `instant` is false").
- **Problem:** Read literally, the catalogue makes every useful HTTP floor-transition request a `400 invalid_param`: `t=0.225`, even the end frame `t=0.45`. Only `t=0` is usable. It also makes negative `t` an error, while §5.8 says values outside the range are clamped, not errors. A host built to the catalogue cannot animate, and the two normative texts cannot both be met.
- **Fix:** In the catalogue, say `t` is a finite decimal number. Non-numeric or non-finite `t` is `invalid_param`; finite out-of-range values are clamped (§5.8). Drop `t` from the "non-integer or negative" list. Optionally extend AT-171 or AT-62 with HTTP `t=0.225` → 200 and `t=-1` → 200 (clamped).

### F-03 — Secrets and state files can sit inside a request root and be painted via `/v1/open` plus a name hint (major)
- **Where:** §5.2.2 Path rule / DD-89 ("Those paths are not required to sit inside a request root"); §5.2.5 (the name hint picks `wiki` for `.md`); `/v1/open` `name` field; §5.2.2 disjointness / DD-93 (write roots vs read roots only); AT-145.
- **Problem:**
  - Nothing forbids `--lrs-credentials-file`, `--tls-key`, `--auth-token-file`, `--pinned-keys-file`, or `--consent-state-file` from living under `--wiki-root`, `--corpus-root`, `--view-root`, `--snapshot-dir`, or `--download-dir`. Any UI-token holder can then `POST /v1/open {"path": "<creds>", "name": "x.md"}`, and the text layer shows the LRS password or TLS private key. On a `0.0.0.0` bind that holder is a remote client.
  - DD-93 checks write roots only against *read* roots. So `--data-dir` (inclusion index with paper identities, `revoke-pending`) or `--log-dir` may equal `--snapshot-dir` or `--download-dir`, which are request roots in Scholar mode.
  - In viewer mode it is unclear whether `--log-dir` must be disjoint from `--view-root`; AT-145 tests Scholar start only.
- **Fix:**
  - (a) Preferred. At startup, refuse any secret or state file (token, TLS key, LRS credentials, pinned keys, consent file, `--data-dir`, `--log-dir`) whose canonical path is inside any request root. Name both flags. Also require `--data-dir` and `--log-dir` to be disjoint from `--snapshot-dir` and `--download-dir`, and apply the rule in viewer mode (`--log-dir` vs `--view-root`). Add an AT.
  - (b) At `/v1/open`, refuse any canonical path equal to a host config or state file, and any path under `--data-dir` or `--log-dir`, with `409 path_escape`. Add an AT.

### F-04 — Snapshot reuse trusts whatever file already sits at `{sha}.s3a` (major)
- **Where:** §5.6 Target / DD-103 ("If that path already exists, the host **does not overwrite it**: the existing file is reused"); §5.6 Download names (an existing download file **is** hash-checked: `download_conflict`); AT-108 (a third-party archive named like a SHA-256 sitting in `--snapshot-dir`); AT-83 (a later download reassembles from the snapshot's BLOB tiles); AT-157.
- **Problem:** Downloads verify an existing file against its name; snapshots do not. A planted file, a stale or partial file from another tool, or the AT-108 third-party archive at `{sha}.s3a` is silently adopted. Its `snapshot_sha256` goes into the index, and a later download "reassembles" bytes from it. That blob may be missing, or may hash to something else. The download then lands under a different `{sha}` name, or the reassembly fails, and neither case has a defined error. The provenance shown for the paper may also belong to some unrelated file.
- **Fix:** Before reuse, open the existing snapshot, run `verify_sealed_archive`, and check that it holds BLOB chunks with `blob_key == sha` that reassemble to bytes hashing to `sha`. If any check fails:
  - (a) treat it as a conflict: return `fetch_error` (or a new `snapshot_conflict`), keep the inclusion record, do not overwrite; or
  - (b) move it aside and write a fresh snapshot.
  
  Also define the error when download reassembly from a snapshot fails (for example `download_error: "snapshot_invalid"`). Add an AT with a planted, non-matching `{sha}.s3a`.

### F-05 — Which lock guards the working archive is unclear; AT-73 conflicts with DD-114 / AT-164 (major)
- **Where:** §5.2.1 Write mutex ("The working-archive exclusive lock used for the tile-10 write (step 1) is the same process-lifetime lock taken at startup (DD-114)"); §5.2.1 Process exclusivity (`flock` on sidecar `--working-archive.lock` and `--data-dir/.lock`); §5.6 ("`TileWriter::create` / `open_append` use the process-lifetime exclusive locks …"); §4.4 (engine `try_lock_exclusive`); error catalogue `lock_failed` (AT-73, AT-89); AT-73 ("a second `TileWriter` / host that cannot take `try_lock_exclusive` on the working archive fails the write"); AT-164.
- **Problem:**
  - The process lock is on a **sidecar** file. The archive may not exist at startup, so it cannot be the archive's own lock. But the engine's `TileWriter` uses `try_lock_exclusive`, presumably on the archive file itself, and other S3A tools know nothing about the sidecar. The spec says these are "the same lock," which cannot be true.
  - So it is undefined whether the host also takes `try_lock_exclusive` on the archive for each write. It is also undefined what happens when an external process (s3a-cli, a Scholar tool) holds the archive lock.
  - AT-73's "second … host" case cannot happen: AT-164 makes a second host on the same archive a startup error, so `500 lock_failed` from a second host is untestable.
  - `/v1/open` of the working archive takes "a short shared read under the lock". An intra-process flock does not exclude the host's own concurrent tile-10 append, so a torn read is possible. It should be taken under the write mutex.
- **Fix:** Name both locks explicitly:
  1. The process-lifetime sidecar `flock`s (host vs host; startup error, AT-164).
  2. The engine `try_lock_exclusive` on the archive file, taken per write or held after the archive is created.
  
  State that an external holder of (2) produces `500 lock_failed` at step 1. Rewrite AT-73 to use an external `TileWriter` (not a second host). Say that the `/v1/open` working-archive copy is taken under the process write mutex.

### F-06 — AT-165 cannot run where §5.1 puts it, and the reader-core → `KiteVerificationReport` mapping is undefined (major)
- **Where:** AT-165; DD-115; §5.1 Test locations ("Pack tests (… AT-64–AT-174 except the reader-only tests below) live in a `kite-acceptance` crate in the s3a-storage workspace"; reader-only are AT-38/39/43/63); §5.1 / §5.7 ("The pack does not depend on `s3a-reader-core`"; "the app maps a post-verify reader-core `VerificationReport`"); §4.7 (reader-core has `VerificationStatus` and `SignerTrust`; no `seal_status` or invalid-tile list is described).
- **Problem:**
  - AT-165 needs pack `open` and a **mapped reader-core `verify`** in the same test. The listed location is `kite-acceptance` in s3a-storage. s3a-reader depends on s3a-storage (pinned at `ff89564`), so this is either a reverse or circular dependency, or a dependency on an old engine.
  - The mapping is never specified. The spec does not say how reader-core yields `seal_status`, `tiles_crc_failed`, or `invalid_tile_indices`, or whether torn tiles (MICRO-VEC-02) count as "invalid". The equality being asserted is therefore not defined.
- **Fix:**
  - (a) Move AT-165 to s3a-reader, feature-gated like AT-38/39/43/63. Add it to the reader-only list in §5.1 and §7. Specify the mapping (a field-by-field table, including whether torn tiles go in `invalid_tile_indices`).
  - (b) Or have the pack define the mapping function, `KiteVerificationReport::from_parts(status, signer_trust, …)`, with a pure table test in the pack, and keep the cross-repo equality as a reader-side test.

---

## Minors

### F-07 — The FFI `hit_test` signature has no `floor`, yet the cache is keyed by `(view, floor)` (minor)
- **Where:** §5.1 table `hit_test(handle, view, x, y)` ("… last frame of `view` (and `floor`, when the view is floor-scoped)"); §5.1 bullet ("The last-frame cache is keyed by `(view, floor)`"); §5.2.1 `/v1/hit-test` (has `floor`); DD-125.
- **Problem:** The phone and pack FFI cannot say which floor's frame to use. So `None` versus `409 no_frame` cannot be decided on floor views, and the HTTP and FFI surfaces differ.
- **Fix:** (a) Make it `hit_test(handle, view, floor: Option<u32>, x, y)`, and define omitted `floor` on floor views as `missing_param`. (b) Or key the FFI cache by view only and document the difference.

### F-08 — "Select and download share one fetch" conflicts with a registry keyed by verb (minor)
- **Where:** §5.5 ("Select and download share one fetch: … a fetch for that `(file_uuid, identity, verb)` is already in flight"); DD-112; §5.5 source table row 1 ("unless a snapshot already exists for this identity + current archive"); step 0 (keyed by verb); Q-07.
- **Problem:** Under the Q-07 draft (one verb) this holds. Under the two-verb alternative the spec allows, a concurrent select plus download use different keys, so two GETs happen. Also, step 0 cannot see a snapshot made under the other verb, which contradicts row 1 ("identity + current archive").
- **Fix:** Key the in-flight registry and the "snapshot exists" lookup by `(file_uuid, identity)`. Keep the verb only in the inclusion-record idempotency key.

### F-09 — Step 0, step 1, and the registry insert are not stated to be atomic (minor)
- **Where:** §5.2.1 Write mutex (lists "step 0 lookup, step 1, …, in-flight registry insert/remove" as separate sub-steps); §5.5 numbered steps; AT-126.
- **Problem:** If the mutex is released between step 0 and step 1, two concurrent selects both see "no line" and write two tile-10 records. If the registry insert happens after the mutex is released, a second request can start a second GET. AT-126 assumes atomicity that the text does not require.
- **Fix:** State that step 0, step 1, and the registry check-or-insert run in one mutex hold.

### F-10 — The index is described as both "update the line" and append-only (minor)
- **Where:** §5.5 step 2 ("update the index line's `snapshot_sha256`") vs §5.5 Inclusion index / DD-104 ("append-only … appends a new line"; "There is no in-place rewrite").
- **Fix:** Change step 2 to "append a new index line with the snapshot SHA-256".

### F-11 — The `SemanticNode` comment says its CSS coordinates share a space with frame width/height, which are pixmap pixels (minor)
- **Where:** §5.8 `SemanticNode` comment ("CSS pixels, same space as hit_test x/y and frame width/height"); `RenderFrame.height` comment ("pixmap pixels"); §5.2.1 response `width`/`height` are pixmap.
- **Fix:** Say "same space as `hit_test` x/y and the request's CSS `w`/`h`".

### F-12 — DD-113 says "remove the queue file" where the body says "remove `revoke-pending`" (minor)
- **Where:** DD-113 ("delete `lrs-retry`, then remove the queue file") vs §5.2.1 ("deletes `--data-dir/lrs-retry/` and finally removes `revoke-pending`").
- **Fix:** Change DD-113 to "then remove `revoke-pending`".

### F-13 — Startup consent-file write failure with `revoke-pending` is undefined (minor)
- **Where:** §5.2.1 ("On the next start, if `revoke-pending` is present, the host … retries the consent-file write and queue deletion"; only the queue-delete failure is made a startup error); AT-169 ("binds only if that write and the `lrs-retry` delete succeed").
- **Fix:** In the normative text, say a failed consent-file write at startup is a startup error naming `--consent-state-file`, binds nothing, and leaves `revoke-pending`.

### F-14 — `POST /v1/consent {"state":"on"}` after `consent_write_failed` is undefined (minor)
- **Where:** §5.2.1 ("refuses every LRS send for the life of the process (AT-169)").
- **Problem:** It is not said whether a later `on` is rejected, accepted while sends stay refused (GET then shows `on` but nothing sends), or clears the latch.
- **Fix:** For the rest of the process, return a defined error (for example `409 revoke_pending`) and keep reporting `off`. Add an AT.

### F-15 — AT-111 allows "not sendable" where §5.2.1 requires a startup error (minor)
- **Where:** AT-111 ("the queue directory is deleted or not sendable"); §5.2.1 ("If queue deletion fails at startup while consent is `off` … startup is an error … binds nothing"); §5.5 Retry queue ("If consent is `off` at process start, the host deletes the queue").
- **Fix:** Change AT-111 to "the queue directory is deleted, or startup fails naming `lrs-retry` (AT-124)".

### F-16 — When `lrs_status` is `failed` versus `queued` is undefined (minor)
- **Where:** §5.2.1 `/v1/select` (`sent | queued | skipped | failed | not_configured`); §5.5 Retry queue; AT-59.
- **Fix:** Define it. For example, `queued` means the attempt failed and a retry file was written; `failed` means the attempt failed and no retry file could be written (or a revoke aborted it). Also say what is returned when an in-flight POST is aborted by revoke (AT-156).

### F-17 — Missing JSON body fields have no error code (minor)
- **Where:** error catalogue `missing_param` ("A required **query** field (`w`, `h`, `handle`, `view`, `floor`, `t`)"); JSON routes need `path`, `paper_id`, `handle`, `state`, `x`, `y`, `view`.
- **Fix:** Extend `missing_param` to cover required JSON body fields, and say whether an unknown enum value for `state` is `invalid_param`.

### F-18 — AT-175 is missing from the test-location list (minor)
- **Where:** §5.1 Test locations ("AT-64–AT-174"); document history ("AT-01–AT-175").
- **Fix:** Change it to AT-64–AT-175.

### F-19 — AT-76 omits `--headless`, which fails in non-window builds and on display-less CI (minor)
- **Where:** AT-76 ("`--headless` is optional on loopback"); §5.2.2 inventory (`--headless` "required in builds without cargo feature `native-window`"); §5.2 Native window ("If `--headless` is omitted and no display is available, startup fails and names `DISPLAY`").
- **Fix:** Have AT-76 pass `--headless`, or say it runs only on a `native-window` build with a display and is skipped (and reported) otherwise.

### F-20 — The `drive_path` root set differs between the source table and DD-89 (minor)
- **Where:** §5.5 Source decision table (inside `--wiki-root`, `--corpus-root`, or `--view-root`) vs §5.2.2 Path rule / DD-89 (a resolved `drive_path` must be inside an allowed request root, which in Scholar mode also includes `--snapshot-dir` and `--download-dir`).
- **Fix:** Say explicitly that a `drive_path` resolving into `--snapshot-dir` or `--download-dir` is not an on-disk source (or that it is), and align DD-89.

### F-21 — Content validation of `--pinned-keys-file` and `--lrs-credentials-file` is undefined (minor)
- **Where:** §5.4 item 2 (pinned-keys format); §5.5 LRS client ("kite uses the first credential line"); §5.2.2 ("Required means a startup error … if it is missing or invalid").
- **Problem:** The spec does not say what happens with a non-hex or wrong-length key line, a file with no keys left after comments (empty list, or error?), a credentials file with no credential line, or a password containing `:`.
- **Fix:** Make each of these a startup error that names the file (or define a behavior), and add AT cases.

### F-22 — The `--egress-allowlist` port rule is ambiguous (minor)
- **Where:** §5.2.4 ("exact hostnames or exact IP literals, each optionally followed by `:port`").
- **Problem:** The spec does not say whether an entry without a port matches any port or only the scheme's default, or whether `host:443` matches `https://host/` with an implicit port.
- **Fix:** Define matching against the effective port (an explicit port, or the scheme default). An entry without a port matches only the scheme default port, or any port; choose one. Add an AT case.

---

## Notes (not findings)
- The bezier mid-value in §5.8 checks out: progress ≈ 0.9461 and opacity ≈ 0.9569 at t = 0.225 s.
- Scope cuts (DD-45, DD-128, Q-15, Q-16), the Unsigned label for a pinned list with no signatures (§5.4 table, AT-154), and the open questions are not raised as findings.
