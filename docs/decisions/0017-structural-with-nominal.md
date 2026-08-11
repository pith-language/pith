---
schema: design-doc/v1
id: decision-0017-structural-with-nominal
title: structural types by default, nominal by declaration
summary: match by shape unless a type explicitly declares identity, in which case structural matching does not apply
kind: decision
status: proposed
created: 2026-04-21
updated: 2026-05-22
tags:
  - types
  - language
relations:
  informed_by:
    - research-configuration
  depends_on:
    - decision-0010-typed-pure-language
    - decision-0013-managed-object-identity
    - foundation-principles
  supersedes: []
---

# structural types by default, nominal by declaration

> superseded by [0026: a generic structural type calculus](0026-generic-typed-calculus.md). the structural-default and nominal-by-declaration mechanism below is carried forward unchanged as one section of the larger calculus; 0026 adds the rest of the constructor set (records, sums, generics, effect types, uncertainty types) and settles the questions this record left open. this record stays in the repository as the history of the structural-versus-nominal decision.

## context

decision 0010 left nominal versus structural typing open as a separate question. values-and-types names it as one of the things the exact type calculus still has to settle.

values cross repository and version boundaries constantly. a build input from one repository, a package from another, and deployment state from a third have to compose without a central type registry governing them. at the same time, some types carry real identity. a package id, a machine, a service, and a capability token are not interchangeable with an unrelated type that happens to share their fields.

pure structural matching composes freely and risks silent confusion when two unrelated types share a shape. pure nominal typing prevents confusion and requires a shared declared identity for every cross-boundary value, which recreates a global registry the rest of the design has rejected.

## proposed decision

structural typing is the default. two values are compatible when their shapes match, without a shared declaration.

a type may declare nominal identity. a nominal type only matches values of the same declared type. it does not match a structurally identical type from elsewhere, and the declaration is what makes that visible.

nominal identity is for types where confusion is dangerous or where the type carries real identity. package identifiers, machine references, service handles, capability tokens, and managed objects from decision 0013 are the obvious cases. an ordinary record of configuration values stays structural, because nothing is lost when it matches by shape.

a nominal type does not forbid intentional conversion. where a path from one type to another is meaningful, it is expressed as a rule or an explicit operation, not by the type system silently widening. the nominal declaration prevents accidental confusion. it does not prevent deliberate transformation.

this leaves one type system. nominal types are a specialization within it, not a parallel model. there is one way types are defined and one way they compose, with an optional declaration that tightens matching for the types that need it.

## alternatives considered

### structural only

all types match by shape.

composes cleanly across boundaries no party controls. two unrelated types that happen to share fields become interchangeable, which is the postcode-versus-customerid problem at scale. for types that carry identity or authority, silent structural match is a security and correctness hazard.

### nominal only

every type has declared identity and only matches itself.

safe and explicit. it requires shared type declarations across repositories and versions, which over time becomes a central registry of canonical types. that registry conflicts with the no-global-plugin-registry principle the kernel already adopts, and it makes ad-hoc composition across boundaries painful.

### structural records plus a separate nominal system

two type systems, one for plain data and one for identity-bearing types.

avoids forcing one model to serve both cases. it is two mechanisms for the same concern, which the principles reject. contributors would have to learn which system a given type lives in, and the boundary between them would be a recurring source of confusion.

## consequences

composition across repositories and versions works by shape by default. a value defined in one place can be consumed in another without a shared declaration, as long as the shapes agree.

types that carry identity declare it. the declaration is a visible, searchable marker that this type is not interchangeable with its structural lookalikes. provenance and capability tracking can rely on nominal identity where it matters, and the rest of the system gets structural flexibility.

the managed-object identity from decision 0013 is expressed as nominal types. the binding between a semantic object and its external identity is part of the type's declaration, not a convention the engine has to infer.

library authors have to decide which of their types are nominal. most are not. the discipline is small, but it is a real choice each domain library makes, and the criteria need to be written down somewhere a library author can find.

## unresolved

the syntax for declaring a nominal type is open. the mechanism is settled, the surface is not.

the criteria for when a type should be nominal need to be stated, probably in the values-and-types design doc rather than here. identity-bearing, authority-carrying, and confusion-dangerous are the current categories. they need to be precise enough that two library authors make the same call about the same kind of type.

how nominal identity interacts with versioning, when a nominal type evolves across versions of a library, needs work alongside the schema-evolution question already open in decision 0010.
