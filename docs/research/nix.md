---
schema: design-doc/v1
id: research-nix
title: the nix baseline
summary: which Nix ideas appear worth preserving and which boundaries need to be reconsidered
kind: research
status: researching
evidence: preliminary
created: 2026-03-11
updated: 2026-03-11
tags:
  - research
  - nix
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# the nix baseline

this project starts from Nix because Nix showed that package construction, dependency identity, environments, and operating-system configuration can share one functional model.

the goal is not to reproduce the current Nix interface with cleaner syntax. some of Nix's current direction is shaped by compatibility with its language, store, daemon, package collection, module system, and installed user base. a ground-up design gets to reconsider those boundaries.

## what should survive

the original project notes identify several ideas worth keeping:

- build results depend on declared inputs instead of machine history
- different dependency versions can coexist
- an environment or system is assembled as a value instead of edited in place
- rollback follows from keeping immutable realizations
- the dependency closure can be inspected and distributed
- system configuration and package construction can compose

these are semantic goals. the exact Nix expression language, derivation format, store path scheme, module system, and command-line interface are possible implementations.

## where the current shape fights the goals

the project notes call out recurring problems across Nix, NixOS, and nixpkgs:

- a dynamically typed lazy language delays many mistakes and produces errors far from their source
- package functions, derivations, overlays, modules, options, flakes, and command-line installables expose several overlapping composition models
- the module system has powerful merging behavior, but priorities, defaults, forced values, and free-form values can hide ownership and conflict
- deployment, secrets, and mutable application data sit outside the otherwise unified model
- evaluation performance and editor support suffer because useful semantic information is discovered late
- platform and cross-compilation concepts are difficult to represent consistently across packages
- the package collection carries compatibility and policy decisions that are hard to separate from core semantics

these points are hypotheses to verify against history and current implementation. they should not be treated as a completed diagnosis yet.

## an important distinction

Nix is declarative about derivations and store objects. executing a derivation still runs a procedural builder. that is fine: the command is contained by a declared contract.

system activation is a different problem. moving a live machine between two closures involves mutable external state, ordering, health, and partial failure. treating that transition as another pure build hides information the deployment model needs.

the new design should keep pure construction and live mutation connected without pretending they have identical semantics.

## questions for the historical pass

- why was a lazy dynamically typed language selected, and which early use cases depended on laziness?
- which problems caused the NixOS module system to develop a separate merge and fixpoint model?
- why did overlays become the package-customization mechanism, and which alternatives were tried?
- which store-path and derivation choices were needed for reproducibility, and which were compatibility choices?
- what were flakes meant to repair, which proposals competed with them, and which problems remain outside their scope?
- how did deployment tools around Nix divide responsibility between evaluation, copying, activation, secrets, and rollback?
- what did Guix preserve from Nix, and what did Scheme, grafts, system services, and channels change?

## current project result

keep the functional artifact graph, explicit dependency closure, coexistence of versions, and immutable realizations.

reconsider the language, module algebra, identity model, effect boundary, deployment model, and extension model from first principles.

## sources

- [original AFFiNE project note](https://affine.desk.karolbroda.com/workspace/ac3c3e9d-50c9-46b7-95f2-f4198f11e892/iuvlvERQJXeRG0DPX8zuS)
- [Nix language](https://nix.dev/manual/nix/latest/language/)
- [Nix store](https://nix.dev/manual/nix/latest/store/)
- [NixOS module system](https://nixos.org/manual/nixos/stable/#sec-writing-modules)
- [Eelco Dolstra's publications](https://edolstra.github.io/pubs/)
