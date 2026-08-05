---
schema: design-doc/v1
id: decision-0022-sync-core-async-scheduler
title: a synchronous pure evaluator core with an async scheduler at the effect boundary
summary: the Pure fragment of decision 0019 evaluates on a synchronous deterministic call stack; the scheduler that drives Action, Observation, Mutation, and Opaque is async, concurrent, and cancellable
kind: decision
status: proposed
created: 2026-04-29
updated: 2026-06-01
tags:
  - effects
  - engine
  - kernel
  - concurrency
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0021-arena-graph-engine
  supersedes: []
---

# a synchronous pure evaluator core with an async scheduler at the effect boundary

## context

decision 0019 fixes five type-level effect categories. `Pure<A>` is terminating by construction (0018); the other four (`Action`, `Observation`, `Mutation`, `Opaque`) perform bounded or unbounded external work and need concurrency, cancellation, and scheduling.

the question this decision resolves is narrower than "is the engine async": it is where the boundary between synchronous and asynchronous sits, and why. decision 0019 does not settle it. the open questions list names the concurrency model as gating the engine prototype alongside the engine representation (now settled by 0021).

two designs were considered. make the entire evaluator async, so every rule body is an `async fn` and even pure computation suspends through the runtime; or keep the pure fragment on a synchronous call stack and let only the scheduler that drives effectful categories be async. the choice is load-bearing for the empty-cache-equivalence invariant (K-9), for the type-level enforcement of the effect categories, and for the runtime's ergonomics.

## proposed decision

### the pure fragment is synchronous

`Pure<A>` rule bodies evaluate on a synchronous call stack. they do not suspend through an async runtime, do not carry `Send` or `Sync` bounds, and do not participate in waker machinery. the empty-cache-equivalence invariant (K-9) is reasoned about on this deterministic call stack, where it is local and structural.

the pure evaluator is a step machine over the arena graph (0021), not a recursive function. each step either performs bounded deterministic work, yields a `Request` to the engine, or completes with a value. this gives the cycle-detection hook (0018) and the depth backstop on impure paths without runtime recursion, and it is what lets the scheduler interleave.

### the scheduler is async

when a rule evaluates and hits an `Action`, `Observation`, `Mutation`, or `Opaque`, it yields a `Request` to the engine. the scheduler that receives these requests is async: it runs effectful work concurrently across rules, cancels requests that are no longer needed, and feeds results back to resume the suspended pure step. this is where `tokio` lives (behind a `Runtime` trait, per 0021's adapter discipline), and it is the only place async appears.

### the boundary is the effect category

the sync/async split maps exactly onto the five-category model (0019). `Pure` never touches the async runtime; the other four categories are the only things that do. this is a type-level enforcement of "async is for effects": a `Pure` computation cannot call a function that requires a `Mutation` capability, because the capability requirement is in the type and the async runtime is reachable only through the scheduler that serves those capabilities. the type system does the work a lint could only approximate.

## why not async everywhere

making the pure evaluator async is the smaller-surface option: one execution model, no boundary to maintain, and the ergonomics of `async fn` throughout.

it is rejected for three reasons that compound.

first, the empty-cache-equivalence invariant (K-9) is the kernel's central correctness claim. it is much easier to reason about on a synchronous deterministic call stack than across async suspension points, wakers, and a concurrent runtime whose scheduling the evaluator does not control. a pure evaluator that can suspend at any `await` point is an evaluator whose observed order of side-effect-free work is partly determined by the runtime, which is exactly what the invariant says must not happen. keeping pure evaluation synchronous makes the invariant local to the step machine rather than a property argued across the runtime.

second, async-everywhere adds `Send` and `Sync` bounds to code that has no reason to cross threads. pure computation is deterministic and terminating (0018); threading async bounds through it is noise that makes the code harder to read without buying anything, because pure computation does not need to be concurrent with itself. the cost is paid everywhere; the benefit is concentrated at the effect boundary, where this decision puts the async runtime instead.

third, the type-level effect enforcement (0019) is stronger when the async runtime is reachable only through the scheduler. if every rule body is `async fn`, the runtime is ambient and the distinction between `Pure` and the effectful categories is a type-system fact the runtime does not enforce. keeping `Pure` synchronous makes the runtime physically unreachable from pure code, which is what "type-level effect category" ought to mean: not just a tag in the type, but a structural inability to perform the other categories' work.

## why not fully synchronous

a fully synchronous engine, with concurrency via threads and channels and no async runtime, is simpler in the type system and avoids the tokio dependency entirely.

it is rejected because the effectful categories genuinely need concurrent scheduling with cancellation. a build that requests five `Action`s wants them to run in parallel; a request superseded by a later one wants to be cancelled rather than completed; a remote fetch wants to time out. threads and channels can express this, but async with structured concurrency expresses it more cheaply and with less error-prone manual lifecycle management, which is why every modern build and deployment tool that has chosen recently has chosen async. the scheduler is the right place for that machinery; the pure core is not the place to avoid it.

## interaction with decision 0019

this decision refines 0019; it does not amend it. 0019 fixes the five categories as distinct types and their caching, scheduling, and authority contracts. this decision fixes where each category executes: `Pure` on a synchronous step machine, the other four on an async scheduler reached by yielding a `Request`. the category contracts in 0019 are unchanged; this decision adds the execution-site fact that 0019 left open.

the closure rule (0019) is unaffected. a hypothetical sixth category would be evaluated on the scheduler, like the four existing effectful categories, unless a later decision argued otherwise.

## interaction with decision 0021

the step machine is the engine's own code over the arena graph (0021). a step that yields a `Request` produces a node and edges in the graph: the request, the capability it requires, the rule that made it. when the scheduler resumes the step with a result, the graph records the dependency. the async scheduler is thus a producer of graph structure, not a separate system running alongside it; the dependency edges and provenance that 0021 describes are recorded by the scheduler's activity.

the `Runtime` trait that wraps `tokio` (0021's adapter discipline) is what makes the async runtime replaceable. a host that embeds the kernel in an `async-std` or custom-runtime process implements the trait; the engine and the pure core never name `tokio`.

## alternatives considered

### async everywhere

every rule body is `async fn`; the runtime is ambient.

smallest surface and no boundary to maintain. rejected because empty-cache equivalence is harder to reason about across async suspension points, because `Send`/`Sync` noise pervades deterministic code that does not need it, and because ambient async weakens the type-level effect enforcement that is the point of 0019.

### fully synchronous, threads and channels

no async runtime; concurrency via threads.

simplest types and no tokio dependency. rejected because the effectful categories need structured concurrency with cancellation and timeout, which threads and channels express more expensively and more error-pronely than async. the cost is concentrated at the scheduler, which is where async is put instead.

### sync pure core, sync scheduler with a thread pool

keep the pure core synchronous but also make the scheduler synchronous, using a thread pool for effectful concurrency.

avoids the tokio dependency while still parallelizing effectful work. rejected because cancellation, timeout, and structured concurrency across many in-flight `Request`s are the scheduler's core job, and reimplementing them by hand on a thread pool recapitulates what an async runtime provides. the async runtime is behind a trait and replaceable; a hand-rolled thread-pool scheduler would be neither.

## consequences

the kernel has two execution sites with different rules. the pure core is synchronous, deterministic, terminating by construction (0018), and reasons locally about empty-cache equivalence (K-9). the scheduler is async, concurrent, cancellable, and is the only place the async runtime appears.

the async runtime is behind a `Runtime` trait and reachable only through the scheduler. library code never names `tokio` in a public signature. embedding the kernel in a non-tokio host is implementing the trait.

the type-level effect categories (0019) are enforced structurally by where each category can execute. `Pure` cannot reach the async runtime; the effectful categories cannot run on the synchronous step machine. this is stronger than a lint and stronger than a type tag.

the cost is the boundary itself. a rule that evaluates purely and then hits an effect must yield a `Request` and be resumed, rather than calling the effect directly. this is the standard cost of a structured effect boundary and is accepted for the same reason the effect categories are types rather than tags: the visibility is the point.

## prototype evidence

the first rust prototype establishes the boundary but does not yet establish the complete scheduler described by this decision.

`Pure` rule bodies run as synchronous frames and yield typed requests. an `Action` rule plans an inert contract containing its executable content identity, arguments, inputs, outputs, environment, platform requirement, capabilities, and network policy. callers can query that plan and its stable contract digest without executing it. every scheduler run receives an explicit action policy. the engine gives the complete plan to that policy and records its named allow or deny decision before invoking the executor; denial returns a stable diagnostic and the executor is not called. before execution, the engine resolves the declared executable and inputs from its content store and passes only that materialized view to the executor. an async executor adapter is the only surface that performs an authorized action. it returns captured output bytes or trees plus a report containing the selected platform, access-verification mechanism, and capability use. the engine imports the captured outputs into its store, validates the resulting report against the contract, and retains it as provenance; the action rule derives the typed semantic result from those engine-owned content identities. the engine does not treat either policy approval or an executor's claim as sufficient proof for cache reuse.

the current driver awaits one action at a time. it does not yet schedule independent work concurrently, cancel superseded requests, or enforce timeouts. the executor interface makes those additions possible without returning ambient async authority to pure rules, but the decision remains proposed until those scheduler properties are prototyped.

## unresolved

the exact shape of the `Request` value a step yields, and how resumption carries the result and any new graph edges back into the step machine, needs a prototype. the design here fixes the boundary; the message format is open.

how cancellation propagates from the scheduler into a suspended pure step, and whether a cancelled step can leave partial graph structure that must be collected or can be discarded with its arena frame, needs the prototype to answer.

whether the step machine's depth backstop (0018) is best expressed as a step count, a recursion depth, or a fuel mechanism, and how it interacts with the scheduler's right to cancel, is open.
