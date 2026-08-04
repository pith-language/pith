---
schema: design-doc/v1
id: design-overview
title: design overview
summary: how the kernel, ordinary libraries, first-party domains, and adapters fit together
kind: design
status: proposed
created: 2026-04-02
updated: 2026-04-02
tags:
  - architecture
  - overview
relations:
  informed_by:
    - research-nix
    - research-build-systems
    - research-configuration
    - research-deployment-and-state
  depends_on:
    - foundation-scope
    - decision-0001-generic-kernel
    - decision-0009-peer-first-party-domains
  supersedes: []
---

# design overview

the project has four layers of responsibility.

```text
first-party domains
build, package, development environments, services, system management, deployment, secrets, policy

ordinary libraries
constraints, state models, transitions, formats, policies

generic kernel
values, rules, dependencies, effects, capabilities, identity, storage, provenance

adapters
executors, operating systems, runtimes, clouds, secret stores, remote APIs
```

the first-party domains make the project useful. they are ordinary clients of the kernel. an external library can replace one, extend one, or define a different domain without a compiler patch.

a build rule can produce immutable content without knowing about packages. a package library can add metadata, constraints, and distribution. a system library can use the package as one input to a filesystem or service. a deployment library can compare a desired domain value with observations and derive mutations.

the shared graph preserves the connection between these results. it does not force them into one hierarchy.

## evaluation and execution

source declarations evaluate without ambient effects. they produce values and requests.

rules derive more values through the incremental graph. when a result requires external work, a rule returns or invokes an explicit effect through a capability.

pure computations, bounded actions, observations, and mutations have different cache and scheduling behavior. keeping them in one graph does not make their semantics identical.

## frontends and the kernel

the first implementation is expected to have a purpose-built typed language. the kernel should consume a typed semantic representation rather than depend on the source syntax.

this leaves room for generated definitions, editor tooling, and other frontends while keeping one evaluation and provenance model.

## first platform

Linux is the first complete target. Linux services, users, filesystems, boot configuration, and process supervision belong to a first-party domain library and its adapters.

the kernel remains free of Linux paths, users, permissions, process models, and init-system concepts.

