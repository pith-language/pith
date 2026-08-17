---
schema: design-doc/v1
title: a written digest names its algorithm
summary: every digest field in the written forms is prefixed blake3, the algorithm that actually hashed it; the name lives once in pith-ids and a test binds the written prefix to it, so the spelling cannot drift from the hasher the way the previous sha256 label did
id: decision-0054-a-written-digest-names-its-algorithm
kind: decision
status: proposed
created: 2026-08-11
updated: 2026-08-11
tags:
  - lock
  - digests
  - written-forms
relations:
  informed_by:
    - research-sources
    - research-index-formats
  depends_on:
    - decision-0021-arena-graph-engine
    - decision-0041-the-written-lock
    - decision-0043-the-development-environment
    - decision-0046-an-index-line-carries-the-requirement
    - decision-0048-pre-release-version-pinning
  amends:
    - decision-0041-the-written-lock
    - decision-0043-the-development-environment
    - decision-0046-an-index-line-carries-the-requirement
  supersedes: []
---

# a written digest names its algorithm

> takes a mislabel no record ever chose. every written form since 0041's prototype has spelled its digest fields `sha256:`, a look borrowed from go.sum and flake.lock, while every digest in the kernel is blake3. `pith-ids` wraps blake3; nothing in the workspace computes or verifies a SHA-256. the correction raised a question worth its own record: should a written digest name its algorithm at all?

## context

the prefix arrived with e86e800, the written lock's first prototype, and propagated from the lock to the index line, the environment file, and the log's leaves. no decision record argued for it, and nothing connected it to the hasher. the label was maintained by hand in parallel with `pith-ids`, which is the shape in which drift lives. a reader who trusted it and ran `sha256sum` over an archive got a mismatch against a correct lock.

the correction could go two ways. a neutral prefix (`digest:`) cannot misname an algorithm because it names none, and the single-party lock formats skip the tag: Cargo.lock carries its checksum as bare hex in a field named `cksum`, and flake.lock puts the algorithm in the field name. the two formats that tag the algorithm in the string, go's hash lines and Subresource Integrity, are multi-party formats, where producers and verifiers who do not control each other need the name to coordinate.

## proposed decision

every digest field in the written forms names the algorithm that hashed it: `blake3:`. the lock's resolver, universe, and bind lines, the index line's archive claim, the environment file's lock and substitution lines, and the log's leaves all spell it, and all derive from one constant in the lock's text grammar.

the ground is the reader. locks are read in diffs and merge conflicts, without the format's documentation open, and the one question a digest raises (what function produced this) the line can answer at the cost of seven characters. go's reference states the same choice for its hash column: "The hash column consists of an algorithm name (like `h1`) and a base64-encoded cryptographic hash, separated by a colon (`:`). Currently, SHA-256 (`h1`) is the only supported hash algorithm. If a vulnerability in SHA-256 is discovered in the future, support will be added for another algorithm (named `h2` and so on)." SRI's grammar puts the algorithm in the string at web scale: `hash-expression = hash-algorithm "-" base64-value`. go and SRI name the algorithm so that parties who do not control each other can coordinate; pith names it for the reader. the mechanism is the same, and so is the future it allows: a second digest kind sharing these formats arrives under its own name.

the drift that produced the mislabel is closed by construction. the name lives once, `pith_ids::DIGEST_ALGORITHM`, beside `DIGEST_LEN`, whose doc already says "every blake3-derived digest in the kernel", and a test binds the written prefix to it. the old mislabel required two places to disagree; the binder turns that disagreement into a failing test.

parse accepts only this spelling. a line naming another algorithm is refused; the message names the expected spelling and the span selects the field. under 0048 the pre-release answer to that refusal is to re-render, and no format version moves. the log's earlier leaves fail the leaf parse the same way, which is the honest behavior for a tree that hashed one way and spelled another.

## alternatives considered

### a neutral prefix (`digest:`)

the position argued inside this round. the written forms are projections of values the engine already hashed one way. the grammar pins the length to 64 hexadecimal digits, so the slot cannot hold a wider digest. and a prefix that names no algorithm cannot misname one.

rejected on the reader, with the drift objection answered by the binder above. the neutral spelling buys safety a test now provides, and it spends the line's one chance to say what the bytes are. it also gives up a distinction the names carry for free: if two digest kinds ever share a format, their prefixes tell them apart the way go's reserved `h2` does and SRI's per-resource hash lists do; a neutral prefix needs a second field to say the same thing.

### hash with sha256 to make the old label true

switch the content identity to SHA-256 so the spelling becomes correct.

rejected. 0021 chose blake3 as the wrapped hasher, and the written form would then be the only place a second hash function exists, two ways to compute what the format calls one field, with no external verifier. nothing outside the workspace checks these lines against the SHA-256 ecosystem today. when something does, 0044's shape applies: the foreign claim is checked at the fetch boundary, and pith binds its own measured identity.

### bare hexadecimal, cargo's shape

drop the prefix entirely, as Cargo.lock's `cksum` field does, and let the field's position carry the meaning.

rejected on self-description. the prefix is what makes a malformed digest refuse well: "carried `xyz`, rather than a `blake3:`-prefixed digest" says what was expected. a bare field gives a hand editor no signal which hex blob is content. cargo can afford bare hex because its checksum sits in a named JSON field; these lines are positional, and a position tells a reader nothing.

## consequences

all written forms move together because all derive from the one constant; a grep over crate source finds the algorithm spelled nowhere else. a lock, index, or log written by an earlier tree of this workspace is refused at read with the expected spelling named, which is 0048's pre-release break, and re-rendering a lock from the engine produces the new spelling with no other change.

if the kernel's hasher ever changes, the change is global and breaking, from computation keys to every written line, and under 0048's post-release rule that is a migration for committed user data. the named prefix makes such a migration visible line by line.

### measured

`the_written_digest_prefix_names_the_kernels_hash_function` (`crates/phloem/src/lock/text.rs`) binds the prefix to `pith_ids::DIGEST_ALGORITHM`: change either side and the binding fails.

`the_written_form_spells_its_digest_algorithm` and `a_digest_spelling_another_algorithm_is_refused_naming_the_expected_one` (`crates/phloem/src/lock/file.rs`) hold the round's reader claim and its compatibility statement. the rendered lock contains `blake3:` and no `sha256` anywhere, and a digest field relabeled `sha256:` over the same bytes is refused with the expected spelling named and the span selecting the mislabeled field.

the workspace suite on the commit that carries this record is green with the three tests above included.

## unresolved

a foreign digest carried as identity is the first real multi-algorithm line. cargo's `cksum` is SHA-256; an adapter over such a registry either re-measures under blake3 at the fetch, which is 0044's current shape, or the written forms grow a second name. either way the adapter's record decides.

the post-release story for a hasher change is stated here as a consequence, not designed. what a lock file migration between digest names looks like is a question for the release that faces it, under 0048's committed-user-data rule.
