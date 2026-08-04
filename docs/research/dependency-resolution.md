---
schema: design-doc/v1
id: research-dependency-resolution
title: dependency resolution
summary: early comparison of SAT, PubGrub, and ASP approaches to versions, variants, providers, and explanations
kind: research
status: researching
evidence: preliminary
created: 2026-03-04
updated: 2026-03-04
tags:
  - research
  - dependencies
  - solvers
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# dependency resolution

dependency resolution is more than choosing the newest version that satisfies a range. system packages add virtual capabilities, conflicts, replacements, architectures, installed state, optional recommendations, variants, and policy. source-oriented package systems add toolchains and target platforms. binary reuse adds another preference over otherwise valid solutions.

the solver should therefore be separate from the package data model. the model defines candidates, constraints, capabilities, preferences, and evidence. one or more solver implementations find a realization.

## libsolv

libsolv translates package requirements and conflicts into satisfiability rules. it was built for operating-system repositories, where the installed set, available repositories, capabilities, obsoletes, and transaction choices all matter.

its implementation combines a compact repository representation with a SAT solver. it records a decision tree for introspection and can suggest ways to handle an unsatisfiable request.

this makes libsolv relevant to package and system management. its model still carries assumptions from traditional repositories and installed-package transactions that may not fit every domain.

## PubGrub

PubGrub was designed around version ranges and useful failure explanations. it adapts conflict-driven clause learning, records the causes of incompatibilities, and uses the derivation graph to explain why no solution exists.

it builds package-specific ideas into the algorithm. at most one version of a package is implicit, and terms operate on version ranges instead of expanding every version into unrelated Boolean variables.

those choices make explanations natural for language package managers. they conflict with a system that allows several versions of the same semantic package in different graph positions unless identity and isolation make those positions distinct solver subjects.

## Spack

Spack's concretizer turns an abstract package specification into a concrete dependency DAG. the current implementation translates package constraints into Answer Set Programming and uses Clingo.

HPC packaging needs versions, compilers, variants, target microarchitectures, virtual providers, and reuse of installed or binary packages. a solver must find a valid graph and optimize preferences among many valid graphs.

this is closer to the intended cross-compilation and variant problem than a single-version application lockfile. it also shows the cost of a richer model: policy and optimization become part of concretization.

## current direction

the kernel should not contain one package solver.

it should provide stable values for constraints, candidate provenance, solver requests, and explanations. the first-party package library can choose a solver suited to its semantics. placement or deployment libraries may use another solver without translating their domain into package versions.

locks record the selected realization and the evidence needed to reproduce selection under the same candidate universe. they do not replace the original constraints.

## alternatives still open

- one general solver for versions, features, toolchains, placement, and policy
- domain solvers behind one typed constraint and explanation protocol
- a PubGrub-style package solver plus separate selection passes for platforms and variants
- an ASP model that solves the complete package and toolchain graph together
- deterministic resolution rules with limited backtracking for a smaller first implementation

the choice depends on whether multiple versions may coexist within one realization, how provider capabilities work, and how much optimization belongs in reproducible resolution.

## questions

- what exactly is the solver subject: name, semantic identity, graph position, or capability instance?
- are preferences part of the lock's meaning or merely a way to choose one valid result?
- how can minimal conflict explanations retain the original source of every constraint?
- can a lock remain valid when new candidates appear in a repository?
- how are host, build, and target platforms represented without copying Nixpkgs' platform complexity?
- when can a previously built binary satisfy a source specification with different provenance?

## sources

- [libsolv](https://github.com/openSUSE/libsolv)
- [openSUSE SAT solver documentation](https://doc.opensuse.org/projects/satsolver/SLE11SP3/html/index.html)
- [PubGrub algorithm](https://dart.googlesource.com/pub.git/%2B/f27dcfdb/doc/solver.md)
- [Spack concretizer glossary](https://spack.readthedocs.io/en/latest/glossary.html#term-concretizer)
- [Using Answer Set Programming for HPC dependency solving](https://arxiv.org/abs/2210.08404)

