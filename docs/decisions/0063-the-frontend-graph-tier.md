---
schema: design-doc/v1
id: decision-0063-the-frontend-graph-tier
title: frontend computations key imported semantic surfaces and return user failures as values
summary: three pure frontend rules share canonical source and import inputs, fetch semantic interface surfaces through NeedBlob, derive their revisions from the elaborator, and keep invalid source in reusable completed values
kind: decision
status: proposed
created: 2026-08-24
updated: 2026-08-24
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

The graph cannot elaborate an imported coordinate from an ABI digest alone. The digest proves which
semantic interface is required, but it is not an address for the declaration table. Conversely, using
the imported `.pi` blob as the address puts documentation and formatting in a dependent's computation
key and removes the cutoff M-12 exists to establish.

The planning documents leave that conflict unresolved. The module-surface plan rejects a second served
interface product, while the frontend architecture requires a deep interface surface behind a digest.
The implementation must pick one before `bodies-of` can resolve an imported type.

## proposed decision

### inputs

The frontend registers three pure rules with identical input types and distinct nominal outputs:

```text
interface-of : (Source, ImportEnv) -> ModuleInterface
bodies-of    : (Source, ImportEnv) -> Bodies
index-of     : (Source, ImportEnv) -> Index
```

`Source` contains the module identity and a path-sorted list of `(path, source ContentId)`. The module
identity cannot be derived from a directory path: paths are source layout, while the module name enters
declaration coordinates and the ABI. `FrontendSource::new` owns sorting and rejects a repeated path, so
the public request path cannot produce two keys for one source set.

`ImportEnv` is a binding-sorted list of `(binding, module, ABI digest, interface-surface ContentId)`.
The ABI digest and surface identity are different types and have different jobs. The ABI records the
semantic contract the caller expects. The content identity lets the frame fetch its encoding. After the
fetch, the frame verifies the encoded module, derives and compares its ABI, and verifies its blob
identity before admitting it to scope. A repeated binding is refused during construction.

The surface encoding contains the module, its sorted imported ABI pairs, its canonical declaration
table, and its sorted provided `(effect category, interface)` pairs. It contains no source positions,
documentation, rule labels, or bodies. Its storage identity therefore has the same invalidation
predicate as the ABI for all represented fields, while remaining retrievable from the content store.
This is a second derived artifact, reversing the module-surface plan's earlier rejection because the
graph supplies the consumer that plan did not have.

`interface-of` returns the canonical surface bytes with the shallow identity, tier, ABI, and
diagnostics. A pure frame has no content-publication step, and adding one after 0062 closed the step and
IR constructor sets would move the body encoding for a frontend-only convenience. The driver publishes
the returned bytes through `put_blob`; the resulting identity is the one placed in dependents'
`ImportEnv`. No unbacked or custom-domain `ContentId` is returned.

### evaluation and failures

Each rule uses the content-only synchronous engine path. Its frame yields one `NeedBlob` per source file
and then one per imported surface. It performs one module-level elaboration after all bytes arrive.
Files are merged into one type arena so declarations may refer forward across file boundaries; a gap in
the merged offset space keeps an end-of-file point span attached to the file before the boundary.

Invalid UTF-8, parse errors, and elaboration errors are completed data. Diagnostics carry the source
blob identity and local offsets. A rule whose interface does not elaborate is absent from `rules` and
appears in `incomplete` with its own diagnostics. A failed attempt is reserved for an invalid imported
surface or a frontend invariant violation.

### revisions and crate boundaries

The three rule revisions derive from the semantic version, crate version, body-IR encoding version, and
their canonical interface. The author-maintained semantic version is paired with a repository check over
the manifests and source trees of `pith-syntax`, `pith-hir`, `pith-elaborator`, and `pith-loader`. A source
change without a refreshed record fails the aggregate repository check; review decides whether the
semantic version must also move.

The crate split follows the data flow. `pith-syntax` lexes and parses, `pith-hir` owns parsed surfaces,
merged module layout, and position data, `pith-elaborator` owns import scope, declaration and interface
elaboration, and ABI derivation, and `pith-loader` owns host binding and the graph adapter. The graph and
the in-process editor path call the same elaborator.

## alternatives considered

### key imports only by source ContentId

Rejected because a documentation or formatting edit changes every dependent's key. That is the failure
the ABI cutoff is designed to prevent.

### key by ABI digest and recover the surface from ambient state

Rejected because the engine has no mapping from a semantic digest to stored bytes. Adding a mutable
side index makes elaboration depend on state absent from the computation key. The surface content identity
is explicit instead.

### add a pure content-publication step

Rejected in this round because it changes the kernel step vocabulary and represented-body encoding for
one frontend transfer. Returning flat bytes makes publication an explicit driver effect without adding a
new evaluator capability.

### merge the three rules into one

Rejected because the measured cutoff holds and the outputs have different consumers and retention needs.
The separate nodes keep `Index` out of rule revisions and let interface readers avoid bodies.

## prototype evidence

The graph-tier integration test evaluates `interface-of(alpha)`, publishes the exact returned surface,
and feeds its ABI and content identity to `bodies-of(beta)`. Renaming alpha's rule changes
`bodies-of(alpha)` while leaving its interface surface and ABI byte-identical; beta's second evaluation is
reused. Changing alpha's nominal representation moves both semantic identities and recomputes beta. A
fresh engine over shared durable state hydrates beta's unchanged attempt.

The same suite covers multi-file cross-reference elaboration, source and import canonicalization,
duplicate-key refusal, interface-surface round trips, and a broken rule completed with its source-bound
diagnostic and `incomplete` entry.

## unresolved

The milestone text names a represented-body edit as the cutoff witness. M-12 has no represented-body
source spelling — 0062 deliberately landed hand-built IR and M-13 owns notation — so the current witness
uses a rule-label edit that changes `bodies-of(alpha)` but not its interface. The exact body-text case must
be added when M-13 makes that edit expressible; M-12 remains underway until then.

`ModuleInterface` temporarily carries the surface bytes. If a later kernel mechanism can publish derived
content without widening the represented step vocabulary, the value can return to the architecture's
shallow `{identity, tier, abi, surface}` shape with `surface` as a stored blob identity.
