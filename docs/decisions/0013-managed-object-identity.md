---
schema: design-doc/v1
id: decision-0013-managed-object-identity
title: managed-object identity
summary: a fifth identity type for the durable external object a deployment owns and mutates across observations
kind: decision
status: proposed
created: 2026-04-15
updated: 2026-04-15
tags:
  - identity
  - deployment
relations:
  informed_by:
    - research-deployment-and-state
  depends_on:
    - decision-0005-separate-identities
    - decision-0012-revision-pinned-plans
  supersedes: []
---

# managed-object identity

## context

decision 0005 separates four identity types: semantic, computation, content, external. the split was drawn from the build and artifact domain. a package has semantic identity, a specific build of it has computation identity, each output has content identity, and external identity names an identifier a platform assigns.

the deployment research pass exposed a concept that does not fit any of the four.

consider a postgres primary that a deployment owns and mutates. the cloud assigns it `db-abc123`. the plan mutates it, the adapter observes it. a platform maintenance event deletes it and recreates it as `db-def456`. same database, from the deployment's point of view. different external identifier.

the four existing types do not answer whether that is the same object. it is not semantic identity: two databases with the same meaning (primary postgres for service X) can be distinct managed objects, and the same managed object can change meaning during a migration. it is not external identity: `db-abc123` is an external identifier and it just changed while the managed object did not. it is not computation or content identity, which are for build outputs. the managed object is mutable.

what deployment needs to name is continuity of ownership across observation, mutation, and platform re-creation. that is its own dimension.

## proposed decision

the identity model gains a fifth type: managed-object identity. a managed object is the durable thing a deployment claims to own on a target platform and reasons about across observations and mutations.

the five identity types are:

- semantic identity: what a value represents across implementations and refactors
- computation identity: a specific rule application and its relevant inputs
- content identity: immutable bytes or a canonical structured value
- external identity: an identifier assigned by an external system
- managed-object identity: the durable external object a deployment owns and mutates

provenance records relations among all five. the type system prevents accidental substitution, as with the original four.

managed-object identity is constructed and maintained by the deployment library and its adapters, not by the kernel. the kernel provides the identity primitive and the provenance machinery. the deployment library decides what counts as the same managed object across a platform re-creation, an adoption, or a rename.

## why overloading the existing types fails

the strongest case against stretching the existing four is the external-identity overload, because it is the one prior systems actually chose and the one with the clearest documented failure.

Kubernetes controllers reconcile on labels and `metadata.uid`. a pod deleted and recreated by the platform gains a new uid. treating uid as the managed-object identity makes every pod restart look like a different object, which is why controllers ignore uid and reconcile on label selectors. that is an admission that uid is not the managed-object identity.

the other overloads fail by analogous collapse. stretching semantic identity loses the distinction between two replicas of a service, which share a role but are distinct objects with distinct failure modes. stretching content identity erases the mutable-immutable distinction the type exists to preserve, since a managed object changes by definition.

managed-object identity is its own dimension because ownership and continuity are their own question, separate from meaning, computation, content, and platform-assigned naming.

## what this fixes

requirements S-3 (explicit ownership) and S-5 (transition contracts) require a stable notion of "the same object across observations." without managed-object identity the model overloads one of the four existing types, and each overload has a documented field failure.

it also gives decision 0012 something to pin. a mutation in a revision-pinned plan targets a specific managed object at a specific revision. without managed-object identity, the plan has no stable thing to refer to.

## alternatives considered

### collapse managed-object into external identity

external identity could mean "the external thing we care about" rather than just "an identifier a platform assigns." the smallest change and the most likely to cause silent bugs. Terraform conflates ownership with the state-file address, Pulumi with the URN, Kubernetes with the uid. all three work until the platform re-creates the object with a new identifier, or the identifier stays the same while the underlying object does not. keeping the two distinct makes that case representable.

### a separate ownership registry instead of an identity type

skip the new identity type and maintain a registry mapping semantic identities to external identifiers with ownership recorded there. close to what Terraform's state file is. the research pass recorded the costs: the registry becomes a source of truth it cannot guarantee, drift between it and reality is invisible until refresh, and partial-apply failures leave it inconsistent. a registry is an implementation of managed-object identity tracking, not a replacement for the concept. making it a typed identity keeps it inside the provenance and query model rather than alongside it.

### let each deployment-like domain define its own

leave managed-object identity entirely to the deployment library with no kernel concept. consistent with the principle that domain meaning lives in libraries. the argument against is the same one decision 0001 makes for the kernel generally: ownership and continuity are needed by every deployment-like domain, and letting each define them separately produces incompatible ownership models that cannot share provenance, queries, or plan semantics. the identity primitive belongs in the kernel. the policy for constructing and maintaining it belongs in the library.

## consequences

decision 0005 is amended to name five identity types rather than four. the glossary and the identity-and-storage design doc name five identity types.

adapters gain a responsibility. they attest not only to the observed state of a managed object and its revision, but to the managed object's identity across observations. an adapter that cannot establish continuity, because the platform provides no stable handle, declares that. the plan surfaces the weaker guarantee.

the deployment library's adoption, import, and rename workflows become operations on managed-object identity rather than ad-hoc state-file manipulations. the Pulumi aliasing problem from the research note becomes a library-level question: how is managed-object identity carried across a refactor of the desired-state declarations? that is a library decision built on this primitive.

## unresolved

the construction rules for managed-object identity need library prototypes. Pulumi's source-derived URNs fail silently on rename and require explicit aliasing. a design that wants managed-object identity to survive refactors has to say where the identity lives when it is not derived from source, and how that storage is kept consistent across a team and a fleet.

whether managed-object identity can ever be content-derived, for platforms where the object's identity genuinely is its content (a git commit, a content-addressed blob), or whether it must always be a separate assigned handle, is an open per-adapter question.

the relationship to revision-pinned plans is direct. a mutation in a plan pins the revision of a specific managed object. the precise shape of that reference, and what it means for a managed object to exist before any observation of it has been made, needs to be worked out alongside the first deployment library prototype.
