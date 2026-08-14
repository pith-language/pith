---
schema: design-doc/v1
id: decision-0035-link-over-a-list-of-objects
title: a link is over a list of objects
summary: replace the fixed-arity-two link interface with (Toolchain, List&lt;Object&gt;) -> Executable, staging each object at a path derived from its position so the planned contract is a function of the list
kind: decision
status: proposed
created: 2026-06-10
updated: 2026-06-10
tags:
  - build
  - types
  - actions
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0015-interface-rule-selection
    - decision-0026-generic-typed-calculus
    - decision-0031-action-cache-identity
    - decision-0034-discovered-header-dependencies
  supersedes: []
---

# a link is over a list of objects

> closes the item the link rule's own doc comment carried since M-3 began: "fixed arity two for now; linking more than two objects needs a list or tree-valued content variant (0026) and is its own follow-up." the `List` constructor that made this possible landed with [0034](0034-discovered-header-dependencies.md); this record is the follow-up it was waiting for.

## context

xylem's link interface was `(Toolchain, Object, Object) -> Executable`, and the constraint that forced that arity is recorded where it bit: before nominal types landed, two content-producing rules over `Value::Blob` collapsed to `() -> Blob` and collided as `E-1102` (0026's "landed ahead" section). the arity question and the dispatch question were the same question — a link over "some objects" needs a type that says some objects, and the calculus had no list to spell it with.

a two-object link is also not a build. a real program links a directory tree of objects, and the shape the fixed arity pushed the fixture into — one build rule per cardinality, its interface restating how many sources it takes — is the metadata burden 0007's static-graph alternative describes, moved into the type.

## proposed decision

the link interface is `(Toolchain, List<Object>) -> Executable`. the objects arrive as one request input carrying a `List` of `xylem.Object` nominal values, in the order the caller wants them linked.

`plan()` walks the list, verifies each element is an `xylem.Object` (a bare `Value::Blob` is a diagnostic naming the expected nominal type, not a staged input), and stages each at a path derived from its position: `object-0.o`, `object-1.o`, and so on. the driver's argument list is those paths in order, then `-o out`. an empty list fails the plan; a one-object link plans normally, because `cc one.o -o out` is a driver invocation a build can mean.

three consequences fall out of the position-derived path, and they are the reasons for it.

the contract is a function of the list and nothing else. two requests over the same objects in the same order plan byte-identical contracts, derive the same `ActionSpecDigest`, and share one cache entry (0031). the object order reaches the driver and therefore the digest: a reordered list is a different request, which is correct for a linker, since symbol resolution and layout are free to make order observable.

rule selection stays unambiguous, which was the constraint that forced fixed arity in the first place. the second input is `List<xylem.Object>`, which under 0015's exact-equality match is distinct from `List<Blob>`, `List<xylem.CSource>`, and every other element type. nominal identity did not stop mattering when the values moved into a list; the element type is now doing the work two positional nominal inputs did.

the position paths are not stable identities. `object-0.o` names where an object was staged for one invocation, and the same content appears at a different path in a differently ordered link. nothing outside the planned contract sees these paths, and the contract never claims the object *is* that path — it claims the executor stages that content at that path for this run.

## alternatives considered

### a tree-valued content variant

model the input as one `Tree` content identity holding all objects, the way 0030 suggests a toolchain is captured.

rejected for inputs, on the difference between a type and a content identity. a tree is a digest over a manifest; the graph and the cache would see one blob-shaped input and lose the per-object edges that make incrementality fine-grained — touching one source would change the tree identity and re-link through a path that cannot express "five of six objects are unchanged". a tree is the right shape for *outputs* that are directories, and for toolchain capture, where the consumer genuinely wants the whole closure as one thing. enumeration before planning is what a link needs, and enumeration is what a list is.

### pairwise folding over the binary interface

keep `(Toolchain, Object, Object) -> Executable` and link N objects as a chain or tree of binary links.

rejected on what it does to the cache and the driver. a chain makes N-1 link actions where a build means one, each a cache entry keyed over an intermediate executable that no consumer wanted, and the fold order becomes part of the build's meaning while being invisible in its description. it also produces real executables at intermediate steps, which a linker will happily make and a build cannot use. the arity-two rule exists because a *driver invocation* takes a list of objects; the honest interface is that list.

### a variadic interface form

extend 0015 so an interface can declare "zero or more inputs of type T".

rejected on cost with no compensating need. 0015's match is exact equality on canonical interfaces, which is what makes refusal of ambiguity cheap and diagnostics exact; a variadic form turns match into a cardinality question per selection and gives every rule-selection diagnostic a plural case. `List<T>` already expresses the thing under the existing equality, and the calculus reserved the constructor. nothing in a build, package, or system domain has asked for variadic inputs that a list does not carry.

## consequences

`types::link_request` takes an iterator of content identities and `LinkAction::plan` takes the second input as a list. the fixture's build rule became `List<CSource> -> Executable` in the same move, so the whole vertical slice — build description, compiles, link — now carries lists end to end, and one registration of the build rule serves any cardinality instead of one rule per count.

the link action's revision moves to `xylem-v2` with the other xylem rules (0034 moved them together; this lands inside the same unreleased bump), so no durable state survives the interface change to migrate.

### measured

`crates/xylem/tests/two_source_build.rs`, over the nix-store gcc 15.2.0: a build of four sources links four objects in one driver invocation — nine actions (four discoveries, four compiles, one link), and the executable computes over all four objects (exit 123). the unit tests hold the planner to the invariants above: one staged path per object in list order, the arguments in the same order, a reordering of the same objects planning a different contract, an empty list failing the plan, and a bare blob in the list failing with the expected nominal type named.

## unresolved

the list is homogeneous and positional. a build that needs to say "these objects, plus this archive, plus this linker script" has nowhere to put the non-object inputs, and the record that takes that question is the one that designs link *options* — flags, libraries, scripts — as part of the link description. that is M-3-adjacent library design, not calculus design, and it is not started here.

linker order sensitivity is asserted, not modeled. the digest says a reordered link is a different request; whether two orders that produce byte-identical executables should share a cache entry is the equality-pruning question across action results, which is 0033's machinery and unchanged by this record.
