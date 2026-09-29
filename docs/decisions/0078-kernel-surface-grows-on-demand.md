---
schema: design-doc/v1
id: decision-0078-kernel-surface-grows-on-demand
title: the kernel and pith:core gain an interface only when a domain being written needs it, in the most general form that serves it, and under the grant model
summary: new engine, effect and pith:core interfaces are added when a named domain cannot be written without them; each is stated in domain-neutral terms, placed in a library when the kernel inclusion rule allows, extends an existing mechanism before adding one, and makes any new authority grantable and visible; a standard library grows the same way
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - planning
  - kernel
  - extensibility
relations:
  informed_by: []
  depends_on:
    - foundation-principles
    - design-kernel
    - decision-0001-generic-kernel
    - decision-0072-action-plans-are-authorized-against-consumer-grants
    - decision-0077-pre-release-pith-is-experimental
  supersedes: []
---

# the kernel and pith:core gain an interface only when a domain being written needs it, in the most general form that serves it, and under the grant model

## context

the milestones so far built the kernel together with three first-party domains, `xylem`, `phloem` and
`stele`, and planned more. the next milestone instead makes it possible for a domain written outside the
repository to run (0071 to 0076), and leaves further first-party domains for later.

this record decides how the kernel and `pith:core` grow once outside authors use them. without a rule,
interfaces get added because they might be useful, or shaped around the first domain that asked.

## decision

### an interface needs a domain

a new engine interface, effect vocabulary item, or `pith:core` type or function is added when a domain
being written cannot be expressed without it. the record or commit that adds it names that domain and
what it could not do.

### the general form

the interface is stated in terms that do not mention the domain that asked for it, as `ActionSpec` is:
it describes running a program with declared inputs, outputs, capabilities and network policy, and
knows nothing about compilers. a request for "compile C" becomes whatever general interface compiling C
needs, if the existing ones are not enough.

### library before kernel

the inclusion rule in [the kernel design](../design/kernel.md) still decides placement: an interface goes
in the kernel only if leaving it to each domain would make composition incorrect, unsafe or impossible to
explain. otherwise it belongs in a library, which later includes a pith standard library written in `.pi`
and components.

### extend before adding

when an existing mechanism almost fits, it is extended. a second mechanism for the same concern needs a
record explaining why the first cannot be extended.

### new authority is grantable

an interface that lets a domain do more than compute, such as reading a path, reaching a host, or holding
a secret, is added together with its grant form under
[0072](0072-action-plans-are-authorized-against-consumer-grants.md). a domain cannot use it without a
grant, and what was granted appears in plans, provenance, and for components in their imports.

### experimental while pre-release

interfaces added under this rule are experimental under
[0077](0077-pre-release-pith-is-experimental.md), so a first version that turns out wrong can be replaced
before the first release.

## alternatives considered

designing a complete standard library before external domains exist would decide its shape without users.

adding interfaces as domains ask, without the generality rule, would leave `pith:core` shaped around its
earliest users.

## evidence

each interface added under this rule cites the domain that needed it. the first is expected to be the
external `example-domain` witness of 0073, which should need nothing beyond what 0071 to 0076 define.

## unresolved

the shape of the standard library, including which parts are `.pi` and which are components, is left
until domains exist that use it.
