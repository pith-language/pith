---
schema: design-doc/v1
id: decision-0049-pure-edge-revalidation
title: a pure edge revalidates against the revision its rule is registered at
summary: a recorded pure dependency is valid only if the rule it names is registered here at the revision it names, because a revised rule mints a new key and leaves the old key's attempt still latest under it; an unregistered rule makes the edge invalid rather than skipped
kind: decision
status: proposed
created: 2026-07-31
updated: 2026-07-31
tags:
  - caching
  - incrementality
  - identity
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0021-arena-graph-engine
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0031-action-cache-identity
  amends:
    - decision-0033-consumer-of-action-reuse
---

# a pure edge revalidates against the revision its rule is registered at

> completes the pure half of what [0033](0033-consumer-of-action-reuse.md) built for action edges. 0033 revalidates an action edge by re-selecting the recorded interface and re-deriving the key; the reasoning was never applied to a pure-pure edge, where the recorded key was taken on trust. 0033 stands and its mechanism is unchanged.

## context

K-9 is the kernel's central correctness claim, and 0021 and 0022 both call it that: "incremental and cached evaluation produces a result equivalent to evaluation from an empty cache under the same declared inputs." the engine violated it, reproducibly, in the ordinary case a build hits.

`durable_pure_dependency_is_valid` received the recorded `PureComputationKey` and asked one question: is the latest reusable attempt under that key still the attempt this edge recorded, or one with a canonically equal result? that is the right question when a dependency's *result* moves, and it is the question the existing tests exercise — `durable_reuse_is_valid_until_a_dependency_result_identity_changes` publishes a second attempt under the same key with different bytes, and the consumer goes dirty.

it cannot see a revision that moved. a revised rule produces a different `PureComputationKey`, because 0023 puts the revision inside the key. so the old key is not superseded; it is *orphaned*. nothing publishes under it again, its recorded attempt stays the latest reusable one under it forever, and the consumer's edge revalidates against a rule body the engine no longer has. the consumer hydrates the superseded result and reports `Hydrated`, which is the shape of a correct answer.

0023 states the rule this breaks in its own words: "false invalidation is acceptable; reuse across a semantic change is not." and it predicted the failure at the level above — its unresolved section leaves the revision manifest's contents open, and what shipped is the one alternative 0023 explicitly rejected, "manual semantic version only": one hand-bumped literal per library, `b"xylem-v3"`, shared by every rule in it.

that shipped shape is what makes the bug reachable rather than theoretical, and it makes it reachable across the boundary the project's central claim rests on. bumping `b"xylem-v3"` moves every xylem revision, so every xylem consumer *inside* xylem recomputes — its own key moved too. phloem derives its revisions from its own manifests, which do not move. so a xylem bump left phloem's package-build attempts reusable, revalidating their xylem edges against orphaned keys, and hydrating xylem results derived from the superseded bodies. the comment above `rule_revision` claimed the opposite in present tense: "bumping this invalidates every cached xylem result."

the deeper reason the gap survived four milestones is that nothing could have caught it. every reuse test asserts same-input behaviour: a second run of unchanged inputs reuses, a fresh engine hydrates, two cold compiles are byte-identical, an edit changes an action count. no test compares an incrementally derived result against a from-empty derivation of the same edited state, which is what K-9 actually says. the property has never been executable.

## proposed decision

a recorded pure dependency edge is valid only if the rule it names is registered in this engine at the revision it names. the check is two comparisons against data the record already carries, and it runs before the existing latest-attempt question.

`PureComputationKey` carries `rule_identity` and `rule_revision` as fields, in the clear, beside the digest. `DurableDependency::Pure` records the whole key. the sqlite adapter stores both as first-class columns on `computations`, with indexes over them. so the fix needs no retained request, no re-derivation of a digest, no record-shape change, and no encoding version movement — which under [0048](0048-pre-release-version-pinning.md) would not have moved anyway, but it also does not need the rebuild.

this is deliberately weaker than what 0033 does for an action edge. 0033 re-selects a rule for the recorded interface, calls `plan()` on the recorded inputs, and re-derives the key, because an action's contract is a function of a planner it must re-run. a pure edge needs none of that: the revision is already in the key, so the question "would the consumer's body request this key" reduces to "is this rule still at this revision." re-deriving the digest would additionally verify that the store handed back a key consistent with its own parts, which is a tamper property rather than an incremental-correctness one, and it belongs to whichever record decides how much the engine trusts its adapters.

### an unregistered rule makes the edge invalid

the case the check forces a decision on: a recorded edge names a rule identity this engine has not registered at all.

it is invalid. refusing costs nothing real, because a consumer whose dependency has no rule cannot be evaluated by this engine at any price — if the consumer were not served from the index, evaluating it would request the dependency and fail `E-1101`. serving it from the index would hand back a value the current rule set cannot derive, which is precisely the class of answer K-9 forbids. so the refusal is not a lost optimization; it is the index declining to paper over a rule set that cannot do the work.

the consequence worth naming is that reuse now depends on the registered rule set and not on durable state alone. a caller that registers a subset of the rules a record was written under loses reuse it had before this record, and a cli or a domain crate registering only what it needs is exactly that caller. that is the correct trade and it is a real behaviour change: the M-2 and M-3 hydration evidence comes from test binaries that register everything, which is why it is unaffected.

### the index is maintained at registration

the check needs a revision per rule identity without scanning the rule arena, because rule selection already scans it once per request and the scheduler's cycle check scans the frame stack, and a third per-edge scan is the wrong direction.

`register_rule` maintains an `IndexMap<RuleIdentity, Option<RuleRevision>>` beside the arena push. it is an index over `rules`, derived at the one call site that adds to it, rather than a table an author keeps in step. `IndexMap` rather than `HashMap` because 0021 forbids `HashMap` in crate source and `xtask check-determinism` enforces it — and worth recording, because the guard greps for `HashMap` only, so the `HashSet` this check could plausibly have used would have passed it.

`None` marks an identity two rules registered at different revisions. the map cannot answer for such an identity, and revalidation treats it as invalid rather than picking one, on 0023's asymmetry: false invalidation is acceptable, reuse across a semantic change is not. nothing in the tree registers a duplicate identity today, because identities are module-plus-label and the labels differ; the case is handled because the registration surface permits it, not because it occurs.

## alternatives considered

### retain the request and re-derive the digest, as 0033 does for actions

give `DurableComputation::Pure` the retained interface and inputs the `Action` variant carries, then re-select and re-derive the whole key at revalidation.

this is the shape every reading of "do what 0033 did" produces, and it is more work than the property needs. the `Action` variant retains its request for a reason its own doc comment states — planning is a function of a planner, so the digest must be re-derived rather than trusted — and a pure key has no planner. what re-derivation would add over the revision check is confidence that the digest the store returned is consistent with the identity and revision beside it, which is a claim about adapter integrity. it also costs a record-shape change, an adapter migration, conformance-fixture churn, and a rebuild, and it would land in the same window 0047 needs. deferred to the record that decides how far the engine trusts a state adapter, where it is one case among several.

### recurse into the dependency's own recorded dependency set

make validity transitive: walk the recorded graph beneath each edge, not just the edge.

this is the deeper property and it is genuinely open — a dependency two levels down whose rule was revised is still not caught by this record, because the edge one level up revalidates on its own terms and the walk stops there. it is left out because it is a separate argued decision with at least two mechanisms worth comparing (a recursive walk versus a per-attempt footprint of the (identity, revision) pairs an attempt's whole subtree used, which is a flat set check against the current arena), and because the recursive form has a consequence nothing else in the tree has priced: if validity requires reaching a dependency's records, 0027's collector may not prune any record still reachable from a reusable attempt, so retention's floor becomes the transitive record closure rather than live content. shipping the reachable half now and arguing the deeper half separately is the honest split; shipping neither because the second is hard is how the action-edge reasoning came to be applied to one variant and not the other.

### make the consumer's key carry its dependencies' revisions

fold the dependency set's revisions into the consumer's own `PureComputationKey`, so a revised dependency changes the consumer's key directly and no revalidation is needed.

rejected on the same ground 0031 records for the action case, where 0033 quotes it: a key must be derivable from the request before the dependency set is known. a pure request's dependencies are discovered by running its body, so a key over them cannot be constructed to look anything up. it would also destroy early cutoff, which is the property 0033 exists to preserve — a dependency recomputed to a canonically equal result would move the consumer's key and force it to recompute, where today it stays reusable.

### treat an unregistered rule as valid, preserving today's reuse

skip the check when the identity is absent, so a narrower rule set keeps the reuse it has.

rejected because it converts the hole into a permanent, quieter one: a rule that was deleted rather than revised would keep serving its results indefinitely, and the narrower rule set that "keeps" its reuse cannot evaluate the consumer anyway, so what it keeps is the ability to answer with a value it could not compute. it also makes the check's meaning depend on why an identity is missing, which the record cannot distinguish — deleted, renamed, not yet registered, and registered by another process are the same absence.

## consequences

`Engine` gains `pure_rule_revisions`, maintained in `register_rule`, and `pure_rule_is_registered_at`. `durable_pure_dependency_is_valid` gains the check ahead of its existing body; nothing else in the reuse path changes, and the action arm is untouched.

reuse becomes a function of the registered rule set as well as durable state. the M-2 and M-3 hydration claims hold because their fixtures register every rule; a future cli that registers a subset will see fewer hydrations, and that is the intended behaviour rather than a regression to fix.

xylem's `rule_revision` comment is corrected. it claimed a bump invalidates every cached xylem result, which held for consumers inside xylem and not for phloem; after this record it holds across the boundary. the constant's granularity is unchanged and still 0023's rejected alternative — one edit moves every rule, and a representation change moves none — which 0047 retires by deriving revisions from the declarations an interface names.

### measured

`hydration_is_refused_when_a_dependency_rule_was_revised` builds a root over a leaf, then opens a second engine over the same durable state with the root's rule unchanged and the leaf's revision moved to `leaf-v2` answering `2` instead of `1`. the root recomputes and observes `2`. with the check removed the same test reports `Hydrated` and the value `1`, which is the violation, so the assertion is a falsifier rather than a restatement.

`hydration_is_refused_when_a_dependency_rule_is_not_registered` opens the second engine with the root's rule and not the leaf's, and the evaluation fails `E-1101` naming the missing rule. with the check removed the root is served from the index instead, which is the case the decision above rules out.

the workspace suite is 659 tests, 0 failures. `a_fresh_engine_over_the_same_state_hydrates_the_build` and the two-source build's reuse assertions are unaffected, which is the evidence that the check costs no reuse a fixture registering its rules should have.

## unresolved

transitive validity, argued above and not taken. a dependency at depth two whose rule was revised is still served, and the record that closes it should weigh the recursive walk against a per-attempt revision footprint and state the retention consequence either way.

K-9's own text does not cover what this record fixes. `requirements/kernel.md` states the equivalence "under the same declared inputs," and a rule set is not an input, so the reproduced violation is a violation of a stronger property nobody has written down. `requirements/index.md` says "a semantic change gets a new requirement or an explicit migration," so re-scoping K-9 to quantify over the rule set is a requirement edit this record owes and does not make.

the property is still not executable. what closes that is a differential harness that runs a generated scenario incrementally and from an empty cache and compares the results, and its edit script must generate revision-moving and input-changing edits only — a rule body edited *without* moving its revision retains cached results by design under 0023, so generating that case would fail forever and would be testing the wrong thing. it belongs on `MemoryEngineStateStore`, which the cross-adapter conformance suite already uses as its reference model, because the sqlite adapter costs roughly a millisecond per computation and a suite that runs each scenario twice is a suite nobody runs. it should be host-agnostic rather than gated on linux, or the author's own host compiles it out.

nothing enforces that a revision moves when a body changes. this record makes a moved revision propagate correctly; it does not make an author move one. 0047 derives revisions from the declarations an interface names, which covers interface-level change automatically and leaves body-level change to discipline, and 0038's represented tier is what would make a body's digest its revision.
