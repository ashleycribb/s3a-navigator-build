# Goldfish review — s3a-kite spec v9 (round 9)

Reviewer: fresh/independent; read only `/workspace/goldfish/s3a-kite.md` (v9, 1219 lines).
Date: 2026-11-16 (America/New_York).

**Summary: 0 blocker / 9 major / 16 minor (25 findings).** Not a clean round: there are majors.

Majors: F-01 to F-09. Minors: F-10 to F-25.

---

## Major findings

### F-01 (major): The `revoke_pending` row in the error catalogue contradicts §5.2.1, DD-113 and AT-189
- **Where:** §5.2.1 error catalogue row `409 revoke_pending`; §5.2.1 "Consent revoke and in-flight LRS"; DD-113; AT-181; AT-189; DD-83 (says the catalogue is normative).
- **Quote (catalogue):** "`POST /v1/consent` `{ "state": "on" }` after a `consent_write_failed` in this process (AT-181)."
- **Quote (§5.2.1 / AT-189):** "After a failed `revoke-pending` write there is no such file, so a later POST on is not `revoke_pending`" / "A later `POST /v1/consent` `{ "state": "on" }` in the same process is not `409` `revoke_pending`".
- **Problem:** A failed `revoke-pending` write also returns `consent_write_failed`. The catalogue, which DD-83 calls normative, therefore requires `409 revoke_pending` in exactly the case where AT-189 forbids it. The catalogue also leaves out the `revoke_delete_failed` trigger that DD-113 lists. An implementer who keys off "a `consent_write_failed` happened" will fail AT-189. One who keys off "the file is present" contradicts the catalogue.
- **Fix:** Reword the row to: "`POST /v1/consent` `{state:on}` while `--data-dir/revoke-pending` exists (after a consent-file write failure or `revoke_delete_failed`; AT-181, AT-124). Not returned after a failed `revoke-pending` write (AT-189)."

### F-02 (major): A failed `revoke-pending` write loses the revoke across restart, so LRS traffic and queued statements resume
- **Where:** §5.2.1 "Consent revoke…"; DD-113; AT-189; §5.5 retry queue; DD-66.
- **Quote:** "If the `revoke-pending` write itself fails, the host returns `500` `consent_write_failed`, sets the in-memory flag `off`, refuses every LRS send for the life of the process, and leaves no `revoke-pending` file (AT-189)."
- **Problem:** The spec does not say whether the host goes on to write the consent file `off` and delete `lrs-retry/` after this failure. The step order ("first writes `revoke-pending` … then writes the consent file") implies it stops. If it stops, nothing durable records the revoke: the consent file still says `on`, the queue is intact, and no `revoke-pending` exists. After a restart, the host reads `on`, retries the queue at startup, and sends statements the researcher revoked. That breaks "xAPI to the LRS is consent-only" (header) and DD-66. A second gap: AT-189 allows a later `POST on` to succeed. The spec does not say whether the in-memory flag then becomes `on`, given that sends are refused "for the life of the process". GET could then report `on` while every send is refused, which is an unobservable state.
- **Fix:** Pick one:
  - (a) On a `revoke-pending` write failure, still attempt (best-effort) the consent-file `off` write and the `lrs-retry/` delete. Return `consent_write_failed` only if the consent-file write also fails, and in that case have the process exit or refuse to serve Scholar writes (fail closed).
  - (b) Make `revoke-pending` the second write: write consent `off` first, so a durable `off` exists before anything else. Treat the "both writes failed" case as fatal.

  In either case, define what a later successful `POST on` does to the "refuse for life" latch. Either the latch clears and GET says `on`, or POST on returns an error while the latch is set. Add an AT that covers restart after the AT-189 failure.

### F-03 (major): AT-169's fixture cannot make the normative consent-file write fail
- **Where:** AT-169; §5.2.3 ("It writes with a temp file plus atomic rename in the same directory").
- **Quote (AT-169):** "On a Scholar host whose `--consent-state-file` is made unwritable after a successful start with consent `on`, `POST /v1/consent` `{ "state": "off" }` returns `500` `consent_write_failed`".
- **Problem:** The consent file is replaced with temp + `rename(2)` in its parent directory. Making the file itself unwritable (for example `chmod 0400`) does not stop a rename into a writable directory, so a correct implementation succeeds and AT-169 fails. Tests "must not be edited to make them pass" (§5.1 / §7). The only ways to pass are a non-atomic write or a pre-check of the file mode, and both contradict §5.2.3. The same fixture problem reaches AT-181, which builds on AT-169.
- **Fix:** Have AT-169 make the consent file's parent directory non-writable (and say so explicitly), or use an injected write-error hook. The parent directory must be distinct from `--data-dir`, so that `revoke-pending` can still be written. State that requirement.

### F-04 (major): LRS transport failures are not classified
- **Where:** §5.5 step 3; §5.5 "HTTP outcomes"; DD-134; DD-131; route table `lrs_status`; `--lrs-timeout-secs`.
- **Quote:** "A 2xx, or a `409` on a replayed statement `id`, is success … A 408, 429, or 5xx is queued … Any other 4xx is a permanent failure".
- **Problem:** Only HTTP status codes are classified. The commonest failures produce no status at all: a `--lrs-timeout-secs` expiry, connection refused, a DNS failure, a TLS handshake or webpki validation failure, or a reset mid-body. The implementer has to guess whether these are `queued` (retryable) or `failed` (and whether a diagnostic file is kept). The route table's "`queued` when the attempt failed and a retry file was written" does not decide it. The choice changes what identity-bearing data sits in `--data-dir`.
- **Fix:** Add a row. Suggested wording: "No HTTP response (timeout, connect, DNS, or I/O error) is treated like 5xx: queued if consent is still `on` in the same mutex hold, else `skipped`. A TLS certificate-validation failure is permanent (`failed`) / or retryable." Pick one for TLS. Add an AT for a timeout and for connection refused.

### F-05 (major): The LRS-host deny rule blocks the `egress_test` fetch fixture when both fixtures are on loopback, conflicting with the consent-on ATs
- **Where:** §5.2.4 deny list and `egress_test` bullet; DD-64; DD-76; AT-54 (second case); AT-26, AT-59, AT-91, AT-110, AT-156, AT-186, AT-191 (consent on, loopback `http` LRS, and an `egress_test` source fetch in the same host).
- **Quote:** "Denied: … the configured LRS host plus every IP it resolves to (even if that IP would otherwise be allowed)" / "In an `egress_test` build it excepts only `127.0.0.1` and `::1` at that exact port."
- **Problem:** The LRS must be loopback `http` in tests, and the fetch fixture must be on `127.0.0.1` or `::1`. If the LRS fixture is on `127.0.0.1`, every source fetch to `127.0.0.1:<egress port>` is denied as an "LRS IP". The spec does not say whether the `egress_test` exception overrides the LRS-host denial. AT-54 says the LRS denial applies "even if otherwise allowed", which suggests it does not override. Several fail-first ATs therefore depend on a harness arrangement that is not specified, such as the LRS on `::1` or `127.0.0.2` and the fetch on `127.0.0.1`. The most natural arrangement makes them fail.
- **Fix:** Pick one:
  - (a) Make the LRS denial compare `(IP, port)` rather than IP alone.
  - (b) State the harness rule: the LRS fixture binds an address distinct from the `egress_test` fixture address (for example LRS on `[::1]` or `127.0.0.2`, fetch on `127.0.0.1`). Add that to the §7 fixtures paragraph and to AT-54.

### F-06 (major): `paper_identity` and the Select hit need information the pack API cannot receive
- **Where:** §5.1 `paper_identity(handle)`; `Hit::Select`; `render_page(..., select_enabled)`; `open_path(ctx, path, …)`; `KiteContext::new(ua_fonts)`; DD-98 ("The pack does not read the host flag"); §5.5 candidate rules; AT-96; AT-101.
- **Quote:** "`Some(stem)` only when the canonical opened path is a current-candidate dossier under the §5.5 glob `{root}/papers/SRC-*.md` (`--wiki-root` wins if the same relative path exists under both roots)".
- **Problem:** To compute this, the pack would need `--wiki-root` and `--corpus-root`, the candidate rules (non-recursive glob, YAML `source_id` equal to the stem, parseable UTF-8 front matter, `--max-open-bytes`), and the request-time recompute. No pack function receives roots or a candidate set. `KiteContext` holds only fonts and a clock, and there is "no process-global state". `Select { paper_id }` is returned by the pack `hit_test` and depends on `paper_identity`, so the pack cannot produce it either. Implementers will invent a hidden global or an undocumented parameter.
- **Fix:** Pick one:
  - (a) The host computes the identity and passes `select_paper_id: Option<String>` into `render_page` (replacing `select_enabled: bool`). Drop `paper_identity` from the pack, or make it return what the host supplied, and update AT-96.
  - (b) Add a `ScholarRoots { wiki_root, corpus_root, max_open_bytes }` argument to `open_path` (host-only), and define exactly which candidate checks the pack runs.

### F-07 (major): For Edge files, "seal trailer" and "seal-failed file" leave open whether a chain break makes the whole file unauthenticated for consent
- **Where:** §5.4 item 6; §5.3 item 4; DD-111; the §5.4 label table (an Edge chain break gives `seal_status` `seal_failed`); AT-178.
- **Quote (item 6):** "A file whose seal trailer is present and fails verification … is unauthenticated as a whole" … "including a CRC-failed tile, a tile after a seal-chain break, every tile of a seal-failed file".
- **Problem:** Edge files have a seal chain in type 70, not a seal trailer, so the first sentence does not map onto Edge. The label table sets `seal_status` to `seal_failed` for every Edge chain break. Read that way, "every tile of a seal-failed file" makes any chain-broken Edge file unauthenticated as a whole, and grants on tiles before the break are ignored. "A tile after a seal-chain break" and §5.4 item 2 ("tiles before the break that already verified remain valid floors") imply the opposite: grants before the break count. The two readings give different `withheld_count`s, and AT-178 does not tell them apart.
- **Fix:** For Edge, state exactly one of these:
  - (a) A chain break makes only the breaking tile and later tiles unauthenticated. Verified tiles before the break are authenticated, and their grants count.
  - (b) Any Edge `seal_failed` makes every tile unauthenticated (matching the hyper-tile trailer rule).

  Replace "seal trailer" with "seal chain" in the Edge sentence, and add an AT with a pre-break grant.

### F-08 (major): For the `missing_signature` quarantine, valid floors and consent authentication are undefined, so grants in a signature-stripped file are honoured
- **Where:** §5.4 Ed25519 bullets ("`FLAG_ED25519_SIGNED` set with no type-70 tile and no usable key is **Quarantined**"); §5.4 label table; §5.4 item 6 and §5.3 item 4 (the lists of what makes a file unauthenticated as a whole); §5.1 mapping table (`invalid_tile_indices` is "every index" only for `signer_not_pinned` or no valid floors); DD-111; AT-134; AT-165.
- **Problem:** For `signer_not_pinned` the spec says "no tile is a valid floor" and that the file is unauthenticated as a whole. For `missing_signature` (the flag says signed, but there are no signatures) it says only "Quarantined". It does not say whether tiles stay valid floors, and the file is not on the unauthenticated-as-a-whole list. The seal chain is unkeyed SHA-256. Someone can therefore change a type-71 denial into a grant on a signed Edge file, recompute the chain, and drop the type-70 tiles. The result is Quarantined, but its "valid floors" are still authenticated, and the forged grant releases withheld event payloads. This defeats DD-111 restrict-only. The `invalid_tile_indices` and AT-165 agreement for this case are also undefined.
- **Fix:** Add `missing_signature` (and, by symmetry, any Quarantined Edge status with no verified signer) to "unauthenticated as a whole". Then pick:
  - (a) No valid floors, like `signer_not_pinned`.
  - (b) Floors stay valid but consent grants are ignored.

  Add the case to the §5.1 mapping table and to the AT-165 fixture list, and add an AT where a stripped-signature file carries a grant. (Note: clearing `FLAG_ED25519_SIGNED` as well gives "Unsigned", which is still honoured. If that downgrade is accepted, record it as a DD.)

### F-09 (major): Step 3 (LRS) behaviour after a 409 `snapshot_conflict` or `download_conflict` is undefined, and the statement can be lost permanently
- **Where:** §5.5 numbered steps 0–3; §5.6 snapshot reuse (`snapshot_conflict` "with `included: true`"); §5.6 download names (`download_conflict`); route table for `/v1/select` and `/v1/download`; DD-120 ("A last matching index line skips step 1 and step 3").
- **Problem:** Both conflicts happen after step 1 succeeded, and both return an error object with no `lrs_status`. The spec does not say whether step 3 still runs for that request. If it does not, the next repeat finds a matching index line and skips step 3 by rule, so a consent-on inclusion is never traced. If it does, the client cannot learn the outcome, and a queued or failed result is invisible. The fetch-failure path explicitly says "step 3 is still attempted"; the conflict paths say nothing.
- **Fix:** Pick one:
  - (a) Step 3 runs after both conflicts, and the 409 body carries `lrs_status`. Update the catalogue rows and AT-94 / AT-180.
  - (b) Step 3 is skipped, and the index line is marked so that a repeat still runs step 3, for example a `lrs_emitted:false` field (this changes the step 0 rule).

---

## Minor findings

### F-10 (minor): The permanent-failure diagnostic file conflicts with the data-dir privacy rule and has no defined marker or claim location
- **Where:** §5.5 "Pass 1 list and skip…" bullet; §5.5 HTTP outcomes; DD-134; AT-191; revoke (`lrs-retry/` delete).
- **Quote:** "The retry queue under `--data-dir/lrs-retry/` may hold identities only when consent is on and a sendable statement exists." vs "Any other 4xx is a permanent failure: log it, keep the queue file, and never retry that file."
- **Problem:** The permanent file is never sendable but holds the identity and the researcher id. The spec also never says how a later retry pass tells it apart from a sendable file (no name or marker format), or where a claimed file is renamed to. If the claim renames the file out of `lrs-retry/`, a revoke in progress at that moment would not delete it.
- **Fix:** Define the names, for example `lrs-retry/{id}.json` (sendable), `lrs-retry/{id}.claimed`, and `lrs-retry/{id}.permanent`, all inside `lrs-retry/`. Amend the privacy sentence to allow `.permanent` files while consent is `on`.

### F-11 (minor): `snapshot_invalid` is missing from the `/v1/download` response contract
- **Where:** route table `/v1/download`; §5.5 step 2b; DD-103; AT-180.
- **Problem:** The route row lists only `"no_source"`, `"fetch_failed"` and (as a 409) `"download_conflict"`. Step 2b adds `"snapshot_invalid"` with 200 semantics.
- **Fix:** Add `"snapshot_invalid"` to the row, with `path: null`.

### F-12 (minor): The consent timestamp after a failed `revoke-pending` write refers to a file that does not exist
- **Where:** §5.2.3 ("After `consent_write_failed` (or a failed `revoke-pending` write), … the timestamp is the revoke time written in `revoke-pending`"); DD-113.
- **Fix:** For that case, say the timestamp is the in-memory revoke time (RFC3339 UTC). Alternatively, say `null`.

### F-13 (minor): The write-mutex scope list omits the `/v1/open` working-archive copy
- **Where:** §5.2.1 "Write mutex" ("held **only** for local disk writes (…list…)") vs §5.2.1, DD-78 and DD-114 ("`/v1/open` of the working archive copies the file under the process-wide write mutex").
- **Fix:** Add "working-archive read-copy for `/v1/open`" to the enumerated list.

### F-14 (minor): Step 2 waiters and index-line verbs are unclear, and the AT-186 sentence keys on the wrong line
- **Where:** §5.5 step 2 and the closing paragraph; DD-112; DD-120.
- **Quote:** "write the snapshot … if this request started the GET, and **append a new index line** with the snapshot SHA-256" / "A repeat whose index line has a null `snapshot_sha256` … retries the fetch".
- **Problem:** The spec does not say whether a request that only waited also appends a line, and under which verb. With two Q-07 verbs, the waiter's own verb line stays null. Step 0 uses "any verb", but the closing sentence speaks of "its index line", which could trigger a second fetch.
- **Fix:** State that only the GET starter appends, with its own verb, and that step 0's any-verb snapshot test governs retries. Reword the AT-186 sentence to "no index line for `(file_uuid, identity)` has a non-null `snapshot_sha256`".

### F-15 (minor): `--snapshot-dir` vs `--download-dir` disjointness is unspecified, and the sidecar `flock` can conflict with itself
- **Where:** §5.2.2 disjointness; DD-93; DD-114; DD-136.
- **Problem:** No rule stops the two flags from being equal or nested. If they are equal, the host takes two exclusive `flock`s on the same `.lock` from two descriptors, which deadlocks with a blocking `flock` or fails with an unclear error with a non-blocking one. The spec also never says the sidecar `flock`s are non-blocking, which AT-164's "startup error" requires.
- **Fix:** Require `--snapshot-dir` and `--download-dir` to be pairwise-disjoint (a startup error naming both flags, added to AT-179). State `LOCK_EX|LOCK_NB`.

### F-16 (minor): The on-disk `drive_path` copy has no regular-file check and no size cap
- **Where:** §5.5 source table ("copy of the on-disk file"); step 2b (runs under the write mutex); `--max-open-bytes`.
- **Problem:** A `drive_path` that canonicalizes to a FIFO or device inside a root would hang delivery while holding the process-wide mutex, which blocks every write and every consent revoke. A huge file has no cap. The file is also hashed for its name and then copied, which is a TOCTOU window.
- **Fix:** Require a regular file (otherwise `on_disk: false`) and cap it with `--max-open-bytes` (or a new `--max-download-bytes`). Hash the bytes as they are written to the temp file and name the file from that hash.

### F-17 (minor): The floor list for a seal-failed hyper-tile is optional ("may")
- **Where:** §5.3 item 6; AT-90; AT-185 ("CRC-valid committed tiles may still be listed as floors").
- **Problem:** "May" makes the valid floor list, `invalid_tile_indices`, `Floor(n)` hits and `invalid_floor` responses implementation-defined for this case.
- **Fix:** Say "are listed as valid floors (display only; unauthenticated for consent)".

### F-18 (minor): The `quarantine_reason` priority is stated only for the reader-core mapping
- **Where:** §5.1 mapping table; DD-115 ("Pack `open` and a mapped reader-core `verify` must agree on … `quarantine_reason`").
- **Fix:** State that pack `open` uses the same priority order (`signer_not_pinned` > `missing_signature` > `seal_trailer` > `chain_break` > `signature_known_signer` > `crc`).

### F-19 (minor): The last-frame cache key is unclear for view aliases and for a stray `floor`
- **Where:** §5.2.1 Handles; §5.2.6; DD-125.
- **Problem:** The spec does not say whether a `banner` frame followed by `hit_test view=manifest` (the same `render_manifest`) is `no_frame` or a hit. It also does not say what happens when `floor` is supplied on a non-floor view (`/v1/frame` or `/v1/hit-test`): ignored, or `invalid_param`.
- **Fix:** Normalize the aliases to one cache key, `first`. Make `floor` on a non-floor view `400 invalid_param` (or ignored; pick one).

### F-20 (minor): AT-76 contradicts the §7 preamble on the control select
- **Where:** AT-76 ("control-then-negative as in the preamble") vs §7 ("AT-76 is exempt from the same-process control-select requirement; any control write for AT-76 runs on a separate Scholar host").
- **Fix:** Replace the AT-76 parenthetical with "(control write on a separate Scholar host, per §7)".

### F-21 (minor): The §8 preamble range is stale
- **Where:** §8 ("DD-01–DD-133 keep their topics … Changes of meaning since v8") vs DD-134–DD-137 and the §12 v9 entry ("DD-01–DD-137").
- **Fix:** Change it to DD-01–DD-137.

### F-22 (minor): How `revoke-pending` is written is unspecified, and AT-189's fixture is ambiguous
- **Where:** §5.2.1 revoke; DD-113; AT-189 ("directory not writable for that name").
- **Problem:** The spec does not say whether `revoke-pending` is written atomically (temp + rename). If AT-189 plants a directory named `revoke-pending`, then "revoke-pending is present" holds at the next start, and "no `revoke-pending` file" becomes ambiguous.
- **Fix:** Specify a temp + fsync + rename write. Define "present" as a regular file. Make AT-189 use an injected write error or a read-only `--data-dir`, and if it uses a read-only `--data-dir`, say how `lrs-retry` deletion is expected to behave.

### F-23 (minor): Handling of symlinked or escaping candidate dossiers is undefined
- **Where:** §5.5 candidate item 1 ("every regular file matching the exact glob"); §5.2.2 path rule ("a candidate file … must canonicalize strictly inside an allowed request root … `409` `path_escape`").
- **Problem:** The spec does not say whether "regular file" means `lstat` or `stat`. It also does not say whether a dossier symlink that escapes its root is omitted from `GET /v1/candidates` or makes the whole request `409 path_escape`.
- **Fix:** Use `lstat` (skip symlinks, matching the corpus walk), or canonicalize and silently omit escapes. Make `/v1/candidates` never return `path_escape`.

### F-24 (minor): An LRS 409 on a first (non-replay) emit is ambiguous
- **Where:** §5.5 step 3; DD-134; AT-191.
- **Quote:** "a `409` on a replayed statement `id` also counts as success".
- **Problem:** On a first emit (not a retry), a 409 could be classed as success or as "other 4xx = permanent".
- **Fix:** Define 409 as success for every emit (the `id` is host-minted, so a 409 means the statement already exists), or as permanent on a first emit. Pick one.

### F-25 (minor): Control presence is undefined in two edge cases
- **Where:** §5.1 Hit bullets; §5.8; AT-97; AT-144.
- **Problem:**
  - It is undefined whether `render_page(select_enabled=true)` paints a Select control when the page has no paper identity.
  - It is undefined whether the `unsupported` banner paints a Continue control, given that `proceed` is always `409 unsupported`.
- **Fix:** Paint Select only when the identity is `Some`. Paint no Continue on `kind: "unsupported"`, and have `hit_test` return `None` there.

---

## Blocker / major titles (for the parent)
- F-01: The `revoke_pending` error-catalogue row contradicts §5.2.1, DD-113 and AT-189.
- F-02: A failed `revoke-pending` write loses the revoke across restart; LRS traffic and queued statements resume, and the POST-on latch is undefined.
- F-03: AT-169's chmod-the-file fixture cannot make an atomic temp+rename consent write fail.
- F-04: LRS transport failures (timeout, connect, DNS, TLS) are not classified as queued or failed.
- F-05: The LRS-host deny rule blocks the loopback `egress_test` fetch fixture, conflicting with the consent-on ATs.
- F-06: `paper_identity` and the Select hit need roots and the candidate set, which the pack API cannot receive.
- F-07: For Edge, "seal trailer" / "seal-failed file" leaves open whether a chain break voids pre-break consent grants.
- F-08: The `missing_signature` quarantine has undefined floors and consent authentication; forged grants in a signature-stripped file are honoured.
- F-09: Step 3 (LRS) after 409 `snapshot_conflict` / `download_conflict` is undefined, and the statement can be lost permanently.
