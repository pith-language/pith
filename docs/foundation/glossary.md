---
schema: design-doc/v1
id: foundation-glossary
title: glossary
summary: working meanings for terms used across the design
kind: foundation
status: active
created: 2026-03-25
updated: 2026-09-27
tags:
  - glossary
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# glossary

these definitions are part of the design. ambiguous words should be split instead of stretched until they cover unrelated concepts.

## action

a bounded external computation with declared inputs and outputs. a compiler invocation is an action.

## artifact

a domain value backed by immutable content. the kernel stores content; a library gives that content artifact meaning.

## capability

typed authority or behavior required by a rule or effect. capabilities can describe execution, network access, a platform, secret access, observation, or mutation.

## declaration

source that evaluates to typed values, rules, constraints, or requests. ordinary declaration evaluation does not perform external effects.

## effect

an explicitly represented interaction whose result cannot be derived from ordinary immutable inputs alone.

## computation identity

the identity of a specific rule application and the inputs relevant to its result. two builds of the same semantic value under different inputs have different computation identities.

## content identity

the identity of immutable bytes or a canonical structured value, given by digest. two values with the same content share content identity by construction.

## external identity

the identity assigned to an object by a system outside this tool.

## kernel

the shared engine for values, rules, dependency tracking, effects, identity, immutable storage, provenance, and diagnostics.

## managed-object identity

the identity of the durable external object a deployment owns and mutates across observations, mutations, and platform re-creation. distinct from external identity, which is an identifier the platform assigns and which can change while the managed object persists. constructed and maintained by the deployment library and its adapters; the kernel provides the primitive and provenance machinery.

## mutation

an effect that changes external state.

## opaque

an effect category for work that has not been modeled into pure, action, observation, or mutation. the engine records that effectful work occurred but does not know its category, declared inputs, or authority. opaque is the visibly distinct adoption path permitted by the principles.

## observation

a time- or revision-bound statement about external state, including its source and uncertainty.

## nondeterminism

a dependency on a source of variation outside the declared deterministic inputs: wall-clock time, randomness, filesystem readdir order, locale, address-space layout, and the rest of the reproducibility lineage. tracked as a capability dependency in the graph. the kernel provides one `Nondeterminism` primitive; the granular taxonomy is a library refinement.

## realization

one concrete value or arrangement satisfying a declaration and its constraints.

## function value

a rule's identity plus arguments already bound to it. it is ordinary data, so it can be part of a computation key. a lambda is an anonymous rule identified by its body digest. (added 2026-09-27, [0082](../decisions/0082-rules-are-called-by-name-and-functions-are-values.md).)

## input

a parameter of a project: another project or a typed value, with a default, that a consumer can replace with `with`. (added 2026-09-27, [0080](../decisions/0080-inputs-are-parameters.md).)

## output

a named value a project provides to the command line and to other projects. it is a root in the graph and computes nothing until asked for. (added 2026-09-27, [0083](../decisions/0083-outputs-and-commands.md).)

## path

a value naming a location. `Path` is inside the project and identified by its content, `RelPath` is inside a tree, `HostPath` is on the machine. (added 2026-09-27, [0081](../decisions/0081-paths-are-values.md).)

## project

one file holding a project's inputs, declarations and outputs, plus the files it names with `include`. (added 2026-09-27, [0079](../decisions/0079-a-project-is-one-file.md).)

## rule

a typed recipe for deriving a value from inputs and requests tracked by the graph.

> edit, 2026-09-27: a rule is called by name and is a function value; see [0082](../decisions/0082-rules-are-called-by-name-and-functions-are-values.md).

## semantic identity

the stable identity of what a value represents, independent of its current contents, file location, or external provider address.

## world

a domain-level collection of desired values and constraints. `World` is not currently a kernel primitive.

