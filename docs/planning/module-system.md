---
schema: design-doc/v1
id: planning-module-system
title: the module system
summary: three layers from a bare name to bytes, sources as routes rather than identities, registries bound to domains and never searched, and workspaces as the bootstrap locator
kind: planning
status: draft
created: 2026-08-17
updated: 2026-08-24
tags:
  - planning
  - language
relations:
  informed_by:
    - research-language-frontend
    - research-dependency-resolution
  depends_on:
    - planning-language-frontend
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0046-an-index-line-carries-the-requirement
  supersedes: []
---

# the module system

this is round five of [the language frontend](language-frontend.md), and its subject is modules, imports
and dependencies.

## what each layer answers

| layer | file | question |
|---|---|---|
| 1 | `src/*.pi` | which local binding does this name? |
| 2 | `module.pi` | which subject, which versions are acceptable, and from which source? |
| 3 | `pith.lock` | which version, and which bytes? |

```
module myorg/hello 0.3.1

registry pkgs.pith-lang.org serves pith

use xylem = pith/xylem >=1.4,<2.0
```

```
import xylem
nominal Greeting = Text
pure rule build(tc: xylem.Toolchain, src: xylem.CSource) -> xylem.Executable = host
```

`nominal X = Text` is 0026's own spelling; the `rule` line is `design/rules-and-graph.md:27`'s, treated as
canonical by 0032; `module.pi` is already the fixture name in `pith-diag`'s tests.

`module` and `use` are clauses in the one grammar, admitted only in `module.pi`, exactly as `import`,
`nominal` and `rule` are admitted only in a source file. one lexer, one parser, one refusal table. the
manifest is not a value inhabiting a declared record type and needs no coordinate — which keeps the
acyclicity argument (a manifest cannot import, therefore cannot compute, therefore no module is needed to
read one) while deleting a whole layer of machinery. this is bzlmod's law restated: statically resolvable
imports and self-computed dependency sets come from two layers, never one.

three grammar rules carry the weight. `import` is a clause in a fixed position at the head of a file and
never an expression — Pkl's distinction, and it is what preserves parse-in-isolation. an `import` naming no
`use` binding is a diagnostic at the import token with a cross-file note. the local alias does not survive
elaboration: `x.Toolchain` and `xylem.Toolchain` both become the same coordinate, so an import cannot change
selection. that last one is the only hard constraint the entire precedent survey produced, and it is
checkable: elaborate one file twice, once aliased and once fully qualified, and assert equal body digests.

the import clause carries no version, no range, no digest, no path and no URL, ever. the population today is
one version per domain, so if the clause can carry a version, the day versions arrive every `.pi` file is
a migration. deno's retreat from URL imports and go's `/v2` are the two systems that learned this the
expensive way.

## identity

a module's identity is 0039's two-part shape — a declared domain identity plus a name. the domain segment is
not the registry namespace. `pithpkgs` is where published packages live; baking it into every module's
semantic identity is the collapse 0039 refuses ("the domain's own identifier … is 0005's external identity …
never the package's identity") and the authority 0047 declined to pay for ("pith has no registry, no
publisher, and no released versions").

`Coordinate { module, name }` keeps its shape. the module identity string is not respelled in this round.
respelling it to a slash-separated form would close the dotted-spelling ambiguity 0047 recorded —
`Coordinate::parse` splits on the last dot, so a module identity containing a dot is ambiguous with a
longer name in a dot-free module — but it also moves every declaration digest, every rule revision and
every computation key in the tree, which makes round one's measured claim (the live tables and their `.pi`
counterparts agree digest-for-digest, and xylem's nine rule revisions are bit-identical) unrunnable. the
cheap half is taken now: `DeclarationTable::declare` refuses a declaration name containing a dot, which
makes parse and spelling total inverses for every name the corpus contains. the module grammar goes to the
module-system round, where 0048's discard-and-rebuild prices the key movement at zero.

who may declare a domain identity is the module-system round's central question and is not answered here.
what makes it safe to leave open is the layering above: because a source file carries a bare name and
nothing else, no `.pi` file is a migration when the answer lands — only manifests change. that is the
property deno lost by putting the locator in source text, and it covers the namespace question as well as
the version one.

## a source is a route, not an identity

a dependency names a subject and, optionally, where to get it. four source kinds, one shape:

```
module myorg/project 0.1.0

registry pkgs.pith-lang.org     serves pith
registry packages.acme.internal serves acme, acme-labs

use phloem = pith/phloem >=0.1
use gnu    = pith/gnu-build >=0.1
use fancy  = acme/fancy-pith >=0.3  from github:acme/fancy-pith tag v0.3.2
use vendor = acme/other >=1.0       from registry packages.acme.internal
use shared = myorg/shared           from path ../shared
```

absent a `from` clause the subject resolves through the registry that serves its domain. `from registry`
names one directly, `from git` and its forge shorthands say *this url is the module*, and `from path` says
*this directory is*. the distinction that matters is index-versus-module: a registry is an index of many
subjects, a git url is one subject.

0039 makes this cheap rather than combinatorial: an origin "is 0005's external identity: recorded in
provenance as where the package is known, never the package's identity." three consequences follow.
swapping a source changes nothing downstream — identity is `(domain, name)`, content identity is the
measured tree, so two forges serving the same bytes leave every computation key beneath byte-identical and
reuse holds across the swap, with the lock proving whether the bytes match. two sources for one subject is a
refusal, and `Lock::new` already implements it, naming the subject, the domain and both versions. the lock
does not fork per source kind: every entry is subject, version, measured digest, and the origin as
provenance, which is 0041 and 0044 exactly.

forge shorthands are configured sugar, not a source kind. `github:acme/fancy-pith` desugars to
`git https://github.com/acme/fancy-pith`, nothing more, and the host comes from configuration rather than
from the compiler. nix's `github:` is the wart to avoid: it is not sugar for `git+https://github.com/…`
because it uses github's tarball api, so the two forms fetch and lock differently. making the desugaring
total means adding `codeberg:` is not a language change and a self-hosted forge is never second-class,
because it writes the general form.

each source kind is an adapter that must measure content and produce provenance, and that bill belongs in
the record per adapter rather than waved at as "support all of them". 0044 built one — the archive fetch —
plus git tree materialization, and the network case is still "no executor admits it at all", so every fetch
is a caller-side effect.

a path dependency escapes the witness, and that must be visible rather than silent: there is nothing to
witness, since the bytes are in the caller's own tree under their own version control, so the lock entry
says *path, not witnessed*, the way cargo's lock carries no checksum for a path dependency. a path source
inside a published module is refused outright.

## registries are bound to domains, and never searched

a registry declares the domains it serves. a subject's source is then computed from its domain in one
lookup, so there is never a set of candidates and never anything choosing among them.

this is not a convenience. a search order across two registries is selection by configuration order, which
K-7 forbids ("load and registration order cannot select behavior"), which `principles.md` rejects as
conflict resolution by import order, and which 0015 refuses in its own domain by declining to rank. there is
no precedence between registries, because precedence is a ranking. npm's per-scope registry mapping is the
same mechanism and was shipped as the fix for the failure mode below.

specificity is not precedence, and the record must say so, because the two look alike and only one is
permitted. a `from` clause is a unique binding for one named subject; a registry declaration is a unique
binding for a domain. at no point do two candidates for one subject coexist. that is 0015's sanctioned
disambiguation — "the caller being more specific" — rather than a tiebreak.

three refusals, and the first two carry the design:

a domain claimed by two registries is refused at configuration load, before any network access, naming both
declarations and their spans. a domain no registry serves is refused at resolve, naming the domain and
listing what is configured. there is no fallback to a default registry, because that silent fallback is the
attack. a subject bound by both a domain registry and a `from` clause is not an error: the `from` clause is
that subject's unique binding, and the record should say so plainly so nobody reads it as an override
ladder.

what this closes is dependency confusion: with two searched indexes, anyone who can publish `acme/anything`
to the public one shadows a private module. birsan's 2021 work landed that inside apple, microsoft and
paypal, pip's `--extra-index-url` is the canonical footgun, and binding domains to registries makes it
unreachable rather than mitigated.

two smaller consequences. mirrors are transport, not a second registry — configured on the registry, and
since the lock witnesses bytes, a mirror serving different content fails the witness rather than winning a
race. and resolution is one lookup per subject rather than one per configured registry, though that barely
matters for the editor, which never consults a registry at all.

the one ordering in this design, named rather than smuggled. project configuration is authoritative for
domain bindings; user configuration supplies bindings only for domains the project does not mention; a
collision warns, names both, and states that the project won. the effective binding is recorded in the lock,
so a machine with different user configuration produces a lock diff rather than a silent difference. that is
a scoping rule rather than a candidate ranking — at no point are two candidates for one subject compared —
but it is close enough to what 0015 forbids that it must be argued in the record rather than assumed, and a
frozen mode that refuses to resolve anything the project has not bound is the honest escape for anyone who
wants none of it.

the price is that there is no zero-configuration public fallback. cargo, npm and pip all default to their
public index and all three have the confusion problem as a result. pith's shipped default configuration
file can bind `pith`, which gives the same first-run experience without compiling a host into the tool —
and deleting that line is then a supported configuration rather than a fight with the toolchain.

## a workspace is the bootstrap locator

```
module myorg/monorepo 0.0.0

workspace {
  members: ["modules/core", "modules/cli", "modules/shared"],
}
```

members depend on one another by path and carry no version constraint, because they move together, and the
whole tree resolves into one lock.

pith's own tree is the first monorepo, which makes this load-bearing rather than a convenience: xylem,
phloem, stele and example-domain have to resolve before any registry exists. it also replaces something this
proposal had wrong. deriving first-party module locations from a directory convention, the `crates/*` minus
`pith-*` shape, while third parties write a manifest, is two mechanisms with the shorter one reserved for
first-party code. that is 0004's argument against "external plugins with a smaller API" applied to
resolution, and 0056's derivation is how that record's own test computes a domain set rather than a blessed
resolution mechanism. a workspace manifest is one locator: the pith tree writes one, a third party's
monorepo writes the same one, and nothing in the resolver knows which is which.

## resolution, and what the compiler links

the frontend is a peer domain declaring its own constraint model and its own solver body under
[0040](../decisions/0040-declared-constraints-and-resolution.md)'s protocol. it shares the range algebra
with phloem and shares nothing else.

that split is 0040's own seam and the proposal must not cross it. 0040's holding is a section heading:
"constraint models are domain-declared; the shared thing is a protocol, not a representation" — "the
package domain declares constraints over package coordinates, a placement domain declares constraints over
machines, and neither translates into the other's vocabulary." and 0040 licenses the one part that is
shared, explicitly: "the range algebra is shared because it depends only on the ordering, not on the
spelling."

the temptation was to extract phloem's whole resolution half — constraint model, resolve interface, lock
document, registry index — into a crate below both, on one-mechanism grounds. it is the wrong reading of
one-mechanism twice over. it needs an amendment to 0040 that nobody argued. and it recreates the privilege
it was extracted to remove: once `import` semantics are defined by a first-party resolution library the
compiler links, replacing the resolution model is a compiler patch, against `design/overview.md:44`'s
"an external library can replace one, extend one, or define a different domain without a compiler patch".

what the compiler links is the protocol — 0040's four inputs and two outputs — plus a range constructor
set, and nothing that names package coordinates. the extraction the shared-layer reading was reaching for
is smaller than it looks and still worth doing on its own terms: `parse_binding_line` has two consumers,
`lock/text.rs` serves the lock, the index and the environment file, and the atomic-publication discipline
is now implemented three times independently. that is a refactor record, not a foundation for this one.

what the compiler must not link is phloem, and the reason is concrete rather than theoretical: phloem
depends on xylem, so a compiler containing phloem contains the C-build domain library, and `import xylem`
would typecheck partly because the compiler already contains xylem. that breaks 0004 and U-10 at the module
layer, and the obvious peerhood test would not catch it, because that test publishes `example-domain`,
which is not in the closure, while xylem is.

three pins, so that two developers on one lock get the same completions: the version scheme is
`numeric-segments`, the preference list is newest-first, and the budget is a constant. pinning the
preference list also makes phloem's `PreferenceList::compare` defect unreachable — it calls
`scheme.compare(left, right)` identically on every iteration, so a preference list is effectively its first
element and 0040's "fewest changes against the previous lock" ordering cannot be expressed in that
structure at all. that defect needs its own fix and is not this proposal's.

the module domain's candidate type carries the ABI digest, and it carries it inside the candidate, so it
participates in the universe digest. keeping it out in a driver-side map — which is what a shared index
line would have forced, to avoid moving phloem's resolve interface — defeats 0046's own guarantee: "a moved
line moves the universe digest, the lock's diff names the moved input." with the digest outside the
universe, a registry can change a module's published interface and no lock header field moves, so 0041's
staleness mechanism has nothing to report. because the module domain has its own constraint model, there is
no shared interface to move and the cost is zero rather than merely pre-release-free.

the universe is scoped to the requirement closure of the root subjects, not the whole index. 0046 measured
that an unrelated new package moves a whole-index universe digest, and a module registry will be larger and
busier than a package registry. for an editor re-resolving continuously, an unscoped universe makes the
staleness answer "the universe moved" for nearly every question — true and useless.

the locator is the workspace manifest above, for first-party and third-party modules alike. the peerhood
test needs extending to read non-`.rs` files at the workspace root, because as built it collects only `*.rs`
and `Cargo.toml` and would therefore not catch a root-level file naming first-party domains — the hole and
the fix are the same shape.

whether a module registry needs an index at all is a fork this round has to state rather than assume. an
index exists for exactly one reason, which is 0046's: a line carries `requires`, so a resolver walks the
requirement graph before fetching anything. without it, resolution means fetching every candidate version of
every subject to read its dependencies. the alternative is the flake shape — a git revision is the module
set, so there are no ranges, no solver and no index, because there is no resolution — and it is entirely
coherent, at the cost of one version per subject per revision and all-or-nothing upgrades. 0040, 0041 and
0046 together already chose the index for packages. the module half does not have to answer the same way,
since the frontend declares its own constraint model. the no-resolution model is the alternative this round
argues against rather than a decision already taken. the git-hosted index is the middle position and is
where this proposal sits: bazel's central registry is a git repository of manifests and works at that
scale, and crates.io moved its index off git onto sparse http once the clone-and-update cost grew with the
index, so git is the right first host and the http move is a swap behind 0044's source adapter rather than a
redesign.

## where libraries live

a registry root holds an index per subject and the surface bytes under digest paths. content of record is
`pith-store` blobs and trees — digest paths with no extension, per `name.md`. dependency sources materialize
into `/pith/var/cache`, which `name.md` already designates as the local cache, and only when spans into a
dependency are actually needed, because `Span` carries no file identity. `/pith/store/<digest>-<name>` is
for installable artifacts and is not where modules live.

the workspace's own modules are not a generated registry under `target/`: that adds a fifth location
`name.md` does not have, ties `.pi` resolution to a rust build directory, and makes `cargo clean` a
module-resolution event.

`pith.lock` is claimed twice today — `name.md` calls it the ecosystem lock file, and
[0043](../decisions/0043-the-development-environment.md) derives it as phloem's default-environment lock,
which cannot be merged with another. the resolution favors `name.md`: modules resolve into `pith.lock`, and
0043 is amended in one sentence so every environment lock including the default is `<name>.pith.lock`. that
is 0043's own one-lock-per-resolution rule applied honestly, and pre-release the rename is free.

## compatibility, computed rather than asserted

because ranges are kept, the enforcement problem bazel tried and deleted is inherited. `compatibility_level`
was adopted as MVS's safety mechanism and killed because "people hate it when compatibility levels aren't
bumped" and equally hate the ecosystem sweep when they are; the replacement is prose.

elm's answer is the only credible one and pith is in a better position to take it than elm was. `pith diff`
over two module surfaces is a total function, because both are canonical values: an added declaration or
rule is minor; a removed declaration, a moved nominal representation, a changed constructor set or a changed
interface is major; doc, order, formatting or a revision bump is patch. the registry adapter gates
publication on it. elm computes this from published documentation; pith's inputs are already canonically
encoded, so the differ is exact rather than approximate.

single version per subject is not policy here, it is a type-system requirement. two versions declaring
`xylem.Object` are two declarations at one coordinate with two digests, and `inhabits` compares coordinate
spelling and representation, so a value from one fails `is_type` against the other. phloem's existing
double-binding refusal, which names the subject and both versions, is the enforcement the type system needs
and it is already built. coexistence is foreclosed at the kernel level; reopening it means `Coordinate`
growing a version qualifier.
