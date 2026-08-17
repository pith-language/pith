---
schema: design-doc/v1
id: decision-0050-cycle-detection-over-the-computation-key
title: a cycle is a live computation key, not a walk over frames
summary: the cycle predicate tests the requested computation's digest against a per-chain set of live digests, replacing a structural comparison against every frame in scope; the walk survives only to name the frames a diagnostic reports
kind: decision
status: proposed
created: 2026-08-03
updated: 2026-08-03
tags:
  - engine
  - incrementality
  - identity
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0015-interface-rule-selection
    - decision-0018-termination-and-recursion
    - decision-0021-arena-graph-engine
    - decision-0022-sync-core-async-scheduler
    - decision-0023-rule-and-cache-identity
---

# a cycle is a live computation key, not a walk over frames

> the first measurement of pure evaluation at any scale, and the reason for taking it now: [0047](0047-the-declaration-table.md) grows a nominal type from a name into a declaration body, and this predicate compared whole interfaces on every request.

## context

0018 makes cycle detection load-bearing rather than defensive. its position is that pure evaluation terminates by construction and "a limit ... is never the primary defense against non-termination in pure code" — the graph carries recursion and cycle detection provides the real bound. so the predicate runs on every pure request, and it is the only thing standing between a self-referential declaration and a stack that never unwinds.

it was implemented as a scan. `cycle_chain` built a `Vec` of every frame in scope — the requesting chain's whole stack plus every ancestor chain's — and compared each against the incoming request on three fields: the selected `RuleId`, the full `Interface`, and the full input-value slice. two allocations and O(depth) deep structural comparisons, per request.

that cost is quadratic in the shape a build actually has. `PureStep::Need` pushes the requested child onto the *requesting* chain rather than opening a new one (0022's synchronous core), so a dependency chain n deep is n frames on one stack, and the nth request compares itself against n−1 frames. nothing measured it, because nothing measured anything: the largest computation graph asserted anywhere in the repository is three nodes, and there was no benchmark harness at all.

the engine already derives exactly the value the predicate needs. `PureComputationKey::new` runs once per request for the reusable index and the durable attempt, and its digest is a domain-separated hash over the rule identity, the rule revision, the request interface, and the inputs — which is a superset of what the scan compared by hand.

## proposed decision

a request is circular when its computation digest is already live in the requesting chain's scope. each chain holds the set of digests of the frames on its stack, maintained where the stack's membership changes, and the predicate is one set lookup per chain in the lineage.

the digest is derived once, in `prepare_request`, and carried on the frame. three things want it — the cycle predicate, the reusable index, and the durable attempt — and deriving it three times was only tolerable while one of the three did not exist.

### the two comparisons are equivalent, and the argument is not "both identify a computation"

the scan compared `(RuleId, Interface, inputs)`. the digest covers `(RuleIdentity, RuleRevision, Interface, inputs)`. those are not the same tuple, and the equivalence needs the two directions stated.

a digest match implies a scan match. within one run the rule set is fixed — `register_rule` takes `&mut self` and so does every entry point, so no rule is registered while a scheduler is live — and the interface and inputs participate in both. what remains is whether one `(identity, revision)` pair could name two `RuleId`s, which would let the digest match where the scan did not. it could only happen if the same rule were registered twice, and 0015 makes such a pair unreachable through selection: two rules with one interface are ambiguous and refused as `E-1102` before any request resolves to either. so the case is excluded by the selection rule rather than by an invariant this record has to maintain.

a scan match implies a digest match, because the scan's three fields are three of the digest's four and the fourth is a function of the `RuleId` the scan compared.

the digest is also strictly more discriminating in the direction that matters for 0018: it separates two applications of one rule at different revisions, which the scan could not, because a `RuleId` says nothing about a revision.

### the walk survives for the diagnostic

`E-1205` names the request chain it found — "dependency cycle: start a -> need b -> need a" — and that requires the labels of the frames in scope, in order. so the walk stays, and runs only after the predicate has already said there is a cycle. detection is the hot path and reporting is not: a run that reports a cycle is about to fail.

### the set is ordered, and the guard that should have caught it did not

`IndexSet`, not `HashSet`. 0021 forbids nondeterministic iteration order in crate source, and this set is iterated when the diagnostic is built.

worth recording because the guard missed it: `xtask check-determinism` greps for `HashMap` only. the rule 0021 states is about iteration order rather than about maps, so `HashSet` has the same defect for the same reason, and the first structure this record wanted was exactly the shape that would have passed the check. the guard should cover both.

### a digest is live only while its own frame is

the digest enters the set when the frame is pushed and leaves when the frame is popped, which is what distinguishes a cycle from reuse. two sibling requests for one value — a rule body that needs the same thing twice in sequence — are ordinary reuse, and the first request's frame has completed and left the stack before the second is made. the ordering matters because the predicate runs *before* the reusable-index lookup, so a digest left behind by a completed frame would refuse its own sibling.

## alternatives considered

### keep the scan and compare digests instead of structures

store the digest on the frame, keep the linear walk, compare 32 bytes per frame instead of an interface and an input slice.

a large constant-factor win and the wrong shape. it leaves the cost O(depth) per request and therefore quadratic in the graph, which is the property 0018 leans on the predicate for. the measured curve below is the argument: at 16,000 the scan was 850 ms and the set is 60 ms, and a cheaper comparison would have moved the constant while leaving the ms-per-request still climbing with depth. it is also barely simpler — the digest has to be on the frame either way, and what this record adds beyond that is one set per chain.

### one set on the scheduler for all chains

hold a single set of live digests rather than one per chain.

rejected because it answers a different question. a digest live in a *sibling* chain is not a cycle: a fan-out group evaluates independent requests concurrently (0029), and two of them may legitimately depend on the same value at the same time. scope is the lineage, and the per-chain sets are how the lineage is expressed. a single set would refuse a correct fan-out.

### detect cycles in the arena instead of the scheduler

look for a back-edge in the recorded dependency graph rather than tracking live frames.

rejected on when the answer is needed. the arena records an edge after a request resolves, and the point of the predicate is to refuse *before* pushing a frame that cannot unwind. the arena also holds completed computations from earlier in the run, so a back-edge there is not evidence of a live cycle. the scheduler owns the live stack, and 0022 keeps it deliberately free of arena state — the module's own doc says it "never touches the arena, so it can be reasoned about without the graph," which this record preserves.

### a depth limit instead of a predicate

bound the stack and fail when it is exceeded.

this is the backstop 0018 describes and it refuses the job on 0018's own terms: a limit "is never the primary defense against non-termination in pure code." it also cannot produce `E-1205`'s chain, which names the requests that close the loop rather than reporting that evaluation went too deep. the backstop is still owed — nothing in the workspace bounds a runaway — and it belongs to the record that gives the engine time and resource limits, which four callers separately need.

## consequences

`EvalFrame` carries `key_digest`. the digest rather than the whole key, because the frame has no other use for the identity and revision, and 64 extra bytes on a frame moved through two enums pushed `large_enum_variant` over its threshold — which clippy caught, and which is the honest reason the field is a digest rather than a taste.

the scheduler owns stack membership. `push_frame` and `pop_frame` replace the raw `Vec` push and pop, so the set cannot drift from the stack; `stack_mut` stays for resumption edits, which change no frame's identity. `finish_frame` no longer pops — the caller does, after publication, which is the ordering the old comment described and now the scheduler enforces.

`cycle_key` is gone. it existed to keep the two call sites from disagreeing about what counts as the same request, and the digest is that agreement.

`crates/pith-engine/benches/scale.rs` is the first benchmark in the repository, over three shapes: a deep chain, a wide sequence, and a fan-out. `harness = false`, so it adds no dependency — a benchmark framework would be a new external dependency in a kernel with few, for numbers whose job is to say whether a curve changed shape. `just bench` runs it.

writing it surfaced 0015 as an ergonomic fact rather than a design claim. the two-rule shapes give their root a second input it never reads, because a root and a leaf that both spell `(Int) -> Int` are ambiguous and refused. distinguishing two rules costs a type, which is the mechanism by which a domain's registered rule count grows with its model.

### measured

`cargo bench -p pith-engine` on this host, release build, before and after, with the benchmark identical across both:

| n | deep chain, scan | deep chain, set |
| --- | --- | --- |
| 1,000 | 5.9 ms | 3.6 ms |
| 2,000 | 18.9 ms | 7.5 ms |
| 4,000 | 52.7 ms | 13.9 ms |
| 8,000 | 201.1 ms | 27.2 ms |
| 16,000 | 850.1 ms | 60.0 ms |

the scan's cost per request climbs from 0.006 ms to 0.053 ms across that range and each doubling of n roughly quadruples the time. the set's cost per request stays flat at 0.003–0.004 ms and each doubling doubles the time. 14x at 16,000, and the gap widens with depth.

the two shapes with no depth are unchanged, which is the control: wide sequence 46.9 ms against 40.0 ms at 16,000, wide fan-out within noise at every size. a stack two deep had nothing for the scan to scan.

`dependency_cycle_reports_the_request_chain` and `repeated_rule_with_different_inputs_is_not_a_cycle` pass unmodified, so detection and its diagnostic are preserved. `the_same_request_twice_in_sequence_is_reuse_and_not_a_cycle` is new and covers the property the bookkeeping introduces; removing the retirement on pop fails it, so it tests the mechanism rather than restating it. the workspace suite is 660 tests, 0 failures.

## unresolved

the predicate is now cheap and rule selection is not. `select_rule` scans the whole rule arena per request, comparing interfaces structurally and cloning each match before sorting, and under 0015 the rule count grows with the domain model. the same computation digest the engine already derives could index it. that is a second record, and the benchmark above is what would hold it to a measurement.

0047 will grow what the remaining structural comparisons cost. the digest is derived from a canonical encoding of the interface, and a nominal type that carries its declaration's body makes that encoding larger, so key derivation gets more expensive per request even as the cycle check stops depending on depth. the benchmark exists so that change is visible rather than inferred.

the backstop limit 0018 and 0022 both require still does not exist, and this record does not supply it. a predicate refuses a request that repeats; it does not bound a body that yields an unbounded sequence of distinct requests.
