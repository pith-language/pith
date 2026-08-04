---
schema: design-doc/v1
id: design-identity-and-storage
title: identity and storage
summary: separate semantic, computation, content, external, and managed-object identities over immutable storage
kind: design
status: proposed
created: 2026-04-10
updated: 2026-04-29
tags:
  - identity
  - storage
relations:
  informed_by:
    - research-artifacts-and-trust
    - research-nix
    - research-deployment-and-state
  depends_on:
    - decision-0005-separate-identities
    - decision-0013-managed-object-identity
    - decision-0014-reproducibility-properties
    - design-values-and-types
  supersedes: []
---

# identity and storage

the system uses several identity types because a single hash or provider address cannot preserve meaning through every operation.

semantic identity names what a value represents across implementations and refactors.

computation identity names a rule application and the inputs relevant to its result.

content identity addresses immutable bytes or a canonical structured value.

external identity names an identifier a platform assigns, such as a cloud resource id. it can change while the underlying object does not.

managed-object identity names the durable external object a deployment owns and mutates across observations and mutations. it is continuity of ownership, separate from external identity: a platform may delete and recreate the object under a new external identifier while the managed object persists. the kernel provides the primitive and the provenance machinery; the deployment library and its adapters construct and maintain it, including the rules for adoption, replacement, and rename.

links among these five identities are recorded as provenance. the types prevent accidental substitution.

## immutable storage

the kernel stores blobs, trees, and serializable values by digest. packages, filesystem images, container manifests, release bundles, and compiler outputs are interpretations built by libraries.

materialization is independent from identity. a graph can refer to remote content without copying it to the local filesystem.

## provenance

the engine records the rule, inputs, transformations, capabilities, trust state, and diagnostics associated with a value while it is derived.

signatures and supply-chain attestations refer to this evidence. a signature over content does not silently imply bit-for-bit reproducibility or trusted dependencies. reproducibility is a distinct claim: content-addressed identity is provided by construction, clean-build equivalence is the engine's invariant under declared inputs, and bit-for-bit reproducibility is a property of the build instructions and environment that the engine verifies by comparison and records, but does not produce.

