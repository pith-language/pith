---
schema: design-doc/v1
id: decision-0031-action-cache-identity
title: an action is identified by its request and admitted by its execution facts
summary: split action cache identity into a request-side key that indexes a completed attempt and an execution-side admission test that decides whether the recorded attempt may be served to this run
kind: decision
status: proposed
created: 2026-05-27
updated: 2026-05-27
tags:
  - actions
  - caching
  - identity
  - incremental
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0014-reproducibility-properties
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0028-sandboxed-local-executor
    - decision-0030-toolchain-closure-as-declared-input
  supersedes: []
---

# an action is identified by its request and admitted by its execution facts

> completes the action half of [0023: separate stable rule identity from cache-invalidating rule revision](0023-rule-and-cache-identity.md), which built a computation key for pure applications and left actions uncached. carries the reuse and revalidation model of [0024: persist content in a filesystem cas and engine state in sqlite](0024-persistent-engine-state.md) across the effect boundary.

## context

`Engine::finish_action` records every completed action `NotReusable(ActionCachingDisabled)`. That was the honest position while action identity was undefined, and both engine-state adapters, the conformance suite, and the invalidation-explanation chain carry the reason as a first-class value. Every milestone note since M-2 has named the same condition for lifting it: action identity must cover the resolved platform and the complete execution semantics. M-3 cannot show a fine-grained rebuild until it is lifted.

That condition, read literally, cannot be met. A pure computation key is derivable from the request alone, so for pure results one test does both jobs: compute the key, find the attempt, revalidate its dependencies. Action identity has two halves, and they become knowable at different times.

The request half is the selected rule's identity and revision, the requested interface, the request inputs, and the digest of the contract the rule planned from them. All of it is known before anything executes.

The execution half is the platform the executor resolved, the confinement it installed, the capabilities it used, and the content it produced. All of it is knowable only after an executor returns, and it describes the environment the run happened in.

Keying the index on the execution half is circular. The engine would have to run the action to derive the key that would have told it to skip the action.

## proposed decision

Action cache identity is the request half. The execution half becomes an admission test, applied to a recorded attempt when reuse is considered.

### the key

`ActionComputationKey` mirrors `PureComputationKey`: the selected rule's `RuleIdentity` and `RuleRevision`, plus an `ActionComputationDigest` over the rule identity, the rule revision, the requested interface, the request inputs, and the `ActionSpecDigest` of the planned contract. The digest is domain-separated as `pith:action-computation:v1`, alongside the other prefixes in `pith-ids`.

Both the request inputs and the spec digest are committed to. An action rule body is two functions: `plan` maps request inputs to a contract, and `complete` maps request inputs and an execution to a result. The spec digest alone cannot distinguish two requests that plan one contract and complete to different results. The request inputs alone leave the planned contract resting on the rule revision having been bumped, which 0023 requires but nothing verifies. Committing to both means the key names the whole rule application.

The spec digest already covers the executable path, the declared toolchain closure (0030), the arguments, the declared inputs *by content identity*, the declared outputs, the environment, the platform requirement, the capability requirements, and the network policy. The key restates none of it.

### the index

`EngineStateStore` gains `latest_completed_reusable_action_attempt`, the action mirror of `latest_completed_reusable_attempt`. The attempt stays the unit of durable record, and the reusable index gains action entries. Both adapters implement it, and the cross-adapter conformance suite holds them to each other as it already does for the pure index.

### the admission test

An attempt found under the key is served only if all of these hold when reuse is considered:

- its recorded dependency set revalidates, by the same walk pure reuse uses
- the current run's policy authorizes the freshly planned contract. Policy is reapplied on reuse, which is the rule every milestone note has stated since action policy landed
- the recorded execution's resolved platform satisfies the planned contract's `PlatformRequirement` and matches the platform this run would execute on
- the recorded `AccessVerification` is at least the level this run demands
- every content identity in the recorded outputs is still present in the content store

A failure of any of these is a miss, and the action runs. None of them is a fault.

The admission test is kept off the attempt. A `DurableReuseDecision` answers whether an attempt is a candidate for reuse at all, which follows from the attempt and its dependencies and is fixed when it is published. Whether a candidate may be served answers whether this environment matches the one it was recorded in, which holds only relative to a run. Writing the second onto the attempt would make a stored record change meaning when the host changes underneath it. An action attempt therefore records the same reuse decisions a pure attempt records, and admission is a typed answer the engine returns from the lookup.

### why confinement is an admission floor and not part of the key

A result produced under weak confinement is a weaker claim than the same result produced under strong confinement, which makes `AccessVerification` look like key material. It partitions the index: a run demanding `Prevented` misses the entry recorded under `Observed` and writes a second entry beside it, the index accumulates one entry per confinement level per action, and the weaker entry still gets served to a weaker demand. As an admission floor there is one entry per request, and the demand decides whether it is good enough.

It also decouples the cache from the seccomp increment 0030 leaves open. When seccomp lands and the local executor moves from `Observed` to `Prevented`, existing entries stay findable and become unacceptable to callers who ask for `Prevented`.

### what reuse claims

Serving a cached action asserts that this contract already ran in this environment under at least these conditions and produced this content. It does not assert that re-running it would produce the same bytes. 0014 keeps determinism separate from confinement, and this record keeps it separate from reuse. An action recorded under `Unverified` is cacheable at `Unverified`, to a caller who accepts that.

## alternatives considered

### key the index on the execution facts

Include the resolved platform and access verification in the digest, so identity covers execution semantics in the literal sense the earlier notes stated.

Rejected as circular. Both facts are produced by the executor, and the index is consulted before an executor is invoked. Computing such a key means running the action, at which point the lookup has no purpose. The reading is salvageable only by moving those facts out of the key and into admission, which is this record.

### content-address the action by its outputs

Index by what the action produced, the way a content-addressed derivation store does.

Rejected for this increment. An output-keyed index answers which run produced a given set of bytes, which is useful for sharing and deduplication, and it cannot answer whether this run may skip an execution, because the outputs are unknown until the execution happens. The input-addressed versus content-addressed question belongs to 0020's store-adapter boundary and 0024's remote-cache trust, both still open. Nothing here forecloses either.

### cache nothing at the action, only the pure results downstream

Leave actions always-executing and let the pure computations that consume them carry the incremental win.

Rejected because the cost is in the action. A compile that reruns to produce a `ContentId` its consumer then finds unchanged has already spent the compile. It would also make the equality-based change pruning that exists for pure results dishonest at the effect boundary: the consumer would prune on a value it paid full price to recompute.

### verify by re-running and comparing

Re-execute and check the result matches the recorded one instead of skipping.

That is 0014's rebuild check. It costs the same as not caching, and it answers the determinism question this record leaves alone.

## consequences

### the consumer of an action stays out of the index

An action attempt becomes reusable. A *pure* attempt that depends on one does not.

A pure computation key covers the rule identity, the rule revision, the requested interface, and the inputs. It does not cover the action rule that a `NeedAction` step will select, or that rule's revision. A pure result recorded when the action rule planned one contract could then be served after the action rule was revised to plan a different one: the consumer's key is unchanged, and the recorded dependency edge names an attempt that is still reusable in its own right. Nothing in the recorded graph shows the drift.

The pure-to-pure case is closed, because a pure edge carries the dependency's key and revalidation compares against the latest reusable attempt for that key. An action edge would need the same, plus the request that produced it, and a re-plan to see whether the key still comes out equal — most of the action's front half, run at revalidation time.

The existing invariant therefore stands. A completed pure attempt with an action edge is not reusable, and it now says so with its own reason (`EffectfulDependency`) instead of inheriting one from an action that refused to cache. The incremental win survives: the consumer re-runs its rule body, re-plans the action, computes the same key, and the action is served from the index. What is skipped is the execution.

Closing the gap means carrying the action's identity into its consumer's key, which is the machinery equality-based pruning across the effect boundary also needs. It is follow-up work.

### elsewhere

The state validator changes shape. `ExpectedReuseDecision::ActionCachingDisabled` disappears: an action attempt derives its reuse decision from its dependencies exactly as a pure attempt does, and `ActionCachingDisabled` becomes an always-accepted decision, because refusing to index a result is sound whatever the dependencies say. A pure attempt carrying an action edge is expected to record `EffectfulDependency`.

`ActionCachingDisabled` acquires a meaning it did not have as a placeholder: this engine has action caching switched off, through `Engine::set_action_caching`. Every durable record already carries the reason as a first-class value, so the stored shape is unchanged.

The `Executor` trait gains `identity`, returning the executor's name and the platform it runs on. The engine asks before consulting the index, since an attempt recorded by another executor or on another platform answers a different question. An executor knows both before it has run anything, and adapters build their reports from the same value.

The engine gains an action computation index beside `pure_computations`, and hydrated action nodes: a completed attempt from an earlier engine instance loads into a fresh node carrying the durable identity of the attempt it came from, on the terms 0024 set for pure hydration.

## unresolved

The key commits to the toolchain closure as *paths*, not as bytes. 0030 declares the closure as host paths, so a toolchain rebuilt in place at one path is invisible to the key and a stale result would be served. Under nix the store path contains the input hash, so the path is a faithful proxy for the content and the gap is narrow. For a distribution toolchain at `/usr/bin/cc` it is real. Closing it means content-addressing the closure into a `Tree`, which is the capture 0030 already hands to the build library, at which point the tree's `ContentId` enters the spec digest and this record's shape is unchanged.

Equality-based change pruning for actions is not addressed. The pure path prunes when a recomputed dependency is canonically equal to its predecessor. The action analogue — a re-executed action whose captured outputs content-address to what the previous attempt produced should leave its consumers reusable — needs the same treatment, and it is the same work the consumer-of-an-action gap above needs.

Retention interacts and is not settled. 0027's collector can remove output content that an index entry names. The admission test checks presence, so a collected entry is a miss, and the entry is then garbage the index should drop instead of answering with. Which store leads that ordering is 0027's cross-store question.

Cross-machine reuse is untouched. Everything here is local: the admission test asks whether *this* environment matches what was recorded. Serving an attempt recorded on another machine needs the trust model 0024 leaves open, and the platform half of the admission test is where that question lands.
