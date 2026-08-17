---
schema: design-doc/v1
id: decision-0048-pre-release-version-pinning
title: every version number stays at 1 until the first release, and pre-release incompatibility is a rebuild
summary: one pinning rule for every version byte and record version in the tree, with discard-and-rebuild as the only pre-release compatibility mechanism, the first tag as the trigger that starts them moving, and the post-release rule fixed per class of artifact rather than per constant
kind: decision
status: proposed
created: 2026-07-28
updated: 2026-07-28
tags:
  - persistence
  - identity
  - storage
relations:
  informed_by:
    - research-build-systems
    - research-artifacts-and-trust
  depends_on:
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0025-relational-engine-state
    - decision-0041-the-written-lock
  amends:
    - decision-0026-generic-typed-calculus
    - decision-0047-the-declaration-table
---

# every version number stays at 1 until the first release, and pre-release incompatibility is a rebuild

> generalizes the rule `refactor(state): pin the record encoding version at 1 while nothing is released` (a9f0b8b) applied to one constant, and amends the two records that plan version movement against it: [0026](0026-generic-typed-calculus.md), whose landed-ahead section narrates four bumps that were made and then unwound, and [0047](0047-the-declaration-table.md), which plans two more.

## context

the tree holds three version numbers over the same bytes and they disagree about what a version is for.

`RECORD_ENCODING_VERSION` is at 1 with a doc comment that states the reasoning in full: "pinned at 1 while nothing is released ... a pre-release database is moved aside and rebuilt rather than versioned around." it reached 5 before it said that. `git log -L` over its declaration shows the sequence: 1 at introduction, then 2 for nominal values, 3 for `List`, 4 for `Record`, 5 for `Sum`, then back to 1 in a9f0b8b. 0026 narrates those four bumps in the present tense and was never revised, so the canonical calculus record and the constant now contradict each other, and only the constant is right.

`value_codec`'s byte-level `ENCODING_VERSION` is at 1 and carries no reasoning at all. `action_codec`'s is at 2, with a comment explaining what version 2 means — the program as a tagged sum (0036) and the exit-status contract (0037). that is an honest record of a format change and it is also the only place in the tree where a version number moved and stayed moved, which makes it the one artifact a reader could take as evidence that the versions are live.

0047 then plans two more moves, one of each kind. six records defer an obligation to "the first release": 0024's migration policy, 0025's restatement of it, 0041's lock-format evolution, 0043's environment-record text format, 0047's Dhall rule, and `records.rs`'s own doc comment. nothing in the repository defines the release those six defer to. there are no tags across 230 commits.

so the situation is not that the version numbers are wrong. it is that there is no rule, and in the absence of one each constant grew its own local answer, one of which was applied and unwound without the record that motivated it being corrected.

## proposed decision

### one rule, stated once

every version number in the tree stays at 1 until the first release. that covers `value_codec::ENCODING_VERSION`, `action_codec::ENCODING_VERSION`, `RECORD_ENCODING_VERSION`, and `SchemaVersion` for every engine-state adapter, and it covers any version a later format introduces before the release exists.

pre-release incompatibility is handled by discarding and rebuilding, never by migration. the mechanism already ships and is already asserted: reopening a database whose reported versions do not match moves it aside and rebuilds, which `durable_engine_state::an_incompatible_database_is_moved_aside_and_rebuilt` holds. a content store needs no equivalent because its objects are addressed by the digest of their bytes, so a grammar change makes new identities rather than unreadable old ones — the store's cost of a format change is wasted space, which 0027's collector will own.

the reason a version number stays at 1 rather than moving with each change is that a version exists to protect a reader, and pre-release there is no reader. the four bumps that happened and were unwound are the evidence: they cost four edits, they protected nothing, and unwinding them cost a fifth. a number that carries no information is not free — it is a claim that compatibility is being tracked, made in the one place a reader would check.

### the trigger is the first tag

the versions start moving when the repository has its first tag. this is chosen because it is mechanical, checkable, and already absent — the six records that defer to "the first release" can then name a condition that exists.

before that tag, `publish = false` in `[workspace.package]` makes the claim structural rather than conventional: a `cargo publish` cannot create the downstream reader whose absence the pinning depends on. this is the same move 0028 makes for confinement and 0047 makes for the representation hole — the honest posture is the one the tooling enforces.

### after the tag, the rule is per class, not per constant

the post-release rule differs by what the version gates, and the axis is whether the artifact is cheap to rebuild and whether anyone commits it.

**derived caches** — engine state and the content store. discard-and-rebuild stays legal forever. these hold results the graph can recompute from declared inputs, which is what K-9 means, so a version mismatch has a correct and cheap answer that needs no migration code. the precedent is rustc's incremental cache, which is keyed on compiler identity and discarded wholesale on a mismatch, and which has never shipped a migration in the years it has existed. discarding costs recomputation; a migration costs a code path that runs once and is tested never.

**committed user data** — the written lock (0041) and the environment record (0043). these must migrate, because a user commits them and a second tool reads them. the precedent is `Cargo.lock`, whose format versions move under a documented policy and whose old versions stay readable, for exactly this reason: a lock file that a new cargo cannot read is a broken repository rather than a slow build. 0041's text projection is where that obligation lands, and it is why the lock's version is not merely "pinned like the state store's" — it is pinned on the same grounds today and governed by a different rule tomorrow.

**digest domains** — unchanged, and governed by 0047's rule rather than this one. a domain is never silently re-based; a change to what participates in a digest is a domain bump or an explicit migration. the Dhall 1.17.0 basis change is the precedent 0047 already carries, and this record does not touch it, because a digest domain is not a compatibility version — it is part of an identity.

separating the three now is the point of writing this before the tag. the alternative is deciding it at the moment a release forces it, per constant, which is how the tree arrived at three answers to one question.

## alternatives considered

### bump each version as its format changes

what 0026 did: move the record version with every new constructor, on the reasoning that the retained-value grammar grew.

rejected on its own measured outcome. the four bumps were made, they protected no reader, and a9f0b8b unwound all four — so the experiment ran and its result is in the history. the deeper problem is that the rule cannot be applied honestly pre-release: "would a prior build misread this?" has the answer "there is no prior build" every time, so the bump is a judgement about a hypothetical, and judgements about hypotheticals drift between constants. that drift is exactly what the tree shows, with `action_codec` at 2 and `value_codec` at 1 over changes of the same kind.

### bump only when a change would be misread rather than rejected

keep the numbers at 1 for additive changes, move them when existing bytes change meaning.

this is the correct post-release rule and it is what the record adopts for that phase. rejected as the pre-release rule because it requires the same hypothetical judgement, and because it produces a version history in which some format changes are recorded and others are not — a reader post-release cannot then tell whether version 1 means "the original grammar" or "everything that was additive." pinning at 1 and rebuilding makes version 1 mean one thing: the pre-release grammar, whatever it was, which no released reader ever saw.

### remove the version bytes until the first release

if nothing is versioned, delete the byte and add it back at the tag.

rejected on what the byte does today. `CanonicalReader::read_version` is what makes a foreign or truncated stream a named refusal rather than a misparse, and the tests that hold it — `read_version_rejects_a_mismatched_byte`, `an_unsupported_version_is_rejected` — are testing the decoder's refusal, not compatibility tracking. adding a version field to a format that lacks one is also the migration this record exists to avoid, paid at the worst moment. the byte stays; what this record fixes is what the number in it means.

### derive the version from a digest of the grammar

make the version a digest over the type and value constructor set, so it moves exactly when the grammar does and never otherwise.

rejected on the Dhall precedent 0047 already cites. a digest over a representation couples the version to the elaborator's internal shape, so a formatting or reordering change that no reader can observe still moves the number — which is the failure mode the 1.17.0 basis change records, and which 0047 spends a section preventing for declarations. a version is a statement about compatibility, which is an author's claim; a digest cannot express a claim its input does not carry.

## consequences

`action_codec::ENCODING_VERSION` returns from 2 to 1, and the comment explaining what version 2 meant becomes the pinning comment the other two carry. this has a property worth naming, because it is the one thing a reset could get wrong: a stream written by the previous build begins with byte 2, and a decoder expecting 1 refuses it as `UnsupportedVersion { version: 2 }` rather than misreading it. the reset is safe in the direction that matters, and `an_unsupported_version_is_rejected` now exercises the version the tree previously emitted.

all three constants carry the same doc comment shape, naming this record. `RECORD_ENCODING_VERSION`'s existing comment is the model the other two adopt, so the reasoning lives in one place per constant and the argument lives here.

0026's landed-ahead section is corrected: its four bump sentences describe history that was unwound, and it now says so and points here. this is the second thing 0026 asserts in the present tense that is no longer true, the first being the constructor set 0047 shrank — both are amendments to a record that describes a calculus still landing, and both stay as pointers rather than rewrites, on 0011's terms.

0047's serialization section loses its two bumps. the round it describes still moves every computation key, because the canonical encodings change shape; what changes is that the pre-release database is discarded on the version mismatch it would otherwise have announced, and 0047's own "moved aside and rebuilt" sentence is the mechanism, now the only one.

`publish = false` moves to `[workspace.package]` with `publish.workspace = true` on all thirteen manifests, replacing xtask's lone literal, so the guard has one source of truth. the twelve library crates were publishable at 0.1.0 before this.

### measured

the workspace suite passes with `action_codec::ENCODING_VERSION` at 1: 657 tests, 0 failures. `an_unsupported_version_is_rejected` refuses byte 2, and `cargo metadata` reports `publish = []` for all thirteen crates, so the guard is resolved by cargo rather than asserted in prose.

## unresolved

what the first release is, beyond the tag that triggers this rule. the crates' own semver policy across twelve published artifacts, whether they version together or apart, and what a breaking change to `pith-core` obliges of `xylem` are the release record's questions, not this one's. U-4 ("public meanings have versions and migrations") is the requirement that owns them and it still has no mechanism.

whether the written lock's text format needs a version line separate from the record encoding, which 0041 leaves open and which this record only classifies. the lock carries a version line today; whether its number is the same number as the record encoding's, or an independent one on the committed-data schedule, is the lock's own question.

how a digest domain bump and a format version interact when both move for one change. this record keeps them separate and 0047 governs the domains, but the case where a grammar change alters both what is encoded and what is digested has no worked example yet, because pre-release neither number moves.
