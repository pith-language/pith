---
schema: design-doc/v1
id: decision-0033-consumer-of-action-reuse
title: a consumer of an action revalidates by re-planning it
summary: let a completed pure attempt holding an action dependency enter the reusable index, and revalidate that dependency by re-selecting and re-planning the recorded request instead of trusting the key recorded when it ran
kind: decision
status: proposed
created: 2026-06-01
updated: 2026-06-19
tags:
  - actions
  - caching
  - incremental
  - identity
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0007-tracked-dynamic-dependencies
    - decision-0015-interface-rule-selection
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0031-action-cache-identity
  supersedes: []
---

# a consumer of an action revalidates by re-planning it

> closes the consumer-of-an-action gap [0031](0031-action-cache-identity.md) named in its consequences, together with the equality-based pruning across the effect boundary its unresolved section names as the same work. 0031 stands: its key and its admission test are unchanged, and what changes here is what a *pure* attempt holding an action edge may claim.

## context

a completed pure attempt with an action dependency is published `NotReusable(EffectfulDependency)`. 0031 argued that this is correct and left closing it as follow-up work with no measurement against it. the first build library supplies one.

`crates/xylem/tests/two_source_build.rs` compiles two sources and links the objects: six computations, three of them actions. after the first build the three action nodes are `Reusable` and none of the three pure nodes is. the two compile entries record `EffectfulDependency`; the build rule above them records `DependencyNotReusable`. building the same two sources again on the same engine allocates three more pure nodes and no action nodes at all: every action is answered from the index, and every pure computation runs a second time. a fresh engine over the same sqlite state and the same filesystem content store behaves the same way. its three actions load from the index without executing, its three pure computations run, and the root evaluation reports `Computed` where the equivalent pure-only graph would report `Hydrated`.

the reusable index therefore holds only what sits below the effect boundary, and in a build every target sits above one. what the re-run costs is the rule body, and for each action step it reaches, re-selecting the action rule and calling `plan()`, because the contract's digest is half of the key the index is read under. xylem's consumers are thin, so today that price is the re-plan and little else. a rule that read an object file and computed over it would pay for the computation on every run too, and nothing in the design bounds how much that is.

### why the invariant holds today

a `PureComputationKey` covers the rule identity, the rule revision, the requested interface, and the inputs. it does not name the action rule a `NeedAction` step will select, so a result recorded under one action rule stays findable under the same key after that rule is revised. the recorded edge does not close the gap either: `DurableDependency::Action` carries an attempt identifier and nothing else, so revalidation cannot ask whether the request would still resolve to that attempt. the pure edge can ask, because it carries the dependency's `PureComputationKey` and the index answers which attempt is currently reusable under it.

## proposed decision

a completed pure attempt holding an action dependency may enter the reusable index. its action edge is revalidated by re-selecting and re-planning the request that produced it.

### the recorded request

`DurableComputation::Action` gains the requested interface and the canonical request inputs, beside the `ActionComputationDigest` and the planned contract it already holds. the record then contains the preimage of the digest it stores rather than the digest alone, so `computation_digest` becomes derivable from the rest of the record and the cross-adapter conformance suite can check it instead of copying it.

`DurableDependency::Action` is unchanged. the attempt it names knows its own key and now its own request, so duplicating either onto the edge would be two copies to keep true.

### the walk

revalidating an attempt already visits its recorded dependencies in order, and each `Pure` edge ahead of an action edge has been confirmed to yield a canonically equal value before the action edge is reached. the consumer is pure, so a body resumed with equal values issues the same request: the recorded request is the request this run would make. an action edge is then valid when

- the recorded action attempt is still retained, and re-selecting its interface against this engine's action rules and calling `plan()` on its recorded inputs produces the contract digest the recorded key was built from, so the re-derived key equals the recorded one
- the current run's policy authorizes that freshly planned contract
- 0031's admission test accepts the attempt: the recorded execution's executor and platform are this run's, its `AccessVerification` meets this engine's floor, and the content it produced is still in the store

any of those failing is a miss. the consumer re-runs, which re-plans through the ordinary path and reports a denial or a planning diagnostic where a run reports one. revalidation answers whether an attempt may be served and never diagnoses.

one plan is run per action edge on the attempt being revalidated. the walk does not recurse through the reusable index, since 0024 has it trust that an indexed attempt was reusable when it was published, so the cost is bounded by the attempt's own edges rather than by the size of the graph beneath it.

### why the plan cannot be skipped

an `ActionComputationKey` commits to the digest of the contract the rule planned, and that digest is the only place in the system where a planned contract is checked rather than asserted. a pure key rests entirely on 0023's obligation that any change which may affect a result bumps the rule's revision; the action key does not have to, and freezing it onto an edge would throw away the difference.

the fixture shows the difference costing correctness. `CompileAction` holds the shared header (`crates/xylem/src/rules.rs:81`), which is not a request input and does not participate in any rule revision. changing the header changes the declared inputs, so it changes the `ActionSpecDigest` and the action key with it, and changes nothing in the compile entry rule's pure key. measured: a fresh engine over the same durable state with a changed header re-executes all three actions. a consumer holding a frozen key would keep its own unchanged key, find its recorded action key still naming a reusable attempt, and hand back the object compiled against the old header.

xylem could fold the header into its compile rule's revision and 0023 arguably requires it to. the general point survives that fix. a revision is a claim its author makes and a contract digest is a fact the engine derives, and an edge that skipped the plan would have no way to tell the two apart.

policy is the second reason, and 0031 does not state it. every milestone note since action policy landed says policy is reapplied on reuse, and policy authorizes a *contract*. serving a consumer whole skips the request that would have produced one, so nothing is ever shown to the current run's policy. re-planning is what makes the reapplication possible, which means the plan this record spends is a cost the policy rule already implies.

### equality across the boundary

when the re-derived key differs from the recorded one, the edge is not immediately dirty. the engine reads the latest reusable action attempt under the *new* key and compares its result to the recorded attempt's, which is what `durable_pure_dependency_is_valid` already does for a pure edge whose dependency was recomputed. an equal result leaves the consumer reusable, and downstream propagation stops there.

that is the second item in 0031's unresolved section, closed by the same machinery it predicted would close both. an action re-executed under a revised rule whose captured outputs content-address to what the previous attempt produced now leaves its consumers alone. when no attempt has been recorded under the new key yet, nothing is known and the consumer is dirty; it re-runs, the action runs, and the comparison is available to the run after.

## alternatives considered

### record the action key on the edge and compare it against the index

the cheapest shape: one field on `DurableDependency::Action`, revalidated exactly the way a pure edge is, with no planning.

rejected because the key would be asserted rather than derived. a revision of the action rule, a different rule winning the same interface, and a change to the state the planner holds are all invisible to a recorded key, and the header case above is the last of the three producing a wrong build. it also leaves the current policy having authorized nothing.

### store the planned contract on the edge and re-authorize that

answers the policy half without running `plan()`: keep the contract the previous run planned and hand it to this run's policy.

rejected because the stored contract is the one the previous run planned. policy would authorize a contract this run might not produce, which reads as a check and is not one. it also leaves the identity half exactly where the previous alternative leaves it.

### carry the action's identity into the consumer's key

0031's own phrasing for this work: extend `PureComputationKey` so a consumer's key names the action keys it depends on.

rejected as circular, for the reason 0031 rejected the same shape for the action key itself. a consumer's key has to be computable before the consumer runs, or there is nothing to look up; the action key needs the contract planned from inputs the consumer's own body produces. it would also partition the index into one consumer entry per distinct action key, where the walk this record proposes keeps a single entry and answers yes or no about it. the pure path already works that way.

### make the action key derivable from the request alone

drop the `ActionSpecDigest` from `ActionComputationKey`, leaving rule identity, revision, interface, and inputs. an action edge would then revalidate exactly like a pure edge, with no planning and no asymmetry.

rejected. this reopens 0031's key, which committed to both halves deliberately, and it moves the whole burden onto rule revisions being honest at the moment the engine gains a way to verify them. the consumer would still have to plan to give policy a contract, so the planning is not saved.

### leave the consumer out of the index

the status quo. rule bodies above the effect boundary are cheap in xylem, and what reuse skips is the execution, which is where the cost is.

rejected because the exemption grows with the build. every target sits above an action, so this is the pure half of the graph re-running on every run and surviving no process boundary, and it forecloses equality pruning at the one edge a build most wants it at.

## consequences

`reuse_decision` loses its action case (`crates/pith-engine/src/graph/reuse.rs:513`). an action edge whose target is reusable stops blocking reuse, and the decision becomes the same test for every edge: a dependency that is not reusable stops reuse, and nothing else does.

`ReuseReason::EffectfulDependency` and `DurableReuseReason::EffectfulDependency` stop being produced, and `ExpectedReuseDecision::EffectfulDependency` leaves the publication validator's derivation the way 0031 removed `ActionCachingDisabled` from it. the variants stay in the record types, because attempts published before this record still carry the reason and `explain_invalidation` still has to read them. a consumer whose action is genuinely not reusable, which now means an engine with action caching switched off, records `DependencyNotReusable` and needs no reason of its own.

revalidation becomes run-scoped. the pure reuse path takes the run's action policy and executor identity, as `reusable_action_evaluation` already does, and `durable_reuse_is_valid` changes signature with it. `Engine::evaluate_pure` has neither, so it cannot revalidate an action edge and answers that any attempt carrying one is not currently valid, which matches its existing refusal to take an effectful step.

a completed attempt gains its effective capability requirements. hydration asserts them empty today (`crates/pith-engine/src/graph/reuse.rs:143`) and justifies it by induction over a reusable subtree that cannot contain an action edge, which is the induction this record breaks. the requirements are derivable from the recorded graph, and deriving them per hydration means an unbounded store walk, so they are recorded and the validator checks them against the recorded dependencies at publication. that is the argument 0025 uses for letting the sqlite schema restate a record's shape.

both engine-state adapters change, the sqlite schema gains the request columns and the capability column, and the schema and semantic-encoding versions move together.

the M-3 fixture gains the assertion it cannot make today: a second build of unchanged sources reuses its root instead of recomputing it, and a fresh engine over the same durable state hydrates it. the fine-grained invalidation test's action count is unaffected, since a served action already returns the existing node rather than allocating one.

## unresolved

planning at revalidation assumes `plan()` is cheap and depends on nothing but its inputs and the rule's own state. 0007 forbids ambient discovery during evaluation and the same obligation covers revalidation, but nothing enforces it, and a planner that read the filesystem would make revalidation answer differently on two identical runs. this is the trust 0023 already places in a rule author for revisions, extended to a second place, and it stays unenforced for as long as rule bodies are arbitrary rust.

the pure edge has the selection gap this record closes for actions. it records the dependency's key, which names the rule that was selected when it ran, and revalidation never re-selects, so a newly registered rule that wins the same interface is invisible to a recorded pure edge. the mechanism here answers it, and would need the pure edge to record its request the way the action computation now does. whether that is worth a second copy of every pure request is a question this record does not settle.

the walk stays one level deep. an indexed attempt is trusted to have been reusable when it was published, which is 0024's model and not changed here; an entry whose own dependencies drifted without anything re-running it is stale in the same way for pure and action edges alike.

record size grows by the request inputs of every action attempt. they sit beside a contract that is already stored whole, so for a compile or a link the addition is proportional to what is there. a request carrying a large value inline is not bounded by anything, and when a value should be stored inline rather than content-addressed is not settled.

cross-machine reuse is untouched, on the same terms 0031 leaves it: the admission test this walk applies asks whether *this* environment matches what was recorded, and a consumer served across machines needs the trust model 0024 leaves open.
