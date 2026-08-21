---
schema: design-doc/v1
id: decision-0059-a-caller-declared-run-bound
title: a caller-declared run bound, one wall clock and one step budget, enforced at the scheduling boundaries, inside the step machine, and in the child
summary: the backstop limit five records and three milestones were each deferring to the next round, as one mechanism; the caller declares a run's deadline and step budget, the deadline descends into every action invocation, and the terminal states keep a bound stop distinguishable from a fault and a timeout kill distinguishable from a crash
kind: decision
status: proposed
created: 2026-08-21
updated: 2026-08-21
tags:
  - evaluation
  - actions
  - executor
  - termination
  - engine
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0022-sync-core-async-scheduler
    - decision-0028-sandboxed-local-executor
    - decision-0031-action-cache-identity
    - decision-0037-exit-status-as-a-declared-outcome
    - decision-0050-cycle-detection-over-the-computation-key
  amends:
    - decision-0028-sandboxed-local-executor
  supersedes: []
---

# a caller-declared run bound, one wall clock and one step budget, enforced at the scheduling boundaries, inside the step machine, and in the child

> amends [0028: a first-party sandboxed local executor using landlock and seccomp](0028-sandboxed-local-executor.md) by retracting one sentence of its unresolved section — "the executor accepts an optional timeout in its configuration" described a facility nobody built, and this record builds the facility and the retraction both. answers the two questions 0018 and 0022 left pinned to rounds that never took them: how the impure backstop is configured, and what form the step machine's bound takes. settles the case 0037 named and left to "the timeout design": a child killed at its deadline must never reach a `Reported` rule as a signal death.

## context

five callers were owed this mechanism, and each was deferring it to the next round. 0018 requires a backstop on the impure paths and leaves its configuration open "to the action prototype milestone"; that milestone closed without it, and its evidence paragraph has carried the debt since. 0022 states its own promotion condition — "timeouts are still not prototyped" — and asks whether the step machine's bound is "a step count, a recursion depth, or a fuel mechanism." 0051 asks who bounds a revalidation walk that is "paid to decide *not* to work." 0037 asks how a timeout kill stays distinguishable from a crash under `ExitStatusContract::Reported`. and 0028's unresolved section claims "the executor accepts an optional timeout in its configuration," which is the older kind of debt: a record describing a facility nobody built.

the tree agrees with all five: no `Duration`, timeout, or deadline anywhere under `crates/*/src`. three milestones ahead want one — M-5a's image tools, M-6's mutations, and the configure-script case M-15 exists for, where a hung probe program is unbounded.

## proposed decision

### one mechanism, declared by the caller

a run may declare a bound: [`RunBound`](../../crates/pith-engine/src/bound.rs), carrying a wall-clock deadline and a step budget. `Engine::run_bounded` and `Engine::run_many_bounded` take it; every existing entry point passes an unbounded one, so a run without a bound behaves exactly as runs did before. there is no default bound, and that is a decision rather than an omission: no number is both generous enough for a real build — compiles run minutes — and small enough to stop a runaway. a defaulted bound either wedges the patient or kills the slow, and which one it does is a property of the workload, not of the default. the caller that knows its workload declares its patience.

### where each half is enforced

the deadline is polled at the scheduling boundaries where cancellation is already polled, which is 0022's own answer to 0018's "how does it interact with cancellation": a bound stop is a stop instruction the caller supplied in advance, with its own code, not a fault discovered mid-run.

the step budget is spent inside the step machine, one unit per step, because the boundary is not enough. a body that yields an unbounded sequence of *distinct* requests steps many times between two boundaries, and 0050's cycle predicate cannot refuse it — a predicate "refuses a request that repeats; it does not bound a body that yields an unbounded sequence of distinct requests." that shape is the reason the budget is a step count rather than a recursion depth or per-frame fuel: a depth counts what a single chain holds, a per-frame fuel composes badly across a fan-out, and what needs bounding is the run's total stepping, which is what the run declared. this is 0022's open question answered.

the deadline descends into every action the run starts: `ActionInvocation` carries it, and the executor that holds a child enforces the clock by killing the child and refusing with the bound's code. the engine has no timer of its own for this, deliberately — an engine-side timer racing the executor future would name the async runtime in engine code, against the same discipline that keeps it out of evaluation signatures — and it does not need one, because a first-party executor wakes the driver at the deadline by returning. the boundary check after the await covers an executor that ignores the deadline it was handed; the record states plainly that the wall clock on an action binds only through the executor honoring the invocation.

### terminal states, and 0037's answer

the bound's code is `E-1216`, beside `E-1215`. the split it governs:

work the run was merely holding when the bound fired — parked chains, actions killed mid-flight — is recorded **cancelled**, not failed, exactly as caller cancellation records it. nothing about that work is known to be wrong, and a larger bound changes the answer; recording it failed would tell a later reader not to bother re-running, which is false.

the action that exceeded its wall clock is the exception, and it is recorded **failed** carrying `E-1216`. it ran and produced nothing within the authority it was given, which is `AttemptState::Failed`'s own definition, and re-running it needs more authority rather than another attempt. the refusal happens inside the executor, before capture: a killed child wrote nothing the declared contract stands behind, so nothing is imported and `complete` never runs. that is 0037's answer — under `Reported`, a timeout never reaches a rule as a signal-death verdict, because the executor refuses first and the diagnostic names the bound, where a crash arrives as a verdict the rule reads.

the deadline is authority for an execution, not content of a request, and so it participates in no computation key (0031's own split, applied): an attempt recorded under a larger bound may be served to a run under a smaller one, because reuse serves a completed attempt without running anything, which is the point of reuse.

the wall clock also covers 0051's walk. `open_roots` — the frontier walk that decides reuse — runs inside the bounded run, and the first scheduling boundary after it checks the clock, so a cold hydration expensive enough to matter is stopped rather than unbounded. the walk is engine-internal and spends no steps, and the clock is what bounds it.

## alternatives considered

### a defaulted generous bound

every run bounded by a constant the engine picks.

rejected above all on the arithmetic: a default that stops a runaway in useful time kills a legitimate build that exceeds it, and a default no build exceeds stops nothing. the bound is the runner's patience, and only the runner knows it.

### a declared timeout on the action spec

the rule declares "this action may take at most N" on `ActionSpec`, the way a toolchain closure is declared (0030).

rejected on identity. the spec's digest is the action key's request half (0031), so a patience number would move the key: the same compile under two deadlines would be two computations, and a recorded attempt would not serve a run that declared less patience — reuse that serves a completed attempt by not running it would be refused by a number that changed nothing about what ran. the closure is content — it changes what the child may read — and the deadline is not.

### executor configuration, 0028's retracted shape

the timeout lives on `LocalExecutor`'s configuration, as 0028's unresolved section claimed it already did.

rejected as the home of the mechanism twice over. a run's deadline is a moment the run starts with, not a number an executor instance carries: the remaining time differs per action and shrinks as the run proceeds, and one executor instance serves many runs. and configuration cannot reach the engine's other halves — a pure body that yields forever never touches an executor. the invocation is the right carrier: per-action, derived from the run, visible to every executor including remote ones.

### an engine-side timer racing the executor

the driver races `first_finished` against a sleep until the deadline, so the engine enforces its own clock while actions run.

rejected because it puts a timer in the engine, which means naming the async runtime where only the `Runtime` trait is allowed to. the executor already holds the child and already owns ending it (`kill_on_drop` exists for cancellation); giving it the clock is one mechanism in one place rather than two mechanisms that disagree.

## consequences

the entry-point family grows to six: `run`, `run_cancellable`, `run_bounded`, and the `run_many` triple. a run that wants both a caller's cancel signal and a bound has no single entry point today — the open half is named below.

`E-1216` enters pith-diag's engine block, and `pith-executor-local` gains the tokio `time` feature it denied having. no encoding version moves (0048): the bound reaches no persisted record shape, and a cancelled-by-bound attempt is the cancelled shape the conformance suite already generates.

resource limits beyond time — cpu, address space, file descriptors — remain absent, as 0028 left them. the invocation seam this record establishes is where they compose: a `RunBound` that grows an rlimit half passes it into the same per-action authority. that is a follow-up with its own measurement, not a deferrel of this record's claim.

0018's backstop exists, configured per run by the caller, which is the question its unresolved section pinned to a round six milestones ago. 0022's timeout half is settled; its promotion now waits on partial cancellation alone.

## prototype evidence

`crates/pith-engine/tests/run_bound.rs` holds the engine's four shapes. `a_body_that_yields_forever_is_stopped_by_the_step_budget` is the shape the budget exists for: a body yielding fresh requests forever under a budget of fifty stops with `E-1216` naming the stepping request's label and the budget, every attempt terminal, nothing failed, the held work cancelled. `an_expired_deadline_stops_the_run_before_any_work` is the boundary check — a deadline already spent stops the run before a chain steps, under the same terminal states. `a_generous_bound_leaves_an_ordinary_run_untouched` is the control. `a_timed_out_action_fails_itself_and_cancels_what_was_waiting_on_it` is the split: an executor refusing with the bound's code leaves exactly one failed attempt — the action's, carrying `E-1216` — and one cancelled attempt — the chain parked waiting on it.

`crates/pith-executor-local/tests/local_executor.rs` holds the child's two. `a_child_that_exceeds_its_deadline_is_refused_with_the_bound_code` runs `sleep 30` under a deadline one hundred fifty milliseconds out: the refusal carries `E-1216`, and the test finishes in a quarter second rather than thirty — the child died at the deadline, `kill_on_drop` ending it when the wait future dropped. `a_generous_deadline_leaves_a_quick_action_untouched` is the control.

the workspace suite is 778 tests, 0 failures. `clippy --workspace --all-targets -D warnings` and `xtask check` pass; no `HashMap` entered the tree.

## unresolved

partial cancellation, the other half of 0022's promotion condition, is untouched: a run still stops all or nothing, and "this request is superseded, drop it and keep going" has no spelling. it belongs beside this record rather than in it, being a scheduler question rather than a bound question.

a run wanting both a caller cancel signal and a bound has no single entry point. the two compose at the signal — a host can wrap its flag behind a deadline — but the engine offers no `run_bounded_cancellable`, and the composition's terminal code (`E-1215` or `E-1216`, by which instruction stopped the run) is unclaimed.

`evaluate_pure` runs unbounded. the pure-only entry point serves libraries and crosses no effect boundary, and a looping host body there is a host bug; but a represented body (0038) will evaluate on the same step machine, and whether the pure-only path should carry a bound of its own is a question that round inherits.

cpu, address-space, and file-descriptor limits on an action compose on the invocation seam this record establishes and are not designed here.

the surface spelling of a bound — what a `.pi` file or a CLI flag declares — is the notation's question (M-13, M-15), as 0028 already said of executor configuration. `RunBound` is the library API until then.
