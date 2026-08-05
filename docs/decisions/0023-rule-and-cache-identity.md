---
schema: design-doc/v1
id: decision-0023-rule-and-cache-identity
title: separate stable rule identity from cache-invalidating rule revision
summary: name a rule independently from the revision of its executable semantics and use the revision in versioned computation keys
kind: decision
status: proposed
created: 2026-05-04
updated: 2026-05-04
tags:
  - identity
  - caching
  - rules
relations:
  informed_by:
    - research-build-systems
    - research-artifacts-and-trust
  depends_on:
    - decision-0005-separate-identities
    - decision-0015-interface-rule-selection
    - decision-0021-arena-graph-engine
  supersedes: []
---

# separate stable rule identity from cache-invalidating rule revision

## context

a persistent computation key has to answer two different questions. provenance needs to know which semantic rule a computation used across compatible refactors. cache validation needs to know whether the executable semantics of that rule may have changed.

one identifier cannot safely answer both. deriving identity from a display label makes diagnostic wording invalidate cache entries. accepting a caller-controlled stable name as the complete cache identity has the opposite failure: changed implementations can reuse results produced by older semantics when the name is retained.

the M-1 prototype therefore uses a deliberately provisional identity derived from rule metadata. M-2 cannot persist or reuse computations safely until the two meanings are separated.

## proposed decision

every rule has a stable `RuleIdentity` and a cache-invalidating `RuleRevision`.

`RuleIdentity` names the semantic declaration. it is derived from a module identity and a declaration identity. display labels, source spans, load order, and arena indices do not participate. a compatible refactor can retain the identity; replacing the rule with a different semantic declaration requires a new identity or an explicit future migration.

`RuleRevision` names one revision of the rule's executable semantics. it is derived from the rule identity and a canonical revision manifest. any change that may affect a result produces a different revision. false invalidation is acceptable; reuse across a possible semantic change is not.

pure computation identity is a versioned, domain-separated digest over:

- rule identity
- rule revision
- the requested interface
- canonical typed inputs

diagnostic request labels and source spans are excluded. changing a rule revision never automatically reuses results from the previous revision, even when observed outputs happen to be equal. equality-based change pruning may stop downstream invalidation after the new revision has been evaluated.

## revision construction

the rust-hosted prototype cannot derive the semantics of an arbitrary trait implementation. its revision manifest therefore includes a conservative revision of the providing crate or executable artifact plus the rule's implementation-local revision data. changing or rebuilding that provider may invalidate more work than necessary, but cannot silently retain work across an unknown implementation change.

future pith-language rules derive their revision manifests from canonical typed semantic ir, the semantic revisions of imported modules, and the evaluator abi version. source formatting, file location, and diagnostic-only metadata do not participate.

the manifest format is owned by the producer, but the kernel's digest construction is versioned and domain separated. changing canonical encoding or evaluator semantics requires a new digest domain or an explicit migration.

## action cache identity

`ActionSpecDigest` continues to identify only a validated declared action contract. it is not by itself an action computation or cache identity.

action planning is a pure rule application and uses rule identity, rule revision, and typed inputs. a future action execution key additionally includes the validated action contract, resolved platform, and every execution property that may affect outputs. executor identity participates until executor equivalence is established for the relevant contract.

authorization evidence is retained separately from output identity. a cache hit is authorized again under the current policy. a policy-derived restriction participates in execution identity only when it changes the actual execution contract. this decision does not enable action caching; it defines prerequisites for the later implementation.

## alternatives considered

### stable caller-controlled name only

the caller supplies a stable name and the cache uses it directly.

this survives refactors but cannot detect an implementation change under the same name. it is unsuitable as a cache revision.

### implementation digest only

the cache and provenance use a digest of executable implementation data.

this invalidates safely but loses stable semantic continuity across compatible refactors and rebuilds. provenance needs a separate stable identity.

### observed output identity as the revision

reuse continues until a recomputation produces different output.

the engine has to recompute before it knows the output, so this is change pruning rather than cache identity. it cannot validate a persistent cache entry before execution.

### manual semantic version only

authors increment a version when behavior changes.

this is simple and too easy to forget. an explicit implementation-local version can be one revision-manifest input, but it is not the only input for rust-hosted code.

## consequences

rules carry more identity data at registration. display labels can change without invalidating computations, while revision changes invalidate them deterministically.

rust-hosted revisions begin conservatively. cache precision improves later when rule bodies are represented as canonical pith semantic ir, without changing the distinction between identity and revision.

persistent graph records store both values. arena-local `RuleId` remains an in-process handle and is never a durable identity.

action caching remains disabled until resolved-platform and execution-semantics identity are implemented and policy is reapplied on reuse.

## unresolved

the module identity and compatibility model across repositories and released versions remains part of the future module-system work. the prototype accepts an explicit module identity at its rust registration boundary.

the exact provider-artifact revision manifest used by first-party rust libraries needs the M-3 build prototype. until then, test and host integrations supply conservative explicit revision data.

