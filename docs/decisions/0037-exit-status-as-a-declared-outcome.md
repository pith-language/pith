---
schema: design-doc/v1
id: decision-0037-exit-status-as-a-declared-outcome
title: a contract declares whether an exit status is a failure or a result
summary: add an exit-status contract to ActionSpec so a program whose nonzero exit is its verdict can succeed as an action, and carry the observed status to the rule that reads the verdict out of it
kind: decision
status: proposed
created: 2026-06-15
updated: 2026-06-15
tags:
  - actions
  - build
  - effects
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0003-explicit-effects
    - decision-0014-reproducibility-properties
    - decision-0031-action-cache-identity
    - decision-0032-action-granularity
    - decision-0036-produced-program-as-content
  supersedes: []
---

# a contract declares whether an exit status is a failure or a result

> extends the action contract [0003](0003-explicit-effects.md) makes visible, to cover how an action ends. [0032](0032-action-granularity.md) is what forces the question into the contract instead of leaving it to a wrapper.

## context

M-3 asks for tests as a build concept. a test is a program whose exit status carries its finding: zero means the assertions held, nonzero means they did not. both are results, and a build wants both recorded, cached, and explained.

the executor reads a nonzero exit as a failed action. `run_action` returns an adapter diagnostic naming the status and an excerpt of stderr, and the engine marks the computation failed. that is the correct reading for every tool the executor has run so far. `gcc -c` exiting nonzero wrote no object file, so there is nothing to capture and nothing worth keeping; the same holds for `ld`, for the depfile pass, and for every action 0032 lists.

it is the wrong reading for a test. a failing test is not a broken contract, and treating it as one has two costs. the failure is not a value, so nothing downstream can ask "did the tests pass" and get an answer. and a failed computation is not in the reusable index, so every build re-runs every failing test, which is exactly backwards from what an incremental build should do: the run that changed nothing should be the cheap one whether the tests pass or not.

nothing the executor can see distinguishes the two cases. both are one process exiting nonzero. the difference is in what the program means by it, which is a property of the contract rather than of the execution.

[0032](0032-action-granularity.md) closes the workaround. running the test under `/bin/sh -c 'prog; echo $? > status'` would turn the status into a declared output and need no new contract at all, but "an `Action` is one invocation of one tool" and that is two, with a shell deciding what happens between them. 0032 reserves that shape for `Opaque`.

## proposed decision

`ActionSpec` gains an `exit_status: ExitStatusContract` field with two variants:

- `SuccessRequired`, the default. a nonzero exit, or a death by signal, fails the action, exactly as before this record. every existing rule declares this, and the behaviour of every existing action is unchanged.
- `Reported`. however the program ended is a fact the rule reads. the action succeeds as long as the executor ran the program and captured what the contract declared, and `ActionRule::complete` decides what the ending means.

the observed ending travels to the rule as `Option<ActionExit>`, where `ActionExit` is `Code(i32)` or `Signal(i32)`. the two are kept apart because a program that exited 1 to report failures and a program that was killed are different facts, and a rule building a verdict needs to tell them apart. `None` is what an executor reports when it observed no ending, which is the honest answer from a test fake rather than a fabricated zero.

### where the status rides, and where it does not

the status is a field on `CapturedActionExecution` and `ActionExecution`, the two envelopes that carry one execution from the executor to the rule. it is not a field on `ExecutionReport` or `CapturedExecutionReport`, which are the parts that persist.

that boundary follows from where `complete` runs. it is called in exactly one place, on the path where an action actually executed; a later run that is served from the reusable index gets the recorded result value and never calls `complete` again. so the status is the raw material a verdict is derived from, and the derived value is what the graph records and reuses. putting the status in provenance as well would be recording the same fact twice in two shapes, and 0025's objection to parallel representations applies.

the consequence is that provenance carries "this test reported failure" as a typed value and does not separately carry "the process exited 1". whether the raw status belongs in provenance too is named in "unresolved".

## alternatives considered

### wrap the program in a shell

`/bin/sh -c 'prog; echo $? > status'`, with the status as a declared output the rule parses. no contract change, and it works today.

rejected by 0032, as above: two invocations with a shell choosing the order is the `Opaque` shape, not the `Action` shape. it also puts a shell and a coreutils closure in the trusted set of every test, and makes the recorded program `/bin/sh` for all of them, which is the provenance objection [0036](0036-produced-program-as-content.md) rejected the loader trick for.

### let the executor decide from the exit status alone

treat a "small" nonzero status as a result and a signal death as a failure, or make some similar rule out of what the executor can observe.

rejected because the executor cannot know. the same status from `gcc` and from a test means opposite things, and any rule the executor invents is a guess dressed as a policy. 0014's distinction between a claim and a measured fact cuts here: the executor can measure how the program ended and cannot measure what the program meant.

### an expected-status set in the contract

let the contract declare which statuses count as success, as `expected: [0, 1]`.

rejected as more mechanism than the question needs, and worse at the thing it looks better at. a test's status set is open — a suite may exit with the number of failures — so the declaration would have to be a range or a predicate, and 0026 has no predicate types by decision. the two-variant form says the one thing that is actually being decided: whether the status is an outcome or an error.

### make the verdict a declared output instead

have the test program write its verdict to a declared output path and keep `SuccessRequired`.

rejected because it only moves the problem. a test that fails still exits nonzero, so the action still fails before the output is captured, and a test that could be relied on to exit zero while writing a failure verdict would be a test harness pith had specified rather than a program a user brought.

## consequences

every `ActionSpecDigest` changes again, for the same reason as 0036: the manifest carries the contract's exit-status tag. the two records land together, so the invalidation is one event rather than two.

the storage encoding carries the field in version 2, alongside 0036's tagged program. 0024's independent versioning of the stored form is what makes that a version bump rather than a migration.

`Reported` puts a fact in front of a rule that the executor used to act on. a rule that reads a signal death as a passing test would call a crash a success, and one that ignores the status would treat every test as passing. the two-variant contract makes the reading explicit but does not make it correct, which is the usual cost of moving a decision to where the information is.

the reusable index now holds failing outcomes. a test whose verdict was "failed" is a completed attempt under 0031's key, and an unchanged failing test is served rather than re-run. that is the point, and it means a user who expects a re-run to re-execute a failing test will need `Engine::set_action_caching(false)` or a changed input, the same as for any other action.

## unresolved

a seccomp or landlock kill reaches a `Reported` action as `Signal(SIGSYS)` or as a failure to read a denied path, rather than as the executor error it is under `SuccessRequired`. the confinement layers stay in force and the kill still happens, but whether it is *reported* as a confinement violation now depends on the rule. `crates/pith-executor-local/tests/executor_contract.rs` asserts this shape deliberately, using the `SIGSYS` the filter delivers for `kill(2)`, because the sandbox is the only way a shell script in this fixture can be signalled at all. an executor that distinguished "the sandbox stopped this" from "the program died" would let a rule keep the distinction; seccomp's `si_code` carries it and `wait4` does not, so getting it would mean the executor watching for the signal rather than reading the exit status.

whether the raw exit status belongs in provenance alongside the derived value is open. the argument for is 0014: how an attempt ended is a measured fact about that attempt, and provenance is where measured facts go. the argument against is that the rule's typed result already records the finding, and adding the status to the durable report means a column in the sqlite schema, a field in the in-memory adapter, and a case in 0025's conformance suite for a fact nothing yet reads.

`ExitStatusContract` is declared by the rule that plans the contract, so it is a property of the rule rather than of the request. a build that wanted to run the same program once as a test and once as a required step would need two rules. whether that ever comes up is unknown, and inventing a request-level override before it does would be guessing.

timeouts interact with this and are still out of scope, as 0028 left them. a program killed for exceeding a timeout would arrive at a `Reported` rule as a signal death indistinguishable from a crash, and the timeout design is where that needs answering.
