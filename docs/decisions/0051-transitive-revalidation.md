---
schema: design-doc/v1
id: decision-0051-transitive-revalidation
title: revalidation walks the recorded subtree, and retention's floor becomes the record closure
summary: a recorded pure edge is valid only if everything beneath it is, checked by walking the record rather than by a per-attempt footprint of the revisions a subtree used, because the footprint is a deep constructive trace and the taxonomy's result is that deep traces lose early cutoff; the walk raises 0027's cache-tier floor from the reusable index to its transitive record closure, and the walk stops at the live frontier
kind: decision
status: proposed
created: 2026-08-05
updated: 2026-08-05
tags:
  - caching
  - incrementality
  - retention
  - identity
relations:
  informed_by:
    - research-incremental-revalidation
    - research-build-systems
  depends_on:
    - decision-0021-arena-graph-engine
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0027-retention-and-gc
    - decision-0031-action-cache-identity
    - decision-0033-consumer-of-action-reuse
  amends:
    - decision-0049-pure-edge-revalidation
---

# revalidation walks the recorded subtree, and retention's floor becomes the record closure

> takes the item [0049](0049-pure-edge-revalidation.md) named first in its unresolved section and left argued but not decided: "a dependency at depth two whose rule was revised is still served, and the record that closes it should weigh the recursive walk against a per-attempt revision footprint and state the retention consequence either way." 0049 stands and its per-edge check is unchanged; this record decides what happens under the edge, and it is the first decision in the tree to put a floor under [0027](0027-retention-and-gc.md)'s open numbers.

## context

0049 fixed the edge and said so precisely. a recorded pure dependency is valid only if the rule it names is registered here at the revision it names, which catches a dependency whose rule moved. what it cannot catch is a rule that moved one level further down, because the edge above it is intact by every test the check applies: that rule was not revised, so its key did not move, so its recorded attempt is still the latest reusable one under it, and the check says yes and stops.

so a root over a middle computation over a leaf, with only the leaf revised, hydrates the root and serves a value derived from the leaf body this engine no longer has. that is the same K-9 violation 0049 reproduced, at depth three instead of depth two, and it survives 0049 in full.

the reason the gap has a shape at all is hydration. every system that revalidates gets transitivity from its scheduler: asking whether a result is current asks the same question of its dependencies, because answering it means requesting them. pith's hydration is exactly the step that skips this. a hydrated node is served from the reusable index without its dependencies ever being requested, and it carries an empty arena dependency list by construction, so the recursion that would have happened does not.

the research note reads five systems at this one question and finds none that checks an edge and trusts what is under it. Salsa's `maybe_changed_after` recurses. rustc's `try_mark_green` recurses and loads the previous session's whole dependency graph to do it. Bazel's Skyframe sweeps the reverse closure of every changed input. Nix does not check at all, because it folded the closure into the output path's identity before the build ran. the shallow position this engine currently holds is not one of the four.

## proposed decision

a recorded completion is valid only if every attempt its record reaches is valid. revalidation walks the recorded graph beneath a completion, applying 0049's per-edge check at every level, and refuses the whole record if any level refuses.

the walk descends through the attempt each edge check *accepted*, never the superseded one the edge recorded. those two differ exactly when a dependency was recomputed to a canonically equal result under an unmoved key — the case 0033's mechanism exists to serve — and the superseded record's own edges may name results the current rule set has since replaced. descending into it would refuse a consumer whose whole subtree is current, which throws away the early cutoff the equal-result comparison one line above had just established.

### the walk is a frontier, not recursion

the recorded graph is as deep as the build is, and 0022 already made the evaluator an explicit stack machine rather than a recursive one for that reason. a validity check that overflows on a chain the evaluator handles would be a worse bound than the one it is replacing, so the walk carries an explicit frontier of attempts and a set of the attempts it has already enqueued. the set collapses a diamond to one visit and bounds the pass if an adapter ever returns a cyclic record, which nothing should but which the check should not depend on.

### the walk stops at the live frontier

an edge whose accepted attempt is one this run already holds live and reusable in the arena is not descended into.

such an attempt was established when it entered the arena. if it was computed here, its own dependencies were computed, reused, or hydrated here, each established the same way. if it was reused or hydrated, it went through this very check. descending into its record re-derives an answer the run already has, and doing that at every depth of a chain is the difference between a walk that is linear in the recorded graph and one that is the graph's triangle. the measured section carries both numbers, because the bound is not a micro-optimization: without it the shape is quadratic, which is the class of cost [0050](0050-cycle-detection-over-the-computation-key.md) had just removed from the request path.

what the short-circuit rests on is that within one run, the durable state an arena node was established against moves only by this engine's own publications. that is 0024's adapter-boundary rule stated as a premise instead of assumed, and it is worth stating because it is the one thing here that a concurrent foreign writer would break — and a concurrent foreign writer would already break the per-edge check, which reads the index once per edge and not once per graph.

### an action edge adds nothing to the walk

blob and action edges are recorded on the pure computation that requested them, so an action attempt's own recorded dependencies are the capability uses its executor reported, and there is nothing beneath one to walk. 0033's re-selection and re-planning remains the whole of what an action edge checks. this is a property of where edges are recorded rather than a decision, and it is stated because a reader who assumes the walk is uniform over edge kinds would look for the action case and not find it.

## alternatives considered

### a per-attempt footprint of the (identity, revision) pairs its subtree used

record on each completed attempt the set of rule identities and revisions its whole transitive subtree used, as the union of its own pair and its dependencies' footprints. revalidation is then a flat check of that set against the registered rule set, with no store reads beyond the attempt already loaded and no descent at all.

this is the mechanism 0049 named as the walk's competitor, and the tree already has its shape: `CompletedAttempt::capabilities` is exactly this — a union propagated over the arena at publication and read back off the record at hydration, "recorded rather than walked back out of the store on every hydration," and checked against the recorded dependencies when the attempt is published so the two cannot disagree. a footprint would be built the same way, by the same code path, and bounded by the number of *distinct rules* in a subtree rather than its size, so a ten-thousand-node build over twelve rules carries twelve pairs per attempt. under [0048](0048-pre-release-version-pinning.md) the record-shape change costs no version movement, only a discard and rebuild.

it is rejected on what it spends. *Build Systems à la Carte* separates rebuilders into a dirty bit, verifying traces, constructive traces, and deep constructive traces, the last of which stores only the terminal input keys and ignores the intermediate dependencies. the footprint is that category: the rule set is its terminal input, and the intermediate attempts are what it declines to look at. the paper's result on the category is flat — "all traces except for deep traces support the early cutoff optimisation" — and the reason applies here without translation. a footprint recorded on a consumer names the revisions its subtree used *when the consumer was published*, so a revision that moves anywhere beneath it invalidates the consumer even when the dependency recomputes to the same value and the consumer's result is unchanged. the walk keeps that case, because the equal-result comparison happens at the level where the change was and the levels above it never learn there was one.

losing early cutoff is not a cost this record is free to accept. it is the property 0031 states, 0033 exists to preserve, and 0049 already refused to trade once, rejecting "fold the dependency set's revisions into the consumer's own key" partly because "it would destroy early cutoff." the footprint is that same rejected trade moved out of the key and into a side table, where it is less visible and costs the same thing. Buck and Nix are the systems in the deep-trace category, and Nix does not experience the loss because it has no early cutoff to lose: a changed input changes the output path and the question does not arise.

the footprint also answers a strictly narrower question than the walk. it sees a revised or unregistered rule at any depth; it does not see a dependency at depth two that another engine recomputed to a *different* result under the same key. the walk sees both, using the comparison the depth-one check already performs, and that second case is not hypothetical — `durable_reuse_is_valid_until_a_dependency_result_identity_changes` is a test of it at depth one, written before this record.

### retain both: the footprint as a fast path, the walk as the fallback

check the footprint first and walk only when it fails, on the theory that the common case is a subtree that has not moved.

rejected because the footprint's failure is exactly the early-cutoff case, so the fast path succeeds precisely when the walk would also have succeeded quickly, and falls through to the walk in the case the walk exists to get right. it buys nothing and costs the record-shape change, the adapter migration, the conformance-fixture churn, and a second definition of validity that has to be kept agreeing with the first. one mechanism per concern; this is two.

### recurse into the attempt the edge recorded rather than the one it accepted

a natural reading of "walk the recorded graph": the record names an attempt, so descend into that attempt's record.

rejected and measured. the recorded attempt is the superseded one exactly when the equal-result comparison accepted a newer one in its place, and the superseded record's edges describe the graph as it was before whatever produced the newer attempt. refusing on those edges refuses consumers that are genuinely current. `a_revised_leaf_recomputed_to_an_equal_result_still_hydrates_its_root` is the falsifier: with the descent pointed at the recorded attempt it reports `Computed` where the correct answer is `Hydrated`.

### leave it shallow and make authors bump revisions transitively

require that revising a rule also moves the revision of every rule that could depend on it, so a transitive change becomes a direct one and the per-edge check suffices.

rejected on 0023's own evidence and 0047's. 0023 records "manual semantic version only" as the alternative it rejected, and what shipped anyway — one hand-bumped literal per library — is what made 0049's bug reachable across the xylem/phloem boundary. this alternative asks an author to compute a reverse-dependency closure by hand across library boundaries and get it right every time, which is the manual-metadata cost Pants v2 names as its reason for choosing inference. it also cannot be checked: nothing can tell whether an author who did not bump was correct or forgetful. 0047 moves in the opposite direction by deriving revisions from declarations, and this alternative would take the derivation the other way.

## consequences

`durable_completion_is_valid` becomes a frontier walk. `durable_pure_dependency_is_valid` enqueues the attempt it accepted instead of returning, and gains the live-frontier short-circuit. no record shape changes, no adapter changes, no encoding version moves, and the action arm is untouched, so the whole of this lands in one file.

reuse now costs a read per attempt in the hydrated part of a recorded subtree, where before it cost a read per immediate edge. the live-frontier bound means a warm engine pays almost nothing and a fresh engine hydrating a large graph pays the graph once.

### retention: 0027's cache tier is no longer the reusable index

this is the consequence the record owes 0027, and it is a floor rather than a rule, because a record the collector removed makes revalidation answer *false* and recompute. nothing serves a wrong result. what breaks is reuse, silently and from the top.

0027 fixes the roots as the reusable index (R1) plus explicit pins (R2) plus a bounded history window per computation key (R3), and it splits engine state into two evictability classes: a cache tier, "an attempt in the reusable index," evictable "when superseded by a newer reusable attempt for the same key," and a provenance tier of edges, capability records, and diagnostics, retained longer. under this record the cache tier is the *transitive closure* of the reusable index under recorded dependency edges, and a superseded attempt inside that closure is cache material rather than provenance material, because a consumer above it revalidates against it.

it is worse than edges alone. the per-level check compares `completion.result` of the recorded attempt against the latest one under its key, so the walk needs results throughout the closure, not just the dependency structure. that weakens the Salsa split 0027 adopted — "an attempt's result blob and produced content are evictable independently of its edges" — for anything inside the closure of a reusable attempt. outside that closure the split holds unchanged.

what this does *not* touch: R2's pins, R3's history window as a bound on the attempts per key *beyond* the closure, the content store's size-budget LRU, the diagnostic TTL, and the content-first, metadata-last ordering. 0027's open question was the numbers, and it stays the numbers. what it gains is a lower bound they have to clear, which it did not have.

the field evidence says this floor is real and expensive. Salsa states it as a rule its LRU must respect: "note that information about query dependencies is not evicted." rustc pays it as a file format, loading the previous session's entire dependency graph and fingerprint set at every session start with no partial load. Bazel pays it as heap and shipped Skyfocus to relieve it, letting a developer name a working set and dropping the rest of the graph, at a stated price: "changes outside of the working set will cause a build error." pith's version of that price is a recompute rather than an error, which is the better failure mode and is the one thing here none of the five manage.

### measured

`hydration_is_refused_when_a_rule_two_levels_down_was_revised` builds a root over a middle computation over a leaf, then opens a second engine with the root and middle rules unchanged and the leaf's revision moved to `leaf-v2` answering `2`. the root recomputes and observes `2`. with the enqueue removed and everything 0049 checks left in place, the same test reports `Hydrated` and the value `1`, so it falsifies the shallow check rather than restating the new one.

`a_revised_leaf_recomputed_to_an_equal_result_still_hydrates_its_root` is the falsifier for the descent direction. a second engine recomputes the middle computation over the revised leaf to a canonically equal result, publishing a new attempt under the middle's unmoved key; a third engine then hydrates the root, whose recorded edge still names the superseded middle attempt. it reports `Hydrated` and the root's original durable attempt. pointed at the recorded attempt instead of the accepted one, it reports `Computed`.

the benchmark gains a fourth shape, `reused-chain`, which is the walk's worst case: a reuse at every depth of a chain, so the walks would sum to the chain's triangle. at one, two, and four thousand it runs 3.2, 6.5, and 13.8 ms, against 3.3, 6.5, and 13.7 ms for the same shape before this record — the walk is not observable there. without the live-frontier bound the same three sizes are 65.8, 264.3, and 1049.3 ms, quadrupling per doubling, so the bound is what the numbers are measuring and the shape exists to hold it. `deep-chain`, `wide-sequence`, and `wide-fanout` are unmoved, which is the control: none of them reuses anything.

the workspace suite is 713 tests, 0 failures. `xtask check-determinism` passes over the `IndexSet` the walk introduced, which it would also have passed over a `HashSet` — the same gap 0050 recorded, unchanged and still worth naming.

## unresolved

the concurrent-writer premise the live-frontier bound rests on. this record states it as 0024's adapter-boundary rule rather than deriving it, and the per-edge check has the same exposure at a finer grain. what a second process writing to a shared engine-state store during a run may and may not do is a question 0024 left to the remote-cache and trust work, and the bound above is now a second caller for it.

0027's numbers, which this record only floors. what an N-attempts-per-key window means when N attempts inside a closure are load-bearing, and whether the collector should be able to see the closure at all before it prunes, are the questions the workload-evidence record inherits from here. Bazel's Skyfocus is the one primary example of pruning a closure deliberately, and its answer — error at the boundary — is the one pith does not need to copy.

the walk is bounded by the recorded graph, and nothing bounds the recorded graph. a chain long enough makes a cold hydration expensive, and unlike the evaluator's own depth this cost is paid to decide *not* to work. the time and resource bounds M-6 already owes four callers are where a ceiling on it would live.

K-9 still does not cover this. 0049 recorded that `requirements/kernel.md` scopes the equivalence to "the same declared inputs" and a rule set is not an input, so both records fix violations of a property nobody has written down. that requirement edit is now owed twice and made zero times.

the differential harness 0049 asked for is still the only thing that would make either record's property executable in general rather than on the shapes a fixture has. this record adds a second shape to that fixture and does not change the argument: a generated scenario run incrementally and from an empty cache, compared, on `MemoryEngineStateStore`, with an edit script that only generates revision-moving and input-changing edits.
