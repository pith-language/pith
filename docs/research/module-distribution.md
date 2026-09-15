---
schema: design-doc/v1
id: research-module-distribution
title: module distribution boundaries
summary: targeted evidence for module distribution, covering registry metadata, authority, lock replay, and the dependency context of compatibility claims
kind: research
status: researching
evidence: preliminary
created: 2026-09-06
updated: 2026-09-06
tags:
  - modules
  - registries
  - locks
relations:
  informed_by:
    - research-language-frontend
    - research-index-formats
    - research-dependency-resolution
    - research-artifacts-and-trust
    - research-dispatch
  depends_on:
    - research-method
  supersedes: []
---

# module distribution boundaries

this is a targeted follow-up to the existing research, checked against primary documentation on
2026-09-06. it supports [module distribution](../planning/modules/distribution.md); it accepts no decision and
claims no implementation measurements. current manuals establish the mechanisms below, not the historical
reasons for adopting them. the earlier lineage notes keep that job.

## metadata before source acquisition

the pressure is resolving a dependency graph without downloading every candidate's source. the
invariant is that a resolver consumes explicit metadata and never discovers dependencies through
execution.

[Cargo's index](https://doc.rust-lang.org/cargo/reference/registry-index.html) carries each version's
direct dependency requirements and archive checksum. it supports both a Git index and individual-file
HTTP acquisition, so transport can change without moving requirements out of the index.

[Bazel's registry](https://bazel.build/external/registry) is a directory or static HTTP service containing
per-version manifests and source locators. its documentation says resolution reads the registry's module
file, which may differ from the source archive's. that is a defined authority rule, not evidence that
the two files agree, and it sharpens the earlier [index-formats](index-formats.md) note's disagreement
question.

pith's proposed result: adopt metadata-first resolution and start with a directory registry whose
snapshot can be distributed through Git. generate requirements from the published manifest and refuse a
downloaded manifest that disagrees with the selected metadata. this exact-match policy is pith's choice,
not cargo's or bazel's demonstrated behavior. keep archive integrity separate from the normalized module
tree's content identity: different archive packaging can carry the same source tree.

the credible alternative remains exact revision dependencies with no index or solver. that simplifies
acquisition but gives up ranged transitive resolution, which M-14 currently promises. choose explicitly
in the opening record; neither content addressing nor the package domain's choice decides it for modules.

## routing does not establish publishing authority

the pressure is locating private and public modules without allowing registry search order to choose a
subject's provider. the invariant is one effective source binding per subject.

[CUE's registry configuration](https://cuelang.org/docs/reference/command/cue-help-registryconfig/)
maps module-path prefixes to registries, chooses the longest matching prefix, and has a default registry.
it also supports an explicit unavailable registry. pith can adopt explicit routing while retaining its
stricter exact-domain mapping and missing-binding refusal. CUE's fallback and longest-prefix rules are
not precedents for pith's no-ranking claim.

a configured route answers which registry a consumer trusts for a domain. it does not answer which
publisher that registry permits to publish the domain. a locally declared `example/name`, a repository
URL, and a content digest each fail to establish that permission independently. this is a distinction
in the proposed pith model, not a security guarantee inferred from the CUE manual.

the bounded choice for M-14 is registry-scoped authority: configuration explicitly appoints a registry
for a domain; that registry's publication adapter checks an explicit publisher-to-domain grant. the
temporary registry can exercise this with a local administrative policy and a trusted publisher
identity supplied by its adapter. such a fixture measures authorization; it does not implement remote
login or a global ownership service. a source's manifest cannot appoint its own trusted publisher.

## a lock has both immutable and mutable inputs

the pressure is reproducing a previous selection while registries continue to change. the invariant is
that ordinary replay cannot silently become an update.

[Bazel's lockfile](https://bazel.build/external/lockfile) records registry-file hashes, negative lookups,
and selected yanked versions. its error mode refuses missing or stale resolution information and does
not update the lock. negative lookup recording partly follows from its registry search order, which
pith rejects; it still demonstrates that absence and mutable admission facts can influence a resolver.

pith's proposed result: distinguish consumption of an existing lock from refreshing its universe.
consumption validates current manifest requirements and bindings against recorded selections and
verified content. update acquires a fresh, finite metadata snapshot and runs resolution. cache missing
metadata as explicit data when it affects an answer; do not interpret a network failure as absence.
define whether yanks exist in this first registry format. if they do, include their status in the
refresh input; if they do not, refuse that unsupported metadata rather than consulting it implicitly.

the alternative, consulting the live registry during every check, makes a lock's usefulness depend on
network availability and unrelated publication activity. reusing a lock must also work after engine
state collection; a universe digest alone cannot reconstruct a snapshot whose bytes have disappeared.

[Go's module reference](https://go.dev/ref/mod#authenticating) distinguishes hashes already recorded
locally from verification through its checksum database. the existing pith
[trust research](artifacts-and-trust.md) and decision 0044 already choose pinned-checkpoint inclusion
verification for their prototype. M-14 needs a module binding format and admission tests on those terms,
not a second transparency system. a Git commit pins content, but does not by itself prove who may publish
a domain or authenticate a version tag's initial association with that commit.

## two compatibility assumptions need executable counterexamples

these were deductions from the current pith code and decisions, not external findings. both have since
been run, and [0068](../decisions/0068-published-surfaces-are-context-bound.md) records what they measured: the
first confirmed and extended to the interface surface, the second confirmed but relocated. the ambiguity
is cross-module, and within one module the duplicate interface is refused at elaboration instead.

first, `pith-elaborator/src/abi.rs` includes imported subject/ABI pairs in a module's ABI. if module A
allows two versions of B with different ABIs, unchanged A source can elaborate to different ABIs.
therefore a registry's single ABI field per A version cannot be interpreted as the ABI under every
allowed dependency resolution. the earlier module-system proposal requires that field but does not
define its dependency context.

the next record should distinguish source-release identity from an elaboration artifact's identity.
the recommended model publishes source and a surface built against a recorded dependency context;
consumers re-elaborate against their selected closure. an advertised surface is directly reusable only
when its dependency ABI context and elaborator revision match. the advertised surface and its context
remain covered by candidate metadata identity; they are not an unchecked map beside the universe.
a fixture with unchanged A and two admissible B versions must establish the exact behavior before
the candidate and lock encodings are fixed.

second, decision 0015 refuses multiple matching rules. adding a provider can therefore change an existing
request from one match to ambiguity. adding a public name may also collide with an unqualified imported
name. the module-system proposal's unconditional classification of an added rule as minor is too strong.

the recommended differ reports exact structural changes separately from compatibility under a declared
dependency context. a provider addition is potentially breaking; a concrete consumer check can show a
collision, while a successful check proves only that consumer. likewise, an unchanged interface and a
changed body do not prove behavioral compatibility. publication can enforce conservative surface-version
rules without claiming a proof of all future program behavior. the missing evidence is two small pith
fixtures, not another general comparison of package managers.


## Rust boundaries for consumer authority

Rust's [visibility rules](https://doc.rust-lang.org/reference/visibility-and-privacy.html) let a public
route expose observations while keeping construction inside validation. a borrowed route then cannot
outlive the configuration it names. [PhantomData](https://doc.rust-lang.org/std/marker/struct.PhantomData.html)
carries a type parameter without storing an owner value; an internal trait's associated constant derives
runtime provenance from the project or user marker. this makes a swapped layer a type error at the merge
call, rather than a reversed precedence boolean. these features are available on the pinned toolchain.

the [ordered-map entry API](https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html) separates
vacant insertion from occupied replacement. the implementation uses that distinction to retain the prior
declaration on a duplicate and both declarations on an override. no additional collection library or
runtime-dispatched policy trait is needed. these are implementation choices for the authority boundary,
not evidence that a registry signature or a source digest has been verified.
