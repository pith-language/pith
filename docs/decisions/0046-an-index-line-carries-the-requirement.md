---
schema: design-doc/v1
id: decision-0046-an-index-line-carries-the-requirement
title: an index line carries the requirement, and a dependency's artifact is a library the graph builds
summary: the index line grows requirement clauses so a candidate says what it depends on and one resolve request walks the edge with no fetches — the cargo and alpine position against PyPI's sidecar, with the disagreement rule stated before the surface exists; a dependency's artifact is a library, objects plus offered headers, produced by a second package-build interface and consumed in-graph by the dependent's build naming its dependencies as (tree, build); the dependent's compiles see the dependency's headers as request data through a declared third input on xylem's compile entry — the first xylem interface change in six records, argued against engine-wide universe composition and the archive side — and the edge reuses selectively: republishing the dependency moves the dependent while republishing an unused package moves nothing
kind: decision
status: proposed
created: 2026-07-20
updated: 2026-08-14
tags:
  - packages
  - dependencies
  - build
  - actions
  - identity
relations:
  informed_by:
    - research-index-formats
    - research-dependency-resolution
    - research-package-builds
  depends_on:
    - decision-0007-tracked-dynamic-dependencies
    - decision-0009-peer-first-party-domains
    - decision-0015-interface-rule-selection
    - decision-0026-generic-typed-calculus
    - decision-0028-sandboxed-local-executor
    - decision-0031-action-cache-identity
    - decision-0032-action-granularity
    - decision-0033-consumer-of-action-reuse
    - decision-0034-discovered-header-dependencies
    - decision-0035-link-over-a-list-of-objects
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0042-binary-reuse-as-admitted-substitution
    - decision-0044-the-first-source-adapter
    - decision-0045-a-locked-source-becomes-a-built-artifact
  supersedes: []
---

# an index line carries the requirement, and a dependency's artifact is a library the graph builds

> takes the item 0045 recorded as the next round's first: "the next round's first item is the edge, with the index format change that makes it honest." the ground was 0044's — one line per version, no requirements, so an edge would have been fabricated by a fixture — and this round's first act is to change that ground: the line a registry answers with now says what a version depends on, and everything downstream of it is measured against what the line actually carried.

## context

0044 built the registry adapter over an index whose line was `<version> <features> sha256:<digest>`, and 0045 built the package over that line into a running executable. the solver's transitive half has existed since 0040 — `requirements_of` turns a chosen candidate's requirements into constraints, attributed to the choosing candidate — and has never been driven by a real index, because no index line carried a requirement to drive it with. what remained was the format, the artifact a dependency is, the channel a dependent's compile sees a dependency's headers through, and the reuse behaviour that would make the edge a graph edge rather than a resolution artifact.

the systems that shipped this read differently at exactly the format question, and the research note reads them from their own documents. cargo duplicates the manifest's dependency list in the index so that, in RFC 2141's words, resolution can happen without fetching, and writes no authority rule for the disagreement its duplication makes possible. Debian derives the `Packages` index from the built packages and states one sentence of contract — the fields "must exist in the record about the package in the Packages file and the value must match exactly or a client might recognize a metadata mismatch" — with enforcement left to what a client "might" do. Alpine extracts the package's own metadata into one signed index the solver exclusively consults, which makes the disagreement impossible by making the second source unreachable. PyPI's simple API originally carried no metadata at all, forcing a download per candidate, and PEP 658 fixed the cost with per-file metadata sidecars rather than inline requirements, on the recorded ground that a project page carrying every distribution's dependency list "would result in net savings" unclear. these are not one answer wearing four hats; the note is the disagreement.

## decision

### the index line carries requirements, and resolution fetches nothing

the line becomes `<version> <features> sha256:<digest>` followed by zero or more `requires <domain>/<name> <range> [<features>]` clauses. the range is a closed spelling over the constructor set 0040 declared — `*`, `=1.0`, `>=1.0`, `>1.0`, `<=2.0`, `<2.0`, and a comma-joined `>=1.0,<2.0` — so no constructor is unprintable and none is invented. `index_candidate` parses the clauses into the `Requirement` the universe already carries rather than the empty box it hardcoded, a malformed clause is refused with the package and the line named, and the adapter renders the same line it reads (`index_line`), the one-spelling arrangement the lock's binding line has, so a publisher and a reader cannot drift into two formats.

the position is cargo's and alpine's, taken against the note's disagreement rather than averaged over it: the requirement lives where the resolver reads it, so a resolution over any depth of graph is one read of the index and zero fetches. PyPI's cost is the measured alternative — a resolver that must download a distribution per candidate to learn what it requires, the "discarding much downloaded data" PEP 658 exists to stop — and Debian's three-place derivation (control to `DEBIAN/control` to `Packages`) is the complexity of being a cache of a fact the package also carries.

what happens when the index and a fetched description disagree is decided before the surface exists. they do not overlap yet: the index carries what resolution needs — requirements, the digest — and the description carries what the build needs — sources, includes. when descriptions acquire requirement-carrying, whichever side ships second meets Debian's rule, sharpened: the values must agree exactly, and a disagreement is refused at the read with both spellings named, never resolved by which code read last. cargo's silence — a duplication with no authority rule anywhere in its documents — is the failure mode the rule exists to not repeat. the witness has nothing to say about requirements, and that is a position rather than a gap: the log witnesses bindings, coordinates to content, which is the fact a fetch can lie about; a requirement is index data, and the answer to an index that lies about requirements is the one 0044 already gave the universe — a moved line moves the universe digest, the lock's diff names the moved input, and the resolution that follows is the caller's decision.

### a dependency's artifact is a library: objects and the headers it offers

the package build interface of 0045 produces `xylem.Executable`, and a dependency of a C package is not an executable. the artifact is a library — the objects a dependent links and the headers its compiles see — and it is produced by a second declared interface over the same closed procedure: `(Toolchain, Tree, Build) -> Library`, where `Library` is a phloem record over `List<xylem.Object>` and a header set of `(path, content)` pairs. the build declaration grows `includes`, tree paths the package offers as headers, where the include spelling is the tree path itself; the library rule resolves them against the tree exactly as it resolves its sources — an include the tree does not hold is the same refusal a prescribed source meets — and offers them in the artifact beside the objects.

this is the second interface over `List<Object>` plus headers, and the record takes it against the two alternatives the previous round left. an artifact sum — one build interface returning `Executable | Library` — is refused because it widens what every consumer must handle: a caller asking for a program would match on a constructor before it could link. `ar` is refused as the first non-compiler tool this project invokes: an archive is a transport for exactly what values already carry, it would spend the confinement story 0028's third allowlist measurement stands for while performing no computation 0032 would recognize as an action's own, and it collapses the per-object edges that make a dependency's rebuild fine-grained — the 0035 argument against tree-valued inputs, one level up. the measurement waits for a tool that earns it.

### the edge is the graph's: the dependent names dependencies as (tree, build)

the executable interface widens to `(Toolchain, Tree, Build, List<Dependency>) -> Executable`, where a dependency is the pair the caller already holds — the dependency's measured tree and its build declaration — and the frame does the rest in one evaluation: it requests each dependency's library against the library interface (`NeedAll`, so dependencies build independently, 0029), merges their header sets with its own includes, compiles its own sources through xylem's compile entries, and links its own objects followed by the dependencies' objects in declared order, the order a linker resolves in. a build with no dependencies passes through an empty library batch, so one procedure serves every depth. the widening is a revision bump (`phloem-package-build-v2`) that breaks the request format rather than migrating it, on the working rule.

the pair rather than a pre-built library value is the input because the edge belongs to the graph. if the caller built the dependency first and passed the artifact, the two builds would be two roots the engine never relates — caller-ordered, no recorded dependency between them — and the invalidation the round claims would be a caller's discipline. with the pair, the dependent's computation key covers its dependencies' trees and builds, the library builds key and cache under their own interface, and the recorded request edge is what moves when a dependency moves. the resolution is untouched by any of this: it was already one request against the declared interface, the constraint set names the dependent only, and the solver walks the edge through the requirement the index carried — nothing caller-side resolves transitively and nothing in the frame resolves at all.

### the compile entry gains a declared input for provided headers

the sharp question of the round. a dependent's compile must see the dependency's headers, and in 0034's arrangement the only channel is the engine-wide `HeaderUniverse` a caller registers at `register_xylem` time — host configuration, on the terms of toolchain discovery. a dependency's headers are not host configuration; they are measured content derived from a fetched archive, and if the only way a dependent's compile could see them were a registration the fixture assembled, the edge would be fabricated at the engine level whatever the index said — the trap 0045 named in advance.

the answer is the compile entry's third input: `(Toolchain, CSource, Headers) -> Object`, where `Headers` is a `(path, content)` list the request declares beside the source. the entry passes it into the discovery pass, which stages the registered universe plus the provided set, and into the compile action, which resolves each discovered path against the same union — one spelling naming two contents is refused, agreeing duplicates collapse. the package rules are the only writers: a library build provides its own includes, a dependent build provides its own includes merged with its libraries' headers. the registered universe keeps exactly the role 0034 gave it, for the headers a build offers from configuration; provided headers are the ones a build offers because another build produced them, and they arrive as request data on every edge they cross.

this is the first xylem interface change in six records, and it is argued rather than done in passing. the alternative that keeps xylem unchanged is per-engine universe composition: library code, not the fixture, assembles the registration from the dependency trees. it was rejected on two grounds. it leaves the header half of the edge outside every computation key — the staleness hole 0034's own unresolved section names, where a changed universe registration is invisible to a root whose inputs did not move — and it couples the correctness of a two-package build to the state of the engine it runs on rather than to the values the graph carries. the provided input costs one interface widening, three revision-bumped rules (`xylem-v3`), and the honesty that a caller now names, beside a source, any headers that come from elsewhere; it buys a dependency's headers riding the same keyed, recorded, invalidating path as its objects. 0034's position is extended, not amended: discovery still resolves against a declared set, the set is no longer a lie by omission when part of it is a dependency's output, and the fixture's registered universe is empty — the header half of the edge is measured, not assumed. 0009's direction is intact: a provided header is a path and a content identity, and xylem learns nothing about packages; `tests/dependency_direction.rs` stays green over the resolved manifest.

### the lock does not grow a required-by field

a two-package resolution writes two entries, and the entries are pins — coordinates to content — with no field saying who required whom. the requirement's attribution already lives where 0040 put it, in the resolution's derivation, where the solver names the choosing candidate and the unsatisfiable answer reports it; the lock's diff names the moved universe and the drifted entry, which is the input diff an invalidation explanation owes; and a build assembles its dependency list from the resolution it just ran, where each chosen candidate carries its `requires`. the entry is not the place an edge belongs: a pin that recorded its requirer would change when an unrelated package dropped its requirement on the pinned one, and a binding that moves because someone else's declaration changed is not a binding.

## alternatives considered

### requirements in the fetched description

the archive-side position: the manifest rides inside the archive and the index points at it, resolution fetching per candidate.

rejected on PyPI's measured cost and 0044's shape. a resolver that must fetch to learn requirements makes every resolution a download cascade — the exact inefficiency PEP 658's motivation records — and the fetch is a caller-side effect with no place inside the solver 0040 placed in the host-rule tier. the index line is where the solver already reads; the requirement joins the digest there.

### an artifact sum over one build interface

one interface returning `Executable | Library`, the build declaration choosing the constructor.

rejected as the widening it is. every consumer of a package build — a dependent's frame, a caller, a future deployment library — would match on the sum before doing the one thing it wants, and the two artifacts have different consumers by nature: a program is an end, a library is an edge. two interfaces over one declared procedure say that instead of encoding it.

### `ar` as the dependency's artifact

the systems-programming default: the dependency builds into a static archive, the dependent links the archive, and `ar` becomes the first non-compiler tool this project invokes.

rejected on the confinement-and-granularity exchange. an archive adds a tool invocation — an allowlist measurement, a closure declaration, a confinement story — to transport content the engine moves for free as values, and it flattens the per-object edges that make the round's selective-reuse claim work: touching one dependency source would re-serve the dependency's other compiles from the reusable index, which is the incrementality 0031 and 0033 exist for. the third allowlist measurement is not refused; it waits for a source format whose extraction is not a pure parse, per 0045's own ground.

### per-engine universe composition for dependency headers

xylem unchanged; the package library composes the engine's registered `HeaderUniverse` from the dependency trees before registration.

rejected on where it leaves the edge. the composition would be library code — honest in authorship — but its effect would live in engine registration state, invisible to every computation key: the dependent's compile would find the dependency's headers because of how the engine was built, not because any request named them, and the fixture could not distinguish a real edge from a registered one. the provided-headers input costs the interface change and names the fact in the request; that trade is the reason this is the round's one xylem change.

### two caller-driven builds

the dependent's build takes the dependency's library as a pre-built input; the caller builds the dependency first, as 0045 already could with two roots.

rejected on what the edge would then be. caller-side ordering composes nothing in the engine — no recorded dependency between the two builds, no shared invalidation, and the reuse asymmetry would be the caller's discipline rather than the graph's measurement. the dependent's request naming `(tree, build)` pairs is the smallest input that makes the engine own the ordering.

## consequences

xylem's compile entry, discovery action, and compile action gain the provided-headers input, with the rule revision moving to `xylem-v3` and no durable state surviving the bump. phloem's build module gains the library rule, the dependency value, and the widened build interface (`phloem-package-build-v2`); the build declaration grows `includes`; the registry adapter reads and renders requirement clauses; the lock and the substitution machinery are unchanged apart from the widened build request they build. `0009`'s direction holds: phloem names xylem, xylem names no package, and the kernel gains nothing.

the standing artifact-interface question takes its second extent: past an executable, the artifact a library package needs is objects plus offered headers as one value over the 0026 constructors — still no kernel constructor, still a nominal content type produced by a declared interface for the executable half, and a record over them for the library half.

the index-versus-description question from 0045's unresolved section is answered to the extent the round forced: the registry index carries what resolution needs and the description carries what the build needs, with the exact-match rule for the day they overlap.

### measured

the portable half, measured on darwin: the range token round-trips every constructor and refuses the malformed spellings naming the line; an index line with requirements reads through the adapter's own rendering and a malformed clause is refused naming the package and the line; one resolve request over a two-package registry — the constraint set naming the dependent only — selects both packages with the requirement carried on the chosen candidate; an unsatisfiable pair reports a derivation whose emptying constraint is attributed to `candidate pithpkgs/hello 1.0` and to no root constraint; a requirement that moves with nothing else moving — same versions, same digests, a different range — moves the universe digest and the lock's diff names the moved universe with no entry drifting; a registry that gains a package nothing requires moves the universe and no entry; a pinned re-resolution reproduces both selections. in the lock's canonical entry order a shorter name sorts first — `util` before `hello`, the length prefix comparing before the bytes — which the test records because a person reading a two-entry lock file will meet it.

the linux-gated half ran on ubuntu on aarch64 in a local VM, inside this repository's nix devshell, where `cc` is a store-path clang discovery admits: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all --check` all clean on that host, and the five tests in `two_package_build.rs` green. the claims, each with its test: two index lines become one running program with nothing fabricated between — one resolution reading the requirement, both archives fetched, measured, and witnessed, the dependency's library built in-graph, the dependent's compile seeing the dependency's header as request data over an engine whose registered header universe is empty, five actions in the cold build, and the program exiting with what the dependency's code makes it return; the second build `Reused` planning no action and a fresh engine over the same state root `Hydrated` allocating none; republishing only the dependency moves the dependent's artifact — the dependent's own tree and lock entry unchanged, the program's exit code moved by the dependency's code, and exactly three actions running where five did, the dependent's discovery and compile served from the reusable index across the moved edge, which is the fine-grained half of the asymmetry; republishing a package neither uses moves the universe digest and moves nothing else — the same request is `Reused`, the artifact identical; and the dependency's library is one value holding its measured object and its measured header under the include's own spelling. xylem's unit half: a provided header joins the staged set, one naming the registered content collapses, and one spelling naming two contents is refused with the path and both identities named.

## unresolved

a dependency's dependencies have no channel: `Dependency` is the flat pair, and a chain a→b→c forces the middle package's build to name its own dependencies, which the library interface does not accept. the natural shape is a nested dependency list — recursive records need a nominal spelling the calculus has not exercised — and the first chain fixture forces it.

the include spelling is the tree path, and nothing renames or prefixes it: a dependent writes `#include "util-1.0/util.h"` because that is where the provider's tree holds it, and two providers offering the same path with different content are refused rather than namespaced. the `-I`-style mapping is build-declaration design that waits for a package that needs it.

whether descriptions ride inside archives is still open — 0045's question, narrowed by this round to the overlap rule above but not decided — and the first registry that carries descriptions picks the spelling.

features on a requirement are parsed and carried but nothing selects over them end to end; coexistence of several versions of one package in one realization stays with 0040's first-solver handoff, which this round's fixture did not force; cycles between packages are refused by the engine's cycle detection and nothing in the package layer explains them better.

the library interface takes no dependencies of its own, for the same flatness as the first unresolved item, and a library that itself links a library would currently inline those objects at its declaration — honest for the prototype, wrong for provenance, and the chain work above is what fixes it.

## sources consulted

- [cargo registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html), [RFC 2141](https://rust-lang.github.io/rfcs/2141-alternative-registries.html), and [RFC 2789](https://rust-lang.github.io/rfcs/2789-sparse-index.html)
- [Debian repository format](https://wiki.debian.org/DebianRepository/Format) and [Debian policy chapter 5](https://www.debian.org/doc/debian-policy/ch-controlfields.html)
- [Alpine Apk_spec](https://wiki.alpinelinux.org/wiki/Apk_spec) and [apk-package(5)](https://github.com/alpinelinux/apk-tools/blob/master/doc/apk-package.5.scd)
- [PEP 503](https://peps.python.org/pep-0503/), [PEP 658](https://peps.python.org/pep-0658/), [PEP 691](https://peps.python.org/pep-0691/), and [PEP 714](https://peps.python.org/pep-0714/)
- [libsolv](https://github.com/openSUSE/libsolv), the [dart pub solver](https://github.com/dart-lang/pub/blob/master/doc/solver.md), and the [pubgrub-rs guide](https://pubgrub-rs-guide.netlify.app/)
- the index-formats research note carries the full quotations: [docs/research/index-formats.md](../research/index-formats.md)
