---
schema: design-doc/v1
id: decision-0060-observation-identity-and-freshness
title: observation identity is the request and derived subject; freshness is observer identity plus an attested revision
summary: an observation rule derives a subject from typed inputs, an observer returns a value and revision, and a recorded observation edge admits its pure consumer only after the same observer attests the same revision
kind: decision
status: proposed
created: 2026-08-21
updated: 2026-08-21
tags:
  - observations
  - identity
  - freshness
  - evaluation
  - engine-state
relations:
  informed_by:
    - decision-0012-revision-pinned-plans
  depends_on:
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0022-sync-core-async-scheduler
    - decision-0023-rule-and-cache-identity
    - decision-0031-action-cache-identity
    - decision-0033-consumer-of-action-reuse
    - decision-0051-transitive-revalidation
    - decision-0059-a-caller-declared-run-bound
  amends:
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0038-represented-rule-bodies
  supersedes: []
---

# observation identity is the request and derived subject; freshness is observer identity plus an attested revision

> amends 0019 by making `Observation` the third operational effect category, and 0038 by adding `NeedObservation` to the step vocabulary the represented ir will encode. answers M-9's question: an observation result is not entered into a durable reusable-result index, while a pure consumer of one can be reused after the recorded observation edge is re-attested.

## context

`Observation::CACHEABLE_AS_RESULT` is false. before this record that meant more than "do not blindly serve yesterday's read": an observation had no computation identity, so an edge could not name what was read, and a pure attempt above any observation had to stay permanently outside reuse. decision 0012 already supplied the world half — a revision is whatever an adapter can attest and the engine checks equality — but not the request half, the adapter boundary, or the admission rule.

0031 and 0033 provide the nearby construction. an action has a request-side key and an execution-side admission test; its pure consumer records the action edge and revalidates it rather than folding the effect into the consumer's key. observation needs the same split without inheriting action semantics. there is no declared executable contract, authorization decision, output import, or result cache. there is a subject in an external world and an observer able to say whether that subject is still at the revision it read.

M-11 fixes the represented body's constructor set. leaving observation as a marker until M-5b would add a yield constructor after that encoding was fixed and move every represented rule revision. M-9 therefore takes the protocol now and exercises it with the cheapest real external fact, a file modification time.

## proposed decision

### request identity

an observation rule is selected by exact interface like a pure or action rule. its deterministic body derives a typed `subject` from the request inputs. `ObservationComputationKey` commits to:

- selected rule identity and revision;
- request interface and typed inputs; and
- the derived subject.

the digest has its own `pith:observation-computation:v1` domain. labels and spans remain diagnostic context and do not enter identity. the subject is retained in the durable computation beside the request material, so an adapter and the publication validator can reproduce the key rather than trust a stored digest.

the subject is deliberately a `Value`, not a kernel path or resource identifier. a file observer may use text naming a path; a deployment adapter may use a declared record naming a managed object. the kernel compares and records the value without interpreting the domain.

### world identity and freshness

the host supplies one `Observer`. its identity names the semantics of its attestations. `observe(subject, bound)` returns the value delivered to the requesting body and a revision value; `attest(subject, bound)` returns the current revision without needing to reproduce the observed value. both receive the caller's run bound on the same rule as an executor: the adapter holding the external operation is responsible for honoring the deadline.

an observation attempt records the intended observer when it becomes pending and records either `NotObserved` or `Observed { observer, revision }` as provenance. publication rejects provenance naming another observer. revisions are typed values because different worlds have different honest tokens — an etag, generation, resource version, content digest, or timestamp — and the engine needs only canonical equality.

a recorded observation edge is valid when all of these hold:

1. the current engine has an observer with the recorded identity;
2. the observation rule re-selects from the retained interface;
3. deriving its subject from the retained inputs reproduces the recorded subject and computation key; and
4. `attest` returns a revision equal to the recorded revision.

failure of an identity or equality test is a cache miss. an adapter diagnostic is an error, not a miss, on 0024's rule that inability to read state must not silently mean recompute. the revalidation is transitive on 0051's walk, so a pure root several edges above an observation is admitted only after the observation at the bottom is fresh.

### scheduling and reuse

`PureStep::NeedObservation(Request<Observation>)` pauses a chain. the run driver serves it asynchronously and resumes the existing frame with `Resumption::One`; no observation-specific resumption constructor is needed. `evaluate_pure` rejects the step with `E-1206`, preserving 0022's synchronous pure-only evaluator. a run without an observer refuses with `E-1217`.

async attestation means a run has an async reuse path as well as the existing synchronous one. roots and nested pure requests opened by `Engine::run` may attest observation edges before reuse or hydration. the synchronous path used by `evaluate_pure` declines those edges and never awaits. action admission remains unchanged.

an observation completion may be reused live only after the same attestation test, but it is never published into a durable reusable-result index. the durable attempt exists so consumers can name and revalidate it. the reusable unit across process boundaries is the pure consumer whose result was derived from that revision, not an observation lookup detached from its consumer.

## alternatives considered

### put the revision in the observation computation key

the key becomes request, subject, and observed revision.

rejected because the revision is unknowable before the effect runs. it would make lookup require observing first, after which the cache has saved no external work. it also conflates 0023's computation identity with 0012's admission fact.

### time-to-live freshness

retain the result for a duration and skip attestation until it expires.

rejected as kernel semantics. a ttl is policy over a source whose revision semantics are already available, and two consumers may require different staleness tolerances. more importantly, elapsed time cannot prove that an external object did not change twice inside the window. an adapter may encode time in its revision when time is the honest source fact; the engine does not invent it.

### make every observation consumer non-reusable

always observe again and recompute every pure ancestor.

correct but rejects incremental computation exactly where deployment planning needs it. it also throws away equality pruning even when the external platform supplies an exact etag or generation. the recorded edge and attestation are sufficient to admit the consumer without putting the observation itself in the result index.

### model a read-only action

declare observation as an action with no outputs.

rejected on 0019's distinction. action reuse asks whether a bounded execution under a contract may stand in for running again. observation reuse asks whether an external subject is still at one revision. executor identity, confinement and imported content do not answer freshness, while an observer revision does not prove action determinism.

## consequences

`Observation` is operational through public `ObservationRule`, `Observer`, `Observed`, and `ObserverIdentity` APIs. the graph gains observation nodes and edges; durable records and both state adapters retain request inputs, subject, intended observer, result and revision. SQLite's pre-release schema grows nullable observation columns and an input table, with no version move under 0048.

pure attempts above observations may enter the reusable index. every admission pays the observer's attestation cost, including cold hydration, and the observer adapter determines whether that is cheaper than observing. two independent consumers of the same live observation may share its value only after attestation; there is no freshness inference from process lifetime.

the constructor set M-11 inherits now includes `NeedObservation`. mutation and opaque remain marker categories, so 0019 stays proposed.

## prototype evidence

`crates/pith-engine/tests/observation.rs` is the thin file-mtime prototype. its observer derives a revision from filesystem metadata and its observation rule derives the path subject from a typed request. the tests assert a cold observation, live root reuse after attestation, cross-engine hydration over shared durable state after attestation, recomputation and re-observation when mtime changes, recomputation when observer identity changes, `E-1206` on the pure-only entry point, and `E-1217` when a run has no observer.

the durable-state conformance generator creates observation computations, publishes observed and not-observed provenance, and routes observation dependency edges through both adapters. `crates/pith-state-sqlite/tests/durable_engine_state.rs` separately round-trips an observation's request, subject, intended observer and revision. key tests assert subject sensitivity and digest-domain separation.

## unresolved

the prototype's subject is a text path and its revision is an mtime. neither is a general filesystem observation contract: path resolution, replacement with an equal mtime, symlinks, clock granularity and content equality all need the first real source or activation adapter to choose what it can honestly attest.

uncertainty, which 0019 includes in `Observation<A>`, is not represented. a failed adapter call is a diagnostic and a successful one is a value plus revision; partial, stale-but-usable and confidence-bearing observations need a typed value construction and a consumer before the kernel should add one.

revision pins on a derived plan remain 0012's unbuilt projection. this record makes every observation revision available in the recorded graph and validates it for cache admission; extracting a pin set into a plan value and re-attesting it immediately before mutation belongs to M-5b and M-6.

the observer receives the wall-clock bound but no cancellation signal, matching the executor boundary today. partial cancellation remains 0022 and 0059's open scheduler question.
