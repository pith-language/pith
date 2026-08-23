---
schema: design-doc/v1
id: foundation-scope
title: scope
summary: what the project should own, what belongs in libraries, and where the implementation stops
kind: foundation
status: proposed
created: 2026-02-25
updated: 2026-08-24
tags:
  - scope
  - product
relations:
  informed_by:
    - research-nix
  depends_on:
    - foundation-problem
    - foundation-principles
  supersedes: []
---

# scope

the project is an integrated build, package, system-management, and deployment tool built on one typed declarative model.

that sentence describes the product people should get. it does not describe the boundary of the kernel. the kernel is much smaller and does not know what a package, service, machine, or deployment is.

## what it should be able to do

someone should be able to use the tool to build one executable without defining a package or machine. the same build output can later become part of a package, development environment, filesystem tree, operating-system image, application release, or deployment.

the first-party distribution should cover:

- general-purpose builds
- package and dependency resolution
- development environments
- service and system composition
- local and remote deployment
- one-shot application and continuous reconciliation
- secrets, identity, provenance, and policy integration

these areas share values and rules. none of them is the parent abstraction of all the others.

## what the kernel owns

the kernel owns the pieces that have to be shared for correctness:

- typed immutable values
- canonical declared types and typed rule interfaces
- rule evaluation
- dependency tracking and incrementality
- explicit effects and capabilities
- stable identities and content digests
- immutable value and byte storage
- caching and concurrency
- provenance, diagnostics, and graph queries

a feature belongs here when letting each domain implement it separately would make cross-domain composition incorrect, unsafe, or impossible to explain.

source modules, imports, and interface linkage belong to the loader. the kernel consumes their elaborated declarations and typed rules.

## what libraries own

ordinary libraries define domain meaning. the official distribution will include first-party libraries for builds, packages, environments, services, systems, deployments, secrets, and policy.

first-party means maintained, documented, tested, and released with the project. it does not mean privileged. if the official package library needs a private engine hook, the extension model is missing something.

## what adapters own

adapters connect domain values and effects to an implementation. examples include Linux, systemd, launchd, OCI, a cloud API, a scheduler, a secret store, an artifact store, and a remote execution service.

an adapter can narrow the guarantees it provides. it cannot quietly change the meaning of the value it receives.

## what is outside the project

the project does not need to become its own operating-system kernel, hypervisor, cloud, CI service, secret vault, monitoring database, source-control system, or container format.

it may drive or integrate with all of those. their details should stay behind typed capabilities and adapters.

the project is also not an ordered task runner. low-level procedural actions will exist because compilers and external APIs are procedural. those actions sit behind declared inputs, outputs, effects, and authority. users should not have to turn the desired system into a hand-written sequence.

## first implementation boundary

Linux is the first platform because it gives the project a complete place to prove the model. Linux should remain a target of the first-party system library, not a collection of language primitives.

the first useful vertical slice should:

1. evaluate a typed project definition
2. build a small multi-language project hermetically
3. resolve and lock its dependencies
4. assemble a development environment
5. compose the application into a Linux service and filesystem
6. produce an immutable system artifact
7. calculate and apply a deployment to one machine
8. show why every dependency, rebuild, configuration value, and deployment action exists

remote execution, fleets, continuous controllers, additional operating systems, and complicated state migrations can follow. the first version still needs the architecture to leave room for them.
