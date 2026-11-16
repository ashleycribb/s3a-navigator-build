# Goldfish review — s3a-kite spec v17 (round 17)

Reviewer: independent fresh read of `/workspace/goldfish/s3a-kite.md` only (no prior reports, dispositions, repo, or web).
Date: 2026-11-16 (America/New_York)

**Summary: 0 blockers, 4 majors, 10 minors.**

Out of scope by instruction and not re-argued here: DD values, Q-xx, scope cuts, and accepted residuals (in particular the DD-141 signature-strip and trailer-strip downgrades).

---

## Majors

### F-01 (Major): `paper_identity` "wiki_root wins" needs filesystem facts the pack is not given or allowed to fetch, and "wins" has two meanings
**Refs:** §5.1 `paper_identity` row and `open_path` row; §5.5 candidate set ("If the same relative path … exists under both roots, `--wiki-root` wins"); DD-61, DD-139, DD-146; AT-96, AT-101, AT-188, AT-193, AT-207.

**Problem.**
1. DD-139 and the `open_path` row list every input `paper_identity` may use: the roots, `canonical_path`, `validated_root`, the YAML in the fd bytes, and `max_open_bytes`. The pack "reads from `fd` only" and, per AT-96 and DD-146, makes no second `lstat`. AT-207 still requires `open_path` of the **corpus** copy to return `None` *because a wiki copy exists*. None of the listed inputs says whether `{wiki_root}/papers/SRC-XXXX.md` exists. To pass AT-207 the pack would have to probe a second path by name, which the spec does not authorize or define (lstat? `O_NOFOLLOW`? does a symlink count? a FIFO?). This is unspecified filesystem I/O in a function the spec describes as fd-only.
2. "Wins" is ambiguous. The §5.1 text says the corpus copy loses if the same relative path **exists** under the wiki root. The candidate list is defined over **qualifying** files: regular files (by `lstat`) with parseable UTF-8 YAML whose `source_id` matches and whose size is within the cap. Suppose the wiki copy exists but is not a candidate: it is a symlink (AT-193), has bad or missing front matter (AT-188), or is oversize. Under the "is a candidate" reading, the list may show the corpus copy with `page_path` = the corpus path. Under the "exists" reading, the pack returns `None` for that path. AT-101 then fails: `POST /v1/open` of `page_path` must yield `paper_identity == id`. An identity could also be listed and selectable while its page has no Select control.

**Why it matters.** As written, AT-207 cannot be passed under the declared inputs. The candidate list and `paper_identity` can also disagree on observable behavior (AT-101).

**Fix.** Pick one rule and define it the same way in both places. For example: "the corpus copy loses iff `{wiki_root}/<same relative path>` is itself a qualifying item-1 candidate (regular file by `lstat`, parseable front matter, matching `source_id`, size ≤ cap)." Make the host compute that fact: it already walks the wiki root for the candidate list. Pass it into `open_path`, either as a field such as `ScholarRoots.shadowed_by_wiki: bool` or by passing `scholar_roots: Some` only for a winning dossier. Then state that the pack does no path I/O. Extend AT-207 with a case where the wiki copy is an omitted symlink or has bad YAML, and assert that the list and `paper_identity` agree.

### F-02 (Major): The candidate walk (§5.5 items 1 and 2) is not covered by the DD-146 shared safe-open rule, so a symlink or FIFO swapped in after `lstat` can be followed or hang the request
**Refs:** §5.2.2 "Shared safe-open rule" (exhaustive site list); §5.5 candidate set items 1–2; DD-143, DD-146; AT-188, AT-193, AT-197, AT-202.

**Problem.** The shared rule (`openat` from a startup dirfd, `O_NOFOLLOW|O_NONBLOCK`, `fstat` regular file, read cap +1) is stated as applying to exactly five sites: `drive_path`, snapshot reuse, the download-dir check, 2b reassembly, and the working-archive copy. Two reads are not on that list:
- Item 1 reads YAML front matter from each `papers/SRC-*.md`.
- Item 2 opens every corpus `*.s3a` found by the recursive walk.

For both, the spec says only "using `lstat`" and "does not follow symlinks", which is a check-then-open-by-name pattern. If a file is replaced by a FIFO between `lstat` and `open`, a plain blocking `open()` hangs. If it is replaced by a symlink, the open follows it. Oversize files are "skipped as fail-to-open", but no read cap is stated, so a file that grows after `lstat` can be read past `--max-open-bytes`. The same walk runs for every `GET /v1/candidates` and for every select/download/skip membership check (DD-143). A hang therefore stalls those requests, and a symlink can make the host read files outside the corpus (for example the token file; item 1 would then parse it as YAML for `title`). The spec treats the same swap as an attack for `drive_path` (AT-202) and for the working archive (AT-203), but leaves this path open and has no AT for it.

**Why it matters.** This is a symlink/FIFO and unbounded-read hole on a request path, with no covering AT.

**Fix.** Add "candidate item-1 dossier read" and "candidate item-2 corpus `*.s3a` open" to the shared safe-open site list. Do the walk from the root dirfd with `openat(O_NOFOLLOW|O_NONBLOCK)` and directory fds for descent, `fstat` a regular file, and read at most `--max-open-bytes` + 1 bytes; any failure is a silent skip. Add an AT in the style of AT-202: swap a listed dossier and a corpus `.s3a` for a FIFO and for a symlink to `--auth-token-file` between two `GET /v1/candidates`. Assert that both GETs return 200 promptly, that no token bytes appear, and that select membership does not hang.

### F-03 (Major): Working-archive writes check `(dev, ino)` on one fd and then reopen by path (`TileWriter::create` / `open_append`), leaving a symlink race AT-203 cannot catch
**Refs:** §5.2.2 `--working-archive` paragraph; §5.6 "API and locking"; DD-114, DD-146; AT-79, AT-203.

**Problem.** §5.6 states that the `TileWriter` constructors "take paths". The host compares `(dev, ino)` on its own `openat(O_NOFOLLOW)` fd and then calls `open_append(path)`, and on first write `TileWriter::create(path)`. Both re-resolve the final component by name. If an attacker who can write the archive parent swaps that component for a symlink (into `--corpus-root`, say) between the check and the constructor, `open_append` appends a tile-10 record to the target. Worse, `create` on a planted symlink may truncate the target. AT-203 only swaps before the request starts, so it cannot catch the race, yet the spec asserts that "the symlink target is not opened or appended to".

**Why it matters.** This is a TOCTOU symlink hole that can corrupt the read-only corpus. It is exactly the threat AT-203 models.

**Fix.** Require the writer to operate on the validated fd. Options:
- (a) Open or create with `openat(parent_dirfd, name, O_NOFOLLOW|O_CREAT|O_EXCL)` (no `O_EXCL` for append) and hand `TileWriter` the Linux magic link `/proc/self/fd/<fd>`, which reopens the same inode without re-resolving the name.
- (b) Add an fd-taking constructor in `s3a-engine`; this does not touch `s3a-core`.
- (c) At minimum, after the constructor opens, `fstat` the writer's own file handle, compare `(dev, ino)`, and abort with `500 write_failed` before any write.

State that create must never follow or truncate an existing symlink. Add a hook-based AT that swaps the component after the `(dev, ino)` check and before the writer opens it.

### F-04 (Major): For an Edge file with `FLAG_SEAL_CHAIN_ACTIVE` clear but type-70 seal records present, the "total" DD-150 table never ties signatures to tile contents, so a tampered file can stay Verified and its tampered grants trusted
**Refs:** §5.4 item 2 (signer attribution: "authentic if it verifies under any candidate key"), §5.4 label table ("`FLAG_SEAL_CHAIN_ACTIVE` clear … (from signatures below) | unsealed"), DD-150 derivation table ("single total derivation"), the unsigned-tail rule, DD-145, DD-148; AT-165, AT-201 (5).

**Problem.** A `DeviceSealRecord` signature covers the stored 32-byte `seal_hash`. A record is "authentic" if that signature verifies under a candidate key (DD-145 rule 2). The only rule that checks `seal_hash` against tile bytes is the "seal-hash mismatch → `chain_break`" row, and every surrounding statement frames it as an active-chain check ("an active Edge chain is broken…", "An active Edge seal chain is intact when…"). For a chain-inactive file that carries type-70 records, no row says whether `seal_hash` must equal the recomputed `compute_seal_hash`, so the "total" claim does not hold.

An implementation can follow the text literally and skip the recompute because the chain is inactive. It would then label a file **Verified** after a tile has been edited and its CRC recomputed, because the signatures still verify over the old hashes. All tiles up to the highest signed `tile_index` would count as authenticated valid floors, so an edited type-71 grant would be **trusted** and release event-log payloads. The pack and reader-core could also legitimately disagree, failing AT-165, because the AT-165 set has no such fixture. (Not rated Blocker: DD-141 already accepts an equivalent downgrade, stripping signatures to reach Unsigned/trusted, so this adds no new release capability. It does produce a false **Verified** label and breaks the totality claim.)

**Fix.** Add DD-150 row(s) for "chain inactive, type-70 present". Either (a) always recompute `compute_seal_hash` per tile when any type-70 record exists, treat a mismatch as `chain_break` with `break_index` at the first mismatch (and say whether `seal_status` stays `unsealed`), or (b) declare signatures on a chain-inactive file non-authenticating, for example `missing_signature`, or Unsigned plus the whole file untrusted. Define "authentic" as "signature verifies **and** `seal_hash` equals the recomputed chain hash at `tile_index`". Add the fixture to AT-201 / AT-165.

---

## Minors

### F-05 (Minor): The §5.3 unsigned-tail clause omits the "only when an authentic type-70 exists" guard
**Refs:** §5.3 item 4 "Trusted vs untrusted" ("have index greater than the highest authentic signed `tile_index`"); compare the §5.4 "Unsigned tail" paragraph and the §5.1 mapping ("if any authentic type-70 `DeviceSealRecord` exists").
**Problem.** Read alone, the §5.3 clause leaves "highest authentic signed `tile_index`" undefined for Unsigned and hyper-tile files. One reading makes every tile untrusted, which contradicts AT-112 / AT-154 / DD-141. The clause also omits the verifying type-70 container exemption.
**Fix.** Add "(only in a file that contains at least one authentic type-70 record; a verifying type-70 container is exempt, §5.4)".

### F-06 (Minor): DD-113 says every revoke persists, but §5.2.1 / DD-144 skip persistence when the flag is already off
**Refs:** DD-113 vs §5.2.1 "If the in-memory flag is already `off`, the revoke performs no persistent writes", DD-144, AT-198.
**Problem.** DD-113 says that "under the mutex set the flag `off` and write `revoke-pending` … write the consent file … delete `lrs-retry/`" with no same-state exception. An implementer who reads DD-113 alone would rewrite the consent file and its timestamp on a same-state `off`, contradicting §5.2.3 ("timestamp is unchanged").
**Fix.** Add to DD-113: "only when the flag was `on`; a same-state `off` performs no persistent writes (DD-144)".

### F-07 (Minor): Revoke handling must not be cancelled by a client disconnect, but the spec never says so
**Refs:** §5.2.1 consent transitions, DD-142, DD-144 ("only a fail-closed exit skips the decrement").
**Problem.** Receipt-time actions increment `revokes_in_flight` before the mutex is taken. In typical async servers, a dropped connection cancels the handler future, so the decrement at the end of the hold never runs. Every later `on` would then fail to qualify for the life of the process, a stuck state. The spec implies the revoke must run to completion but does not require it and has no AT for it.
**Fix.** State that once receipt-time actions have run, the revoke runs to completion, decrement included, even if the client disconnects. Add an AT case: revoke, then close the socket at once, then a later `POST on` succeeds.

### F-08 (Minor): AT-214's waiter clause and the "remove the registry entry in 2b" wording do not match the step-2 registry model
**Refs:** §5.5 step 2b ("any in-flight registry entry for this key is removed in this hold … Waiters receive the same `write_failed` outcome"), §5.2.1 in-flight registry, DD-112; AT-214.
**Problem.** Registry entries exist only for a GET and are removed in the step-2 publish hold, before any 2b runs. On-disk copies never create an entry. Waiters receive the **step-2** outcome (the snapshot path) and then run their own 2b. They cannot receive the starter's 2b failure. AT-214 passes only if the hook happens to fail the waiter's own 2b too.
**Fix.** Reword: "2b failure affects only the request whose 2b failed. A waiter whose own 2b fails gets `500 write_failed` with its own `included` / `lrs_status`." Drop the registry-removal sentence from 2b, or limit it to "none remains". In AT-214, require the hook to fail every 2b in the test.

### F-09 (Minor): Some AT assertions have no specified observable (offsets, card rectangle and opacity, native-window trigger)
**Refs:** AT-15 and AT-21 (tile offsets), AT-62 / AT-13 (card rectangle and opacity), AT-125 (native window issues `POST /v1/select`); §5.8 `SemanticNode` roles.
**Problem.**
- No frame, text-run, or semantic field exposes a tile's byte offset. The floor-list fields are `index`, `type_id`, `record_count`, `size_bytes`, and `payload_hash_short`.
- The animated card has no semantics role or rect, so "within 1 CSS px" depends on pixel edge-detection against unspecified colors.
- AT-125 does not say how the test makes the native window send a select, since the spec defines no UI automation hook.
**Fix.**
- Reword AT-15 / AT-21 to assert that each floor's `payload_hash_short` (or its detail content) equals the bytes at the expected offset.
- Add a `card` semantics role carrying its rect (and optionally its opacity) for `floor-transition` frames.
- Define a test hook, or narrow AT-125 to "any window-originated request carries the Bearer".

### F-10 (Minor): The type of `tiles_crc_failed` (and `detail`) in `KiteVerificationReport` is unspecified
**Refs:** §5.1 report fields, mapping table, DD-99.
**Problem.** `invalid_tile_indices` is defined as a union that includes `tiles_crc_failed`, which implies a list of indices, but the field is typed nowhere. A count would make the union undefined, and AT-165 compares reports field by field.
**Fix.** Declare `tiles_crc_failed: Vec<u32>` (increasing tile indices) and `detail: String`.

### F-11 (Minor): The error code for `ENOTDIR` from a non-symlink intermediate component is undefined
**Refs:** §5.2.2 path rule ("`ELOOP` or `ENOTDIR` from a symlink is `409` `path_escape`"), the `/v1/open` error order, DD-146.
**Problem.** `/v1/open` of `{root}/dir/file.md/x`, where `file.md` is a regular file, yields `ENOTDIR` with no symlink involved. The spec does not say whether that is `not_found`, `not_regular_file`, or `path_escape`.
**Fix.** State that `ENOTDIR` from a non-symlink component is `404 not_found`, or `409 not_regular_file`, and add it to AT-171.

### F-12 (Minor): When the `ADDON_EDU` check on an existing working archive runs is unstated (AT-46 depends on it)
**Refs:** §5.6 ("fail closed if an existing working-archive file lacks the bit"), the `addon_missing` row, DD-16; AT-46.
**Problem.** AT-46 needs host B to **start** and then fail on select. Startup already `lstat`s the working archive, so an implementation could reasonably fail closed at startup instead, and AT-46 would then fail.
**Fix.** State: "checked at each step 1 (not at startup); the result is `409 addon_missing`, nothing is written, and the request has no `lrs_status`." Also give `addon_missing` its place in the select error order.

### F-13 (Minor): The scope of the reserved temp-name rule for `/v1/open` is unclear
**Refs:** the `not_regular_file` row ("a temp file reserved in §5.6"), §5.6 / DD-122 ("Temp names are `s3a-kite-tmp-{pid}-*` … not openable via `/v1/open`").
**Problem.** A temp file is a regular file, so DD-146's `fstat` check passes. The rejection must therefore be name-based. The spec does not say whether that applies only in `--snapshot-dir` / `--download-dir` or also to a user file with that name under `--view-root` / `--wiki-root`. Nor does it say where the check sits in the `/v1/open` error order, or whether an AT covers it.
**Fix.** State: "a basename matching `s3a-kite-tmp-*` directly inside `--snapshot-dir` or `--download-dir` is `409 not_regular_file`, checked after the walk and before the read". Add a case to AT-171 or AT-187.

### F-14 (Minor): "Prefix" in the disjointness rules is not defined as a path-component prefix
**Refs:** §5.2.2 disjointness ("no directory is equal to or a prefix of another"), DD-93, DD-129; AT-145, AT-179.
**Problem.** Read as a string prefix, `/srv/data` vs `/srv/data2` is a false violation. Read as a component prefix, it is not. ATs could be written either way.
**Fix.** Say "equal to, or an ancestor directory of (path-component prefix after canonicalization)". Add the `/x/a` vs `/x/ab` negative control to AT-179.
