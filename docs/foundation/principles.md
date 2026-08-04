---
schema: design-doc/v1
id: foundation-principles
title: design principles
summary: the small set of rules currently constraining the architecture
kind: foundation
status: proposed
created: 2026-02-26
updated: 2026-02-26
tags:
  - principles
  - semantics
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# design principles

this is a shorter version of the larger design checklist that started the project. these are the ones that currently constrain the architecture. the rest should appear as requirements where they become concrete.

## define meaning once

each fact, type, contract, or rule has one canonical semantic definition. documentation, schemas, clients, lock data, plans, provenance, and platform representations are derived from it.

one source of truth does not mean one file or one repository. it means there is one owner and one definition of the meaning.

## keep meaning separate from realization

a service can mean the same thing when run through systemd, a local process supervisor, or some future runtime. a package can keep its identity when represented as a filesystem tree, archive, or remotely stored object.

platform details belong in realizations and adapters. they enter a public contract only when they are actually part of that contract.

## preserve information

composition should keep types, origins, constraints, validation status, errors, and reduced guarantees. turning everything into strings or anonymous maps makes later tooling guess what happened.

the system should be able to answer where a value came from, which rule changed it, and why an implementation was selected.

## make authority and dependency explicit

rules declare what they need. effects declare what they can access. components declare the capabilities they require and provide.

ambient filesystem access, environment variables, credentials, network access, and global registries are hidden dependencies. the normal path should make them impossible.

## reject ambiguous composition

merge and inheritance behavior must be deterministic. incompatible declarations are errors. replacement names the value or behavior being replaced and checks that it still has the expected owner.

registration order is not an acceptable conflict-resolution rule.

## model uncertainty instead of hiding it

declared configuration can make many invalid states unrepresentable. external reality cannot. machines become unreachable, observations go stale, and APIs return incomplete information.

the types should say this directly. `Unknown`, `Unreachable`, `Stale`, and `Unchecked` are more useful than a value that only looks trustworthy.

known failures belong in contracts. faults in the engine or an adapter cross a separate boundary.

## keep the kernel small and the extension model strong

the kernel contains mechanisms that need global coordination. domain meaning lives in libraries.

extensions use typed interfaces, rules, capabilities, and effect handlers. they should be able to define new domain values and new implementations without mutating global state or depending on private hooks.

## make the common path short

safe code does not need to be ceremonial. defaults and inference are useful when they are deterministic, inspectable, and fail closed.

gradual adoption belongs at system boundaries. partially modeled inputs are allowed, but the weaker guarantee stays visible until something validates or refines them.

## one mechanism per concern

each thing the system does has one way to be expressed. a flag that silently changes semantics, a second api that does the same job under different rules, and a knob that relaxes a check are not features. they are the same defect wearing different clothes.

when the obvious path does not fit a new case, the answer is to extend the one path, not to add a second one that competes with it. this is more design work every time. it is the cost of a system where someone reading two pieces of code can tell whether they do the same thing.

an escape hatch is allowed when a case genuinely cannot be expressed. it must be a visibly distinct, marked construct, not a hidden option that makes the normal machinery behave differently. the relaxation stays obvious at the call site.

## optimize for projects that live

the design should be judged with large repositories, partial migrations, failed builds, stale observations, slow editors, changing teams, and old configurations.

performance includes CPU time, memory, startup, rebuild scope, network transfer, editor latency, and operational cost. a fast clean demo is weak evidence.

## preserve public meaning

internal algorithms and storage formats can change. stable semantic identifiers and public contracts should keep their meaning unless a migration explicitly changes them.
