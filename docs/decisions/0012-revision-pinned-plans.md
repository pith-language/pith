---
schema: design-doc/v1
id: decision-0012-revision-pinned-plans
title: revision-pinned plans
summary: a plan is a derived value bound to the observation revisions it was built from; execution validates those pins before acting
kind: decision
status: proposed
created: 2026-04-14
updated: 2026-04-14
tags:
  - deployment
  - plans
  - effects
relations:
  informed_by:
    - research-deployment-and-state
  depends_on:
    - decision-0002-declarative-semantics
    - decision-0003-explicit-effects
    - decision-0005-separate-identities
  supersedes: []
---

# revision-pinned plans

## context

requirement S-8 says one-shot application and continuous reconciliation "can execute the same desired-state and transition semantics." the deployment research pass over Terraform, Kubernetes, Pulumi, and Crossplane found that no surveyed system achieves this. Terraform is plan-based and treats the observation snapshot as durable truth until apply. Kubernetes is reconciliation-based and has no durable plan. Crossplane chose reconciliation and inherited the absence of a plan preview.

the tension is not on the surface. it is in what an observation means. in plan mode, an observation is a snapshot a plan is built from, expected to hold until execution. in reconciliation mode, an observation is a current-state input expected to change. a design claiming to support both modes has to answer what happens when an observation goes stale between the moment a plan is derived and the moment it is acted on.

the deployment research note records this as the open question gating the rest of the deployment layer.

## proposed decision

a plan is a derived, inspectable value. for every observation it was derived from, it carries the revision of that observation. executing the plan requires those revisions to still hold at the moment of execution. if a revision has moved on, the plan is invalidated and re-derived against current observations.

an observation revision is whatever an adapter can attest to: a resource version, a generation counter, a content digest, an etag, a timestamp with source. the engine does not interpret it. it checks equality.

the two execution modes become the same mechanism with different policy. one-shot application derives a plan from current observations, asks for approval, validates the pins, acts. continuous reconciliation derives a plan, validates the pins immediately, acts, repeats. the difference is the cadence of re-derivation and what triggers it. the semantics of plan, observation, and execution are shared.

## worked example

a plan to resize a database targets managed object `db-abc123` at revision `v42`, the revision the adapter returned when the database was observed during planning. the plan records that pin.

between approval and execution the database is changed through the cloud console. it is now at `v43`. at execution time the executor asks the adapter for the current revision of `db-abc123` and receives `v43`. the pin breaks. the plan is not applied against a world it was not built for. it is returned to the planner, re-derived against `v43`, and the new plan is what gets shown next.

this is the failure Terraform's model does not catch. Terraform builds a plan from a refresh and then acts on the assumption that the refreshed state still holds. when it does not, the documented `provider produced inconsistent result after apply` failures leave a state file that matches neither the old world nor the new one.

## what this commits the model to

the observation type gains a revision field the executor depends on. it is not metadata. adapters that cannot provide a meaningful revision cannot support pinned plans and must declare that, which surfaces in the plan as a weaker guarantee under requirement T-6.

plans are immutable values. they can be stored, signed, compared across runs, and shown for approval. a stored plan that can no longer be applied because its observations have moved is reported as such, not silently re-executed.

the roll-forward recovery pattern from Terraform is replaced by re-derivation. when a plan is invalidated, the engine re-derives against current observations and presents the new plan. the user never reasons about a half-applied plan against a state that does not match reality, because the plan and the observations are distinct values and the engine never silently substitutes one for the other.

continuous reconciliation is the same mechanism with a different policy: re-derive on a schedule, on observation change, or on desired-state change, and apply immediately because the pins are fresh by construction. one-shot application inserts a human approval step between derivation and execution and validates pins at execution time.

## alternatives considered

### snapshot plans without revision pins, the Terraform shape

a plan is derived from observations and executed without recording what it depended on. simpler, and the source of the documented failure modes. an apply against a world that has moved produces inconsistent results and a state file recording the partial outcome. rejecting the pinless plan is rejecting the class of bug.

### reconcile without plans, the Kubernetes shape

skip the plan artifact and act on the diff between desired and current state on each pass. robust to missed events and restarts, which is why Kubernetes chose it. gives up the inspectable plan that requirements S-4 and T-6 depend on. a user must be able to see what a change will do before it happens.

### re-derive on every observation change

treat any revision change as invalidating every plan that depends on it and re-derive immediately. correct, and effectively continuous reconciliation at high cadence. wasteful when observations change faster than a human can approve plans, and it does not distinguish between an observation change that affects a given plan and one that does not. the pin set scopes invalidation to the observations a specific plan actually used.

### transactional plans with rollback

plans declare transactional boundaries and the engine rolls back on failure. not rejected in principle. the research pass found that the platforms deployments target generally do not provide the distributed transactions this would require. where an adapter can offer a real transaction, the plan should expose it. where it cannot, the plan should say so rather than claim atomicity. compatible with per-adapter transactional contracts. does not assume them.

## consequences

the observation type gains a load-bearing revision field. adapters that cannot attest a revision declare that limitation, and it surfaces in the plan.

plan derivation is more expensive than a naive diff, because the planner tracks which observations each step depended on. in practice the dependency graph already records this, and the plan's pin set is a projection of the subgraph the plan was derived from.

the partial-failure story is honest by construction. a plan that breaks some pins and not others can be re-derived whole, or the library can apply the subset whose pins hold. that is policy, not mechanism, and it lives in the transition contracts of the involved domains.

## unresolved

pinning granularity needs prototypes. pinning an entire plan to the freshest observation of every object it touches is conservative. pinning per-mutation to the specific objects each mutation reads is precise but requires the planner to record its read set.

whether a partially-invalidated plan can be applied in pieces or must be re-derived whole is a policy question that depends on the transition contracts involved.

how revisions from adapters with no natural notion of generation (a DNS record, a file on disk, a config edited by hand) are constructed and attested is per-adapter work. the model is agnostic to the revision's internal structure. the adapter still has to produce one.
