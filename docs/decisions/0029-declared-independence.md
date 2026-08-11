---
schema: design-doc/v1
id: decision-0029-declared-independence
title: independence is declared by the rule body, not inferred by the scheduler
summary: a rule that wants two results concurrently says so with one batched step; the engine never guesses that sequentially requested work is independent, so the concurrency it exploits is exactly the concurrency a rule authorized
kind: decision
status: proposed
created: 2026-05-21
updated: 2026-05-27
tags:
  - engine
  - kernel
  - concurrency
  - effects
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0021-arena-graph-engine
    - decision-0022-sync-core-async-scheduler
  supersedes: []
---

# independence is declared by the rule body, not inferred by the scheduler

> refines [0022: a synchronous pure evaluator core with an async scheduler at the effect boundary](0022-sync-core-async-scheduler.md), whose "prototype evidence" section records that the driver awaited one action at a time and left concurrent scheduling to be prototyped. 0022 stands: the core stayed synchronous and only the scheduler became concurrent. what 0022 did not say, because it did not come up until the scheduler was built, is where the concurrency comes from.

## context

0022 describes a concurrent scheduler over a synchronous pure core, and the engine was built that way: pure steps run on a call stack, and the only `.await` is the executor call. the concurrency half was left as "not yet prototyped."

building it surfaced a gap that reads as an implementation detail and is not one. the step machine yields exactly one thing at a time. `PureStep` had `Need`, `NeedBlob`, `NeedAction`, and `Complete`, each naming a single request; `Value` has no aggregate variant; and the engine's entry point took one root request. a rule body that wanted two results asked for the first, was resumed, then asked for the second.

that shape makes the dependency structure sequential *by construction*. the scheduler cannot overlap the two requests, because at the moment it receives the first it has not been told the second exists — and when it receives the second, the first has already completed. there is no width in the graph for a scheduler to exploit, no matter how the scheduler is written. a work-list, a task pool, and a parked-frame design all schedule the same single chain.

so "make the scheduler concurrent" was not the whole problem. the prior question is how a rule says that two results do not depend on each other.

three sources of that information were available.

**infer it.** watch what a rule requests over its lifetime, remember that the second request did not consume the first result, and run them concurrently next time. this is what a build system does when it has a static dependency graph.

**derive it from types.** make the resumption value a product and let a rule request a tuple, so requesting `(A, B)` is by definition requesting two independent things.

**have the rule declare it.** add a step that carries a batch of requests, whose meaning is "these do not depend on one another."

## proposed decision

the rule body declares independence. `PureStep` gains one variant:

```rust
NeedAll(Box<[Request<Pure>]>)
```

whose contract is that the requests do not depend on one another. the engine evaluates each as its own chain, overlapping their actions, and resumes the body once with every result in the order requested.

resumption becomes a shape rather than a bare value:

```rust
pub enum Resumption {
    One(Value),
    Many(Box<[Value]>),
}
```

`One` answers `Need`, `NeedBlob`, and `NeedAction`; `Many` answers `NeedAll`. a body cannot be resumed with a shape it did not ask for, and the two constructors are the only ones, so the match is exhaustive and compile-checked.

the second source of independence is the caller's, not a rule's: `Engine::run_many` takes a slice of root requests and drives one chain per root. building three targets is three independent roots, which is the multi-target case every build tool has and which no single-root entry point can express.

the sequential reading of `Need` is preserved exactly. a body that yields `Need(a)` and later `Need(b)` still gets `a` before `b` is selected, because the engine cannot know that `b` was not computed *from* `a`. ordering remains the default; independence is the thing you opt into.

### why declaration rather than inference

inference is unsound at the moment it matters. the engine sees a resumption value go into a rule body and cannot see whether the body used it to construct the next request. treating "did not appear to use it" as "did not depend on it" is a guess about the interior of a rule, and the cost of guessing wrong is a request evaluated against a dependency that had not been established — a wrong graph, not a slow one. a scheduler that reorders on a guess also makes the recorded dependency edges a function of scheduling rather than of the rule, which contradicts 0021's premise that the arena graph is what the evaluation actually did.

inference is also unnecessary. the rule body is the one participant that already knows. asking it to say so costs one enum variant.

this is the same argument 0003 makes for effects and 0019 makes for effect categories: the thing that is true about a computation should be visible in its declaration rather than reconstructed by analysis. concurrency is a property of the dependency structure, and the dependency structure is authored, not discovered.

### why not a product-typed resumption

making `Value` carry tuples would give independence for free: `Need` a pair and the pair's components are visibly independent.

rejected on blast radius and on honesty. `Value` is the semantic value domain; it feeds `Type`, the canonical encoding, content digests, the durable result encoding, and the conformance suite. adding an aggregate variant to obtain a *scheduling* property puts a scheduler concern into the type system, and the type calculus (0026) has its own reasons to decide what aggregates mean. `Resumption` is the engine's resume channel, not part of the value domain; it is exactly where a statement about scheduling belongs, and it can change without a semantic encoding version.

### what it does not change

the core stays synchronous. `NeedAll` is served by the same step machine: the engine opens a chain per request, and each chain runs on a call stack with no `.await` in it. the concurrency is between chains, at their park points, which is precisely 0022's claim.

`Pure` gains no ambient async authority. a fan-out of pure requests is still evaluated by the synchronous driver, and `Engine::evaluate_pure` still rejects effectful steps with `E-1206`. what overlaps is actions, and actions were already the only thing behind the async boundary.

cycle detection is unchanged in meaning and wider in scope. a request that reappears below a `NeedAll` is as circular as one that reappears below a `Need`, so the check walks the requesting chain *and* its ancestor chains. a fan-out is not an escape from the cycle rule.

## consequences

`PureRuleFrame::step` changes signature from `Option<Value>` to `Option<Resumption>`. this is a breaking change to the rule-authoring API, taken now while the only rule authors are the repository's own fixtures. `Resumption::one` gives bodies that never fan out a one-call migration.

the engine's evaluator splits along the seam this decision describes: a scheduler that owns the set of in-flight chains and touches no arena state, a synchronous core that runs one chain on the step machine, and a driver that serves the effects a chain stops for. the sync/async boundary 0022 asserts is now a module boundary, which is a place a reviewer can check it.

`AttemptState` and the durable records are untouched. a fan-out child is an ordinary pure computation with an ordinary dependency edge from the frame that requested it; nothing about reuse, provenance, or the conformance suite changes shape.

the executor now sees concurrent calls. `Executor::execute` takes `&self` and was already required to be `Sync`, so this imposes no new bound, but an executor with shared mutable state now has to say what it does about it. the first-party local executor (0028) has none: each action gets its own scratch root.

overlap costs memory. `n` fan-out children mean `n` live chains, and the actions among them are started under a bound (`Engine::action_concurrency`) rather than all at once; see unresolved.

## alternatives considered

### a work-list scheduler over the existing single-request steps

replace the depth-first stack with a work-list and run whatever is ready.

rejected because nothing is ever ready but the one frame at the top. with single-request steps the graph is a path, and a work-list over a path is a stack with extra bookkeeping. this alternative is what the concurrency work looked like before the sequencing problem was noticed, and naming it here is the point: the scheduler was never the constraint.

### futures in the value domain

let `NeedAction` return immediately with a handle, and block only when a rule reads it. this is the maximally permissive design: every action overlaps with everything after it, with no annotation.

rejected for this milestone. it puts an unforced deferred value into the semantic domain, it makes the point at which an action's failure surfaces depend on when a rule happened to read the handle, and it interacts with cancellation and with the durable attempt lifecycle in ways that want their own record. it is a reasonable direction for a later milestone and is not foreclosed: `NeedAll` is a strictly weaker construct that it would subsume.

### concurrency only across roots

ship `run_many` and stop. no rule-authoring change.

rejected as insufficient. it gives a build tool concurrency across targets and none within one, which is the wrong way round: a single target's independent compilations are where the parallelism in a build actually is. it is kept, but as one of the two sources rather than the only one.

## unresolved

the width limit is over actions and is now in place. a `NeedAll` of a thousand requests still starts a thousand chains — chains are cheap — but the actions those chains stop for are started `Engine::action_concurrency` at a time, defaulting to the host's available parallelism. a chain that stops for an action joins a queue holding the *request*, so an action waiting for a slot has not been planned, has no computation node, and has materialized nothing; the bound therefore limits live invocations and child processes, which is what costs. the remaining resource limits 0028 defers (timeouts, rlimits, cgroups) are unaffected by this and stay open.

duplicate requests inside one batch each get their own computation. the reusable index dedupes across time, not within a batch, because no attempt has completed while the batch is being prepared. whether the engine should coalesce identical in-flight pure computations — a general "one computation per key in flight" property, which also covers two chains reaching the same request independently — is a real question and a separate one; it needs the same machinery as cancellation, and it is deferred to the record that lands that.

scheduling is first-ready. the driver takes the first action whose executor returned and resumes its chain, with no priority, no fairness guarantee, and no critical-path awareness. this is the right starting position — it has no tuning parameters to get wrong — and the M-3 build library is what will say whether a real workload needs more.

cancellation is not in this record. `NeedAll` makes it reachable to have several actions in flight when one fails, and today the driver's answer is to record the survivors as failed and drop their futures, which does not stop the child processes they started. the cancellation work 0022 leaves open now has a concrete shape to attach to, and takes it.
