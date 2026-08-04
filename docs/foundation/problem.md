---
schema: design-doc/v1
id: foundation-problem
title: the problem
summary: why build, package, system, and deployment tools should share semantics without sharing domain assumptions
kind: foundation
status: proposed
created: 2026-02-24
updated: 2026-02-24
tags:
  - problem
  - motivation
relations:
  informed_by:
    - research-nix
    - research-build-systems
    - research-deployment-and-state
  depends_on: []
  supersedes: []
---

# the problem

software moves through several tools before it runs. one tool resolves dependencies, another builds them, another creates an environment, another describes a machine, and another deploys it. each tool rebuilds part of the same graph with its own identities, configuration rules, caches, errors, and extension model.

Nix showed how much becomes possible when package construction, dependency closures, environments, and operating-system configuration share a functional model. it also accumulated several semantic layers that do not line up cleanly: language values, package functions, derivations, module merges, store objects, activation scripts, and external deployment tools.

build systems such as Bazel and Buck2 have stronger incremental engines and remote execution models. configuration languages such as Dhall, Nickel, and CUE explore typing, totality, contracts, and constraints. Terraform and Kubernetes model external state, but make provider resources or controller-owned API objects central. none of these systems supplies the whole model we are looking for.

the project should find the smaller mechanism underneath these domains. it needs to be strong enough that libraries share dependency tracking, effects, identity, caching, provenance, and diagnostics. it also needs to stay generic enough that today's idea of a package or service does not become a permanent engine primitive.

the practical problem is therefore two-sided:

- domain tools duplicate correctness machinery and lose information when they hand work to each other
- universal tools tend to hardcode one domain model or become so low-level that every extension rebuilds the missing semantics

the design work is to find the useful boundary between those failures.

