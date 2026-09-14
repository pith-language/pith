---
schema: design-doc/v1
id: decision-0063-the-frontend-graph-tier
title: frontend computations key imported semantic surfaces and return user failures as values
summary: three pure frontend rules share canonical source and import inputs, fetch semantic interface surfaces through NeedBlob, derive their revisions from the elaborator, and keep invalid source in reusable completed values
kind: decision
status: proposed
created: 2026-08-24
updated: 2026-08-28
tags:
  - language
  - graph
  - identity
  - diagnostics
relations:
  informed_by:
    - planning-frontend-architecture
  depends_on:
    - decision-0022-sync-core-async-scheduler
    - decision-0023-rule-and-cache-identity
    - decision-0047-the-declaration-table
    - decision-0048-pre-release-version-pinning
    - decision-0053-parse-diagnostics-carry-their-source
    - decision-0061-the-declaration-artifact
    - decision-0062-the-ir-constructor-set
  amends:
    - decision-0061-the-declaration-artifact
  supersedes: []
---

# frontend computations key imported semantic surfaces and return user failures as values

> amends [0061](0061-the-declaration-artifact.md): a dependent still keys on semantic input rather
> than imported source bytes, but the graph also needs the content identity of an interface surface
> so `NeedBlob` can retrieve the declarations the ABI digest commits to.

## context

the graph cannot elaborate an imported coordinate from an ABI digest alone. the digest proves which
semantic interface is required, but it is not an address for the declaration table. conversely, using
the imported `.pi` blob as the address puts documentation and formatting in a dependent's computation
key and removes the cutoff M-12 exists to establish.

the planning documents leave that conflict unresolved. the module-surface plan rejects a second served
interface product, while the frontend architecture requires a deep interface surface behind a digest.
the implementation must pick one before `bodies-of` can resolve an imported type.

## proposed decision

### inputs

the frontend registers three pure rules with identical input types and distinct nominal outputs:

```text
interface-of : (Source, ImportEnv) -> ModuleInterface
bodies-of    : (Source, ImportEnv) -> Bodies
index-of     : (Source, ImportEnv) -> Index
```

`Source` contains the module identity and a path-sorted list of `(path, source ContentId)`. the module
identity cannot be derived from a directory path: paths are source layout, while the module name enters
declaration coordinates and the ABI. `FrontendSource::new` owns sorting and rejects a repeated path, so
the public request path cannot produce two keys for one source set.

`ImportEnv` is a binding-sorted list of `(binding, module, ABI digest, interface-surface ContentId)`.
the ABI digest and surface identity are different types with different jobs: the ABI records the
semantic contract the caller expects, the content identity lets the frame fetch its encoding. after the
fetch, the frame verifies the encoded module, derives and compares its ABI, and verifies its blob
identity before admitting it to scope. a repeated binding is refused during construction.

> amended by [0067](0067-local-module-workspaces.md): the `module` half of each entry is the declared
> subject of the imported module, and the surface encoding and validation compare subjects. the binding
> half remains and continues to feed only scoping and elaboration.

the surface encoding contains the module, its sorted imported ABI pairs, its canonical declaration
table, and its sorted provided `(effect category, interface)` pairs. it contains no source positions,
documentation, rule labels, or bodies. its storage identity therefore has the same invalidation
predicate as the ABI for all represented fields, while remaining retrievable from the content store.
this is a second derived artifact, reversing the module-surface plan's earlier rejection because the
graph supplies the consumer that plan did not have.

`interface-of` returns the canonical surface bytes with the shallow identity, tier, ABI, and
diagnostics. a pure frame has no content-publication step, and adding one after 0062 closed the step and
IR constructor sets would move the body encoding for a frontend-only convenience. the driver publishes
the returned bytes through `put_blob`; the resulting identity is the one placed in dependents'
`ImportEnv`. no unbacked or custom-domain `ContentId` is returned.

### evaluation and failures

each rule uses the content-only synchronous engine path. its frame yields one `NeedBlob` per source file
and then one per imported surface. it performs one module-level elaboration after all bytes arrive.
files are merged into one type arena so declarations may refer forward across file boundaries; a gap in
the merged offset space keeps an end-of-file point span attached to the file before the boundary.

invalid UTF-8, parse errors, and elaboration errors are completed data. diagnostics carry the source
blob identity and local offsets. a rule whose interface does not elaborate is absent from `rules` and
appears in `incomplete` with its own diagnostics. a failed attempt is reserved for an invalid imported
surface or a frontend invariant violation.

### revisions and crate boundaries

the three rule revisions derive from the semantic version, crate version, body-IR encoding version, and
their canonical interface. the author-maintained semantic version is paired with a repository check over
the manifests and source trees of `pith-syntax`, `pith-hir`, `pith-elaborator`, and `pith-loader`. a source
change without a refreshed record fails the aggregate repository check; review decides whether the
semantic version must also move.

the crate split follows the data flow. `pith-syntax` lexes and parses, `pith-hir` owns parsed surfaces,
merged module layout, and position data, `pith-elaborator` owns import scope, declaration and interface
elaboration, and ABI derivation, and `pith-loader` owns host binding and the graph adapter. the graph and
the in-process editor path call the same elaborator.

## alternatives considered

### key imports only by source ContentId

rejected because a documentation or formatting edit changes every dependent's key. that is the failure
the ABI cutoff is designed to prevent.

### key by ABI digest and recover the surface from ambient state

rejected because the engine has no mapping from a semantic digest to stored bytes. adding a mutable
side index makes elaboration depend on state absent from the computation key. the surface content identity
is explicit instead.

### add a pure content-publication step

rejected in this round because it changes the kernel step vocabulary and represented-body encoding for
one frontend transfer. returning flat bytes makes publication an explicit driver effect without adding a
new evaluator capability.

### merge the three rules into one

rejected because the measured cutoff holds and the outputs have different consumers and retention needs.
the separate nodes keep `Index` out of rule revisions and let interface readers avoid bodies.

## prototype evidence

the graph-tier integration test evaluates `interface-of(alpha)`, publishes the exact returned surface,
and feeds its ABI and content identity to `bodies-of(beta)`. renaming alpha's rule changes
`bodies-of(alpha)` while leaving its interface surface and ABI byte-identical; beta's second evaluation is
reused. changing alpha's nominal representation moves both semantic identities and recomputes beta. a
fresh engine over shared durable state hydrates beta's unchanged attempt.

the same suite covers multi-file cross-reference elaboration, source and import canonicalization,
duplicate-key refusal, interface-surface round trips, and a broken rule completed with its source-bound
diagnostic and `incomplete` entry.

## amendment after M-13

the represented-body cutoff witness now exists.
`a_body_edit_moves_bodies_and_leaves_the_interface_surface_byte_identical` edits a written body in A,
recomputes `bodies-of(A)`, serves `interface-of(A)` as `Reused`, and serves the dependent `bodies-of(B)` as
`Reused`; A's interface surface and ABI are byte-identical across the edit. the representation edit beside
it is the invalidating control. M-12 is complete.

## unresolved

`ModuleInterface` temporarily carries the surface bytes. if a later kernel mechanism can publish derived
content without widening the represented step vocabulary, the value can return to the architecture's
shallow `{identity, tier, abi, surface}` shape with `surface` as a stored blob identity.
