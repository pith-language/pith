---
schema: design-doc/v1
id: decision-0020-nix-as-adapter-not-substrate
title: reuse Nix infrastructure as adapters, not as the substrate
summary: treat the Nix store, binary caches, and nixpkgs as adapter-backed inputs behind the kernel's storage and effect boundary, never as the engine or identity model
kind: decision
status: proposed
created: 2026-04-24
updated: 2026-04-24
tags:
  - nix
  - adapters
  - storage
  - adoption
relations:
  informed_by:
    - research-nix
  depends_on:
    - decision-0001-generic-kernel
    - decision-0003-explicit-effects
    - decision-0004-first-party-without-privilege
    - decision-0005-separate-identities
    - decision-0014-reproducibility-properties
    - decision-0019-effect-categories-and-nondeterminism
  supersedes: []
---

# reuse Nix infrastructure as adapters, not as the substrate

## context

the project starts from Nix and keeps its semantic goals, but it is not a reskin of the current Nix interface. decisions 0001, 0004, 0005, 0010, 0014, and 0016 together commit the project to a different engine, identity model, language, and reproducibility framing.

none of that requires ignoring what Nix has already built. the Nix ecosystem has a working content-addressed store, a widely deployed remote cache format, and a package collection of roughly a hundred thousand recipes. rebuilding all of that before the system is useful would delay adoption for no design benefit.

the question is where the reuse boundary sits. this record sets it.

## proposed decision

Nix infrastructure may be reused only behind typed adapter boundaries that sit under the kernel's identity, effect, and provenance model. it is never the engine, never the identity scheme, and never the evaluator.

three adapter roles are in scope:

- a local content-store adapter over a Nix store, providing blob and tree storage by digest
- a remote cache adapter over a Nix binary cache, providing substitution and trust evidence from `narinfo` signatures
- a package adapter over nixpkgs, realizing derivations as `Opaque` actions with fixed outputs

in every case the kernel retains semantic, computation, external, and managed-object identity, capability discipline, and provenance. the adapter carries content identity and whatever weaker guarantees the source provides, recorded honestly.

the principle, in one line: Nix is one realization among possible others, never the definition of what a value, build, or identity means.

## what this commits the model to

storage is pluggable below content identity. a graph can be served from a Nix store, a plain content-addressable store, an OCI registry, or something custom, without the layers above changing.

the `Opaque` category from decision 0019 is the on-ramp for unmodeled imports. a nixpkgs derivation enters the graph as `Opaque`, gets a content identity, and is usable like any other value, with the weaker guarantee visible in provenance rather than hidden.

substitution and trust evidence flow through provenance. a substituted artifact records where it came from, what signed it, and that substitution is not reproducibility, consistent with 0014.

## alternatives considered

### Nix as the substrate

the kernel could run on the Nix daemon, evaluator, and store path scheme.

this collapses the five identity types onto one path scheme, makes the Nix runtime privileged inside the engine (which 0004 forbids for first-party code), and gives up the typed pure language 0010 chose. it is the option this decision rejects.

### no Nix reuse

the project could build its own store, cache, and package collection from nothing.

this keeps the design pure and maximizes distance from Nix's current boundaries. it also delays usefulness past the point most adopters would wait for, and discards a working content-addressed store and remote cache format for no architectural reason.

### wrap Nix to make it more pure

a typed layer could sit on top of the Nix evaluator and present the new model as a facade.

the purity the project wants is not a sandboxing problem. it is a typing and composition problem. a facade over a dynamically typed lazy evaluator inherits that evaluator's late errors and weak tooling, which is what the project exists to leave behind.

## consequences

the project gets day-one access to a mature package collection, a working store, and a widely available remote cache, which materially lowers the cost of adoption for people coming from Nix.

the cost is the weaker guarantee carried by anything behind an `Opaque` boundary. that weakness is a feature of the on-ramp, not a defect, and 0014 requires it be surfaced rather than laundered into a full content-identity or reproducibility claim.

the adapter set is extensible by third parties under the same authority checks as first-party code, per 0004.

## unresolved

the honesty boundary around input-addressed versus content-addressed derivations needs specification. most nixpkgs paths are addressed by declared inputs, not measured content. a store path lookup against a binary cache is therefore a computation-identity claim presented as a content fetch. the adapter must record the distinction rather than silently treat the result as content-addressed.

whether nixpkgs-via-adapter is a permanent feature or a migration scaffold to be deprecated domain by domain is a strategic choice left open. the design supports either; a prototype will inform which is worth the investment.

the precise interface each adapter implements, the trust semantics of `narinfo` signatures in the kernel's attestation model, and the fall-back ordering between local store, remote cache, and build are left to the storage and build library designs.
