---
schema: design-doc/v1
id: decision-0014-reproducibility-properties
title: separate the reproducibility properties
summary: distinguish content identity, clean-build equivalence, and bit-for-bit reproducibility, and only claim the third when it has been verified
kind: decision
status: proposed
created: 2026-04-16
updated: 2026-04-16
tags:
  - reproducibility
  - identity
  - artifacts
relations:
  informed_by:
    - research-reproducibility
  depends_on:
    - decision-0005-separate-identities
    - decision-0013-managed-object-identity
  supersedes: []
---

# separate the reproducibility properties

## context

the notebook sometimes writes about reproducibility as if it were one property the engine provides. requirement A-6 ("clean-build equivalence") is currently doing the work of three distinct claims, and the identity-and-storage design doc slides between "a signature over content does not silently imply reproducibility" and language that treats reproducibility as an engine guarantee.

the Reproducible Builds project, which is the only place the bit-for-bit property has been operationalized at scale, defines it precisely:

> a build is reproducible if, given the same source code, build environment, and build instructions, any party can recreate bit-by-bit identical copies of all specified artifacts.

reproducibility is a property of the source, the environment, and the instructions. it is not a property of the build system. the build system can verify it. it cannot produce it. a compiler that embeds timestamps, a build script that reads readdir order, or a tool that injects a UUID produces non-reproducible output regardless of how disciplined the engine is.

Nix makes this distinction in practice even where the marketing blurs it. Nix can detect that a derivation is reproducible by building it twice and comparing. content-addressed derivations let it substitute a verified-reproducible result. but Nix does not make derivations reproducible. the reproducibility percentage of nixpkgs is a measured quantity, not a designed-in constant. it is tracked. it moves.

conflating these properties risks claiming a guarantee the engine cannot provide and obscuring the one it can.

## proposed decision

the design recognizes three distinct properties and uses separate terms for each.

content-addressed identity is the property that two values with the same content have the same identity. it is a property of the storage and identity model, provided by construction for blobs, trees, and serializable values.

clean-build equivalence is the property that incremental, cached, local, and remote execution produce results equivalent to a clean execution under the same declared inputs and platform contract. it is a property of the rule engine and the executors. this is requirement A-6, and the invariant Skyframe and DICE were built to preserve. the kernel provides it when executors honor the declared action contract.

bit-for-bit reproducibility is the property that two independent builds under the same declared inputs produce byte-identical output. it is a property of the build instructions and the build environment, not of the engine. the engine verifies it by building twice and comparing content identities, and refuses to assert it when unverified. it cannot produce it.

an artifact's provenance records which of these have been established. content identity is always present. clean-build equivalence is asserted by the executor's contract. bit-for-bit reproducibility is present only when the engine has performed the comparison or accepted an attestation whose trust state is recorded.

## what this commits the model to

the engine distinguishes "this artifact is reproducible" from "this artifact was built once." the distinction is visible in provenance and in any supply-chain attestation derived from it. a signature over an artifact's content does not imply bit-for-bit reproducibility. a bit-for-bit attestation is its own claim, as requirement T-5 already says for trust claims generally.

executors that sandbox actions against the Reproducible Builds determinism rules claim a stronger guarantee than those that do not, and the plan and provenance surface the difference. an action marked `hermetic` without evidence of determinism discipline is a claim, not a measured fact. this is the same principle the effects design doc states: marking an action hermetic is not evidence that it was hermetic.

the determinism rules are concrete and are the useful artifact the Reproducible Builds community produced over roughly a decade. do not embed the maker or the place of making. do not embed a timestamp unless it is clamped to SOURCE_DATE_EPOCH, the community's specification for a source timestamp:

```
SOURCE_DATE_EPOCH=1722739200
```

the value is a unix timestamp representing the source's last-modification time. tools that embed timestamps must clamp them to a value no later than SOURCE_DATE_EPOCH. formatting to a human-readable date is deferred to runtime. beyond timestamps, the rules cover filesystem readdir order, locale, timezone, umask, usernames and hostnames, uninitialized memory, address-space layout, embedded randomness, parallelism races, and profile-guided optimization. these are the documented ways two builds under "the same inputs" produce different bytes.

the build library should adopt these as defaults for the actions it defines and surface them as part of the declared action contract.

## alternatives considered

### treat reproducibility as a single engine guarantee

collapse the three properties into one and claim the engine produces reproducible builds. the strongest marketing position and the one the engine cannot back. the Reproducible Builds project exists because real builds embed nondeterminism in ways no engine can remove without the build's cooperation. claiming the unified guarantee sets up every nondeterministic action as a counterexample and erodes trust in the guarantees that do hold.

### claim reproducibility for hermetic actions

any action run under a hermetic sandbox is reproducible. closer, still an overclaim. hermeticity and reproducibility are related, not identical. an action can be hermetic and still embed a timestamp from a source it legitimately read. an action can be reproducible without being hermetic if its nondeterministic inputs happen to be fixed across builds. keeping the two claims separate lets provenance record which was established.

### verify reproducibility for every action

build every action twice and compare, as a default. the most rigorous option and it doubles build cost. a reasonable default for actions that publish artifacts for wide distribution, and the wrong default for a local incremental build during development. the decision makes verification available and records its result. policy on when to verify belongs in the build library.

## consequences

requirement A-6 is narrowed to clean-build equivalence, which is the property the engine actually provides. a new requirement should record the bit-for-bit verification property framed honestly: the engine verifies reproducibility by comparison and records the result. it does not produce it.

the artifact and identity model gains a provenance field for verified-reproducible status. the supply-chain story in T-4 and T-5 inherits this. a reproducibility attestation is a distinct claim, derived from the graph, with its own trust state.

the build library's default action contracts should adopt the Reproducible Builds determinism rules and SOURCE_DATE_EPOCH. this is library policy, not kernel semantics, but it follows directly from the framing. if reproducibility is a property of the build instructions, the first-party build library should ship instructions that have it.

## unresolved

how reproducibility verification interacts with remote execution needs specification. comparing content identities from two remote builds is straightforward. comparing a local and a remote build requires agreement on the action contract including the determinism rules, and the remote executor's trust state affects whether the comparison counts as evidence.

the representation of "verified reproducible" in the content identity scheme needs work. whether a verified-reproducible artifact shares content identity with its independently-built twin by construction, or whether the equality is recorded separately, gets worked out alongside the first action prototype.

whether the project adopts SOURCE_DATE_EPOCH directly or defines its own equivalent is a build-library decision. direct adoption is the path of least friction against existing tooling, and divergence needs a reason.
