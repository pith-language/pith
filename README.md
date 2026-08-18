# Pith

Pith is an experimental computation kernel for build, package, environment,
and system tooling.

The project explores what these domains can share without making any one of
them the kernel's model. Values, rules, dependencies, effects, identities,
content, and provenance belong to the common layer. Builds, packages, services,
and systems are defined by libraries over it.

This repository contains both the design notebook and the implementations used
to test that design.

## Shape of the project

```text
  xylem             phloem             stele
  builds       packages and envs      systems
       \             |             /
        typed values, rules, and requests
                       |
                  Pith kernel
       graph, effects, identity, storage
                       |
            executors and state adapters
```

The first-party domain libraries use the same registration and execution
interfaces available to another library. The kernel itself has no built-in
concept of a package, service, machine, or deployment.

## Current state

Pith is a working prototype, not yet a tool for general use. There is no source
language or complete user-facing workflow, and the command-line interface is
currently limited to a small evaluation stub and content-store materialization.

The implemented slices currently cover:

- typed rule selection and incremental graph evaluation
- content-addressed blobs and trees, persistent engine state, and reuse
- bounded local actions with declared inputs and outputs
- a Linux local executor using Landlock and seccomp confinement
- build, package, development-environment, and immutable-system prototypes

Linux system activation is the next planned slice. Deployment, external-state
reconciliation, and the source language remain design work. The
[milestones](docs/planning/milestones.md) document keeps the detailed boundary
between what has been demonstrated and what is still proposed.

## Reading the design

Start with the [problem](docs/foundation/problem.md), [scope](docs/foundation/scope.md),
and [design overview](docs/design/overview.md). The [principles](docs/foundation/principles.md)
explain the constraints behind the architecture, and the
[documentation index](docs/index.md) maps the full notebook: requirements,
research, decisions, and planning.

Most documents describe a direction rather than released behavior. Their
status distinguishes accepted decisions, proposals, research, and living
references; the milestones link implementation claims to their evidence.

## Working with the repository

The development environment is pinned with Nix:

```sh
nix develop
just test
```

Run `just` to list the available checks and development commands. `just check`
runs formatting and static analysis; `just ci` runs the full local CI suite.

Linux-specific executor and system fixtures compile to zero tests on other
platforms, so a successful run there does not exercise those paths.

Pith is licensed under [Apache 2.0](./LICENSE). Third-party notices are collected
in [NOTICE](./NOTICE).
