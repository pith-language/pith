---
schema: design-doc/v1
id: decision-0044-the-first-source-adapter
title: the first source adapter reads a registry as a caller-side effect, and the witness for a remote binding is a transparency log checked locally
summary: a source adapter is a caller-side effect that produces declared inputs — a registry index becomes the candidate universe, a fetched archive becomes measured content, a log over binding lines becomes the evidence a verification consumes; the threat model names five adversaries and pith's position is detection after a binding exists and detection of a source that disagrees with the witness; pith prevents nothing and vouches for no content; the witness 0041 predicted is confirmed as the log — a checkpoint and an inclusion proof verified locally, the checkpoint pinned by a person's configuration on the same terms 0042 pinned origins; the git refusal lifts at the fetch, binding the measured archive of the materialized tree, because git's own object model is the intrinsic witness there; a candidate carries where it was read as its origin
kind: decision
status: proposed
created: 2026-07-13
updated: 2026-07-27
tags:
  - packages
  - sources
  - trust
  - provenance
  - effects
relations:
  informed_by:
    - research-artifacts-and-trust
    - research-sources
    - research-nix
  depends_on:
    - decision-0003-explicit-effects
    - decision-0007-tracked-dynamic-dependencies
    - decision-0014-reproducibility-properties
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0020-nix-as-adapter-not-substrate
    - decision-0027-retention-and-gc
    - decision-0032-action-granularity
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0042-binary-reuse-as-admitted-substitution
    - decision-0043-the-development-environment
  supersedes: []
---

# the first source adapter reads a registry as a caller-side effect, and the witness for a remote binding is a transparency log checked locally

> takes the question three records deferred. 0039: "what would settle it is a threat model for M-4's actual sources, which does not exist yet." 0041: "the first source adapter's threat model picks among the log, the threshold, and the attestation, and the entry's shape predicts the log." 0042: "what settles the leg is the first remote source adapter and the threat model it forces." this is that adapter and that threat model, and it is the first time any of the package machinery meets bytes it did not fabricate.

## context

M-4 closed with every measurement taken against an in-process universe over fabricated bytes, its own statement now says so, and the three deferrals above all point here. what changed with this record is that a registry exists in the prototype — a local filesystem index and archive store, real enough to be read, lied about, and republished — and a log exists beside it, so the witness question has an adversary to name instead of a shape to predict.

the systems that shipped this problem disagree about the answer, and the disagreement is about detection versus prevention. the hashing nobody disputes. Go's checksum database is an append-only merkle tree over the same lines a lock entry is — module, version, hash — with clients verifying inclusion proofs against a signed tree head. its design doc states the position plainly: "our focus turned to detection of compromise," and a compromised server can feed one victim a forked log only by serving that fork forever, with gossip and independent proxies to raise the cost of the lie. it detects a registry rewriting history. it detects nothing about a module that is malicious and honestly hashed. TUF takes the preventive position: four signing roles, thresholds, expiry, rollback and freeze protection, all inside a closed key set whose root keys are kept offline. an attacker below threshold fails, and the framework's own spec is explicit that protection means the attack fails, not that updates succeed. Debian's repository format is TUF's ancestor at one archive key: a signed Release file hashes every package index, every index entry hashes its package, and a mirror without the key can withhold but not forge. crates.io's index is the trust-the-registry position: the registry computes and publishes each version's checksum, versions are immutable by policy with `yanked` the one mutable field, TLS carries the transport, and no independent witness exists. Nix makes the fetch itself safe from below: a fixed-output derivation — `fetchurl`, `fetchgit` — declares its output hash in advance, which is why the manual lets these derivations, and only these, out of the network namespace, since the declared hash turns any network activity into a verifiable claim. and git's object model is its own witness: an object's name is the hash of its bytes, a tree's hash covers everything beneath it, a commit's hash covers its tree and parents, and a receiving side verifies every object against its name: the one entry in this list where the content address and the authentication are the same fact, and the decision below leans on it.

none of these is portable into pith whole. the log needs a log operator and a key story for its checkpoints. TUF needs distributed keys and an expiry discipline. the registry-trusted position is what pith has now, and the deferrals exist because it is weak. what the precedents do fix is the shape of every leg: coordinates name, digests authenticate, and the only open question is who witnesses the first binding.

## decision

### the threat model, written down

five adversaries, named separately because they have different answers.

a compromised mirror serves bytes other than the ones its registry's index claims. detected, unconditionally, at fetch time: every archive is hashed locally from the bytes read and compared against the binding, so the mirror's word is never taken and the diagnostic carries both content identities.

a registry that republishes under one name serves different content for the same coordinates with its own index updated to match — internally consistent, so the digest check alone cannot catch it. this is the adversary the witness decides about. the log's line for those coordinates is the independent record, and disagreement is detected. at the first binding — no line in any pinned log — the registry is trusted, and this record names the gap.

a network attacker substitutes bytes in flight. detected by the same measurement, whatever the transport does or fails to do. the prototype has no network at all, and the check does not depend on having one.

a malicious publisher publishes code that matches its own claim. no witness detects this: content that matches its digest is not thereby good. the binding machinery makes no claim here, and the record refuses to imply one. review is one answer and attestations about builds are another, and they answer different questions than a source pin does.

a compromised pith host rewrites locks and skips checks. nothing cryptographic resists the machine that runs it. the most a log buys against this is third-party comparability — two machines holding different checkpoints for one log have caught a fork — and that is gossip, unresolved here.

the position in one line: pith detects tampering after a binding exists and detects a source that disagrees with the witness. it prevents nothing and vouches for no content. per remote binding it trusts the pinned checkpoint, and the registry itself for the first line no log has witnessed.

### what a source adapter is, in the model

a caller-side effect, in the position 0041 put the lock's write and 0043 put the toolchain's discovery. the adapter's functions read a registry directory and produce values: a universe from the index, measured bytes from an archive, evidence from the log. the engine never learns that sources exist.

not a rule in the graph: 0007 forbids ambient discovery during evaluation, and a fetch inside evaluation would make the universe an ambient input, which is what 0040's reproducibility argument rules out. not an `Opaque`: 0032's consequences section records that `Opaque` still has no operational path in the engine — no registration, no scheduler step, no durable record — so choosing it here is choosing to build the engine's effect tier, and for a fetch that does not need it. a fetch also has a fully declarable contract, and 0019's progression runs from opaque to modeled, never the reverse. an `Action` is the honest future home and is deferred: one invocation of one tool, inputs the binding's coordinates, output the archive's bytes, expected digest declared in advance: Nix's fixed-output derivation shape, which makes network access safe from below. when an executor exists that admits network under a declared output digest, the fetch moves into the graph with the binding's digest as its fixed output, and nothing in this record's verification changes.

### where the candidate universe comes from

the query is a separate step whose result becomes the declared input, which is the whole reconciliation with 0040. the universe the adapter returns participates in the computation key like any other input. when the registry's answer moves between two runs — a version added, an index line rewritten — the universe digest moves and the lock's diff names it as the moved input, as a fabricated universe's digest moved in 0041's tests. a pinned re-resolution under the old universe reproduces the old selection, and the moved registry is drift-shaped at fetch time, reported with both content identities.

nothing here caches the index or its snapshot. the query is a read, repeated by the caller whenever it resolves. whether a registry snapshot is worth retaining as an artifact, on which axes of 0027's policy, is unresolved below.

### the witness, decided

the log. 0041's prediction is confirmed: a lock entry is a witnessed-hash-line shape, coordinates bound to a digest, and the transparency log is the arrangement that witnesses that line. go's sum database is the precedent down to the leaf spelling, which is why the log's leaves are the lock's own binding lines, one spelling shared by the file and the log.

what ships is the client half, as three legs in 0042's shape. the policy leg: the checkpoint a log serves is the checkpoint the configuration pins, and nothing vouches for the pinned checkpoint but the configuration naming it — 0042's degradation, repeated at this boundary. the first fact leg: the inclusion proof carries the line the log holds for these coordinates into the tree the checkpoint commits to, verified by recomputing the root from the leaf and the path. the second fact leg: the digest the line witnesses is the digest the binding names. a mirror is caught by the measurement, a republisher by the third leg, a checkpoint that no longer commits to its own leaves by the second, a foreign log by the first.

what the position costs is named rather than hidden. a pinned checkpoint that never moves is a freeze the client imposes on the log, and pith ships no answer to it: signatures and cosignatures over checkpoints need keys, consistency proofs and the gossip that compares checkpoints across machines and time are how go detects a log serving one victim a forked tree, and both are unresolved. the upgrade touches one leg — the pinned checkpoint becomes a verified one — and leaves the two fact legs standing, which is the same isolation 0042 bought for its authorization leg.

### the git candidate

the refusal lifts at the fetch, and what the entry binds is the measured archive of the materialized tree. the mechanism: the source sum gains a constructor for a materialized tree — the revision, the tree hash it resolved to, and the content identity measured from the bytes the fetch read — and the entry binds that content with the forge as its origin. the unfetched constructor, a bare revision and tree hash, still refuses: the refusal is the invariant on every constructor, that a lock binds content that was read, and the bare reference marks the edge between a reference and content instead of leaving it a gap in the machinery.

that the lifting costs nothing cryptographically is git's own doing. a revision authenticates its content intrinsically — object names are content hashes and the transfer verifies them — so the git source needs no log: the witness is the revision. the ref is the part git does not authenticate, a mutable pointer no commit records, and the ref is what the candidate records as provenance and never carries into a binding. resolution chooses among references. the fetch materializes only the choice, and the lock binds only the measurement.

### what the origin means

a candidate carries where it was read, and the entry inherits it. the old spelling, in which an archive's own digest stood in as its registry locator because an in-process universe named no registry, is gone: the registry identity is declared by the caller the way a forge's name and a local path are, and the origin records it. the policy leg still matches origins by exact equality, deliberately: an origin names a source, not content, and admitting a mirror is one configuration line naming it. 0042's separation of where-a-binary-came-from from who-authorizes-it stands unchanged, and the origins a substitution policy admits are the same values an entry now records.

## alternatives considered

### the fetch as a fixed-output action now

put the fetch in the graph immediately, as an action whose output digest the binding declares: Nix's `fetchurl` arrangement end to end.

rejected for this record's scope and named as the future home. no executor admits network at all today — the local executor's contract refuses what it cannot confine, and network policy is deny by default — so the action would exist as a declaration nothing can run, which is the dead-code position 0042 refused for attestations. the adapter as a caller-side effect produces the same bytes and the same measurements, and the verification this record ships is unchanged by where the bytes came from.

### threshold-signed metadata, TUF's shape

a root of trust offline, threshold signatures over the index, freshness and rollback protection inside the key set.

rejected on the same ground 0042 rejected keys: there is nothing to distribute. TUF also witnesses a repository's whole target set under one authority, which fits a pith-operated repository and does not fit the open question, where the sources are other people's registries and the witness must be one they can be caught disagreeing with. the log's line-level shape is the witness for that. a pith-operated repository adopting TUF internally is a future that does not interact with this boundary.

### attestations over fetched sources

a builder's signed claim binding a subject digest, the sigstore arrangement: keyless identity, ephemeral keys, Rekor's log underneath.

absorbed for binaries, refused for sources. 0041 already measured this: an attestation witnesses a build, and a source pin is not a build. the right home for the attestation is 0042's authorization leg, where it arrives beside the offer and upgrades one clause. Rekor is also a transparency log, which agrees with the choice here: when pith's own builds sign, the log shape is already the witness pith's client knows how to check.

### the adapter as an opaque boundary

wrap the whole fetch-and-verify in one `Opaque`, the way a nixpkgs derivation enters 0020's model.

rejected twice over. `Opaque` has no operational path — choosing it is choosing to build the engine's effect tier before a fetch needs one — and the fetch is the case 0019's escape hatch exists to decline: its contract is declarable down to the output digest, so modeling it as opaque hides a contract the code could state.

### trust the registry, digest-only

the crates.io position as policy: the configured registry is inside the perimeter, checksums guard integrity, nothing else is asked.

described rather than rejected, because it is the floor this record builds on: with no log pinned, every leg above still runs, the measurement still catches mirrors and tampering, and the record's refusal is only to leave the position unnamed. the difference between this and what ships is one pinned checkpoint.

## consequences

phloem gains three modules. the witness is the log's client half and the tree math an operator and a verifier share. the registry adapter does index, fetch, and witness reads over a directory, in the sparse-index layout crates.io fixed, miniaturized. the forge adapter does reference resolution and materialization through git. the candidate gains its origin. the source sum gains the materialized-tree constructor. the binding line is extracted as the one spelling the lock's file and the log's leaves share. no kernel constructor lands, the engine is unchanged, and xylem is unchanged: the fifth record in the package line to need none.

the artifacts-and-trust research questions take their first answers. "which provenance claims are measured by the engine and which by an adapter" is answered: the adapter measures, the engine never does, and the boundary is 0003's. "what trust policy is evaluated before reuse from a remote cache" is answered: 0042's admission test, unchanged, now over origins that name real sources. "should digests include a schema and hash algorithm identifier" keeps its answer with the research note: the file already spells its algorithm prefix per digest, and the log inherits the spelling. the deeper schema question stays with the research note.

### measured

the prototype reads real bytes and each claim has a test. a universe read from a registry directory resolves through the engine and locks bindings whose archives are then fetched and measured equal to what the entries bind. a tampered archive is refused with both content identities named. a registry that republishes under one name — bytes and index rewritten to agree with each other — passes the digest check and is refused by the log's witnessed line, with both digests named. a checkpoint the configuration does not pin is refused naming both checkpoints, and a checkpoint that no longer commits to its own leaves fails the inclusion proof naming the computed root and the checkpoint's. a registry answer that moved between two runs moves the universe digest and the lock's diff names the moved universe and the moved selection. resolving, locking, digesting, and verifying write nothing: only the adapter's reads touch a path. a bare git reference refuses to lock with the refusal 0041 fixed. the materialized resolution locks the archive's measured content, equal to what git itself produces for the same revision twice. the merkle tree's own tests cover every record verifying, a proof issued for another record, a tampered leaf line, a truncated path, and an index outside the tree, each refused with what was expected and what was found.

## unresolved

checkpoint authenticity. signatures, cosignatures, and a freshness story for pinned checkpoints need the key infrastructure no pith record has shipped. until one exists, the policy leg trusts configuration, and a freeze imposed by a stale pinned checkpoint is a failure mode the client cannot see. consistency proofs and gossip — comparing checkpoints across machines and time, the mechanism by which go's clients catch a forked log — are unwritten.

a network client is out of scope by construction: the registry adapter reads a directory, and transport, mirroring, and partial download are a future adapter's own questions. the local directory stands in for whatever the transport produces.

index retention and caching. every resolution re-reads the index. whether a snapshot is worth persisting as an artifact, on which of 0027's axes, is a workload question the prototype does not generate.

the local index carries no dependency edges — one line per version, no requirements — so transitive resolution over fetched sources is designed for by 0040 and unmeasured here. the first index format that carries requirements measures whether the candidate record the universe spells is the one a real registry answers with.

mirror choice and multi-registry universes. one read names one registry. whether a caller composes several registries into one universe, and what the origin of a composed candidate then is, has no measured need yet.

## sources consulted

- [go checksum database design](https://go.googlesource.com/proposal/+/master/design/25530-sumdb.md) and [the transparent-log design under it](https://research.swtch.com/tlog)
- [the update framework specification](https://theupdateframework.github.io/specification/latest/)
- [sigstore documentation](https://docs.sigstore.dev/)
- [cargo's registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- [nix advanced attributes (fixed-output derivations)](https://nix.dev/manual/nix/stable/language/advanced-attributes) and [the sandbox setting that exempts them](https://nix.dev/manual/nix/stable/command-ref/conf-file)
- [debian repository format](https://wiki.debian.org/DebianRepository/Format)
- [git internals: git objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)
