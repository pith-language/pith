---
schema: design-doc/v1
id: decision-0045-a-locked-source-becomes-a-built-artifact
title: a locked source becomes a built artifact through a closed build procedure, and a realization is the attempt the engine already has
summary: the fetched archive unpacks as a parse, not a tool — a tar is a deterministic encoding and the adapter reads it caller-side beside the fetch; a package's build is data from a closed procedure the library owns, the stdenv and Guix position, because a fetched source cannot carry host code and the script-per-package position needs 0038's represented bodies; the procedure runs as one pure rule requesting xylem's compile and link entries, so phloem plans no action and a realization reuses on the engine's machinery alone, which is the measurement 0039 was owed; a realization is nothing phloem constructs — it is the attempt the engine already holds — and the enum that said how a binding was served is renamed to `Serving` for saying that; one dependency edge is deferred to the first index format that carries requirements
kind: decision
status: proposed
created: 2026-07-15
updated: 2026-07-27
tags:
  - packages
  - build
  - actions
  - identity
relations:
  informed_by:
    - research-package-builds
    - research-artifacts-and-trust
    - research-nix
    - research-build-systems
  depends_on:
    - decision-0003-explicit-effects
    - decision-0009-peer-first-party-domains
    - decision-0014-reproducibility-properties
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0026-generic-typed-calculus
    - decision-0028-sandboxed-local-executor
    - decision-0029-declared-independence
    - decision-0030-toolchain-closure-as-declared-input
    - decision-0031-action-cache-identity
    - decision-0032-action-granularity
    - decision-0033-consumer-of-action-reuse
    - decision-0035-link-over-a-list-of-objects
    - decision-0036-produced-program-as-content
    - decision-0038-represented-rule-bodies
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0042-binary-reuse-as-admitted-substitution
    - decision-0043-the-development-environment
    - decision-0044-the-first-source-adapter
  supersedes: []
---

# a locked source becomes a built artifact through a closed build procedure, and a realization is the attempt the engine already has

> takes the one item M-4's statement still owed after 0044 closed the rest: "no package was built end-to-end from a lock entry into an artifact through xylem — the substitution machinery measures offers, but nothing turns a locked source into built output." the claim under test is 0039's, argued in three records and unit-measured in none: "a realization is identified by the computation and content identity the kernel already has, so the package library adds no identity machinery and stays a peer consumer of the build library." 0042 and 0043 already spend it — an admitted binary substitutes for a realization that was never produced, and an environment names realization coordinates for realizations that did not exist. this round either makes the claim a measurement or finds where it breaks.

## context

the seam the round opens on is `request.rs`: `Description::inputs` was a list of content identities and `compile_requests` mapped each to one xylem compile — no link step, no options, no procedure, and the file that produced it was a stand-in its own doc comment admitted no real package survives. the sources were also stand-ins: bytes a test fabricated, until 0044's adapter read a registry, fetched an archive, and verified the binding against a log. what exists now is a lock entry bound to measured archive content and no machinery between that archive and a running executable.

the systems that shipped this move disagree about the machinery, and the disagreement is on one axis. nix's stdenv owns a fixed procedure — the phases in `setup.sh`, `genericBuild` looping over them, a derivation overriding a phase by shadowing a variable — and a vanilla package writes no build code at all. Guix's build systems are procedures whose phase lists are first-class data a package rewrites with `modify-phases`, possible because scheme is homoiconic. Spack's package.py inherits phases from a build-system class; Debian's `debian/rules` "must be an executable makefile" the packager writes, with debhelper's `dh` sequences layered on top as an inheritance convention; portage's ebuild is bash whose phase functions carry `default_` inherited bodies; cargo owns the compile pipeline and lets the package ship one script, `build.rs`, whose effects reach the build through a stdout data channel so the system can keep reading them; homebrew's formula writes the whole `install` method and inherits only the environment. every one of them splits the same way: the declaration is data and the procedure is code, and who owns the code decides whether the system can inspect a build before running it. that is 0038's axis — a host rule is `debian/rules`, a represented rule is the gexp — and a package build has to take a position on it now, because a locked source is about to become an executable.

## decision

### a realization is a reading, not a value

0039 said a realization "is identified by the computation and content identity the kernel already has," and the honest reading of that sentence is that there is nothing to construct. a realization is the evaluation the engine already returned: the artifact's content identity, which is the nominal value the link completed with, and the attempt that produced it, which is the engine's computation under its own key. the package library constructs neither, carries neither, and adds nothing to either — the description's build and the tree are request inputs, the computation key covers them, and the artifact is store content.

the name was already taken by the wrong concept. `substitution.rs` had a `Realization` enum whose variants are `Substituted` and `Built`, which is how-it-was-obtained, not what-it-is: a served binary and a run build are two ways a *binding* was served, and the thing they stand in place of is a realization neither of them names. the name is corrected here — the enum is `Serving`, the function is `serve` — and the concept is right to have no phloem-side constructor. a test that wants to hold a realization holds the evaluation.

### the unpack is a parse, and it sits beside the fetch

an archive's bytes become buildable content in two halves. the pure half: a tar is a deterministic encoding, and reading it is a total function from bytes to files — the ustar parser in `archive.rs`, which refuses what it cannot interpret (a bad magic, a failed checksum, an entry type that is not a regular file or a directory, a path that climbs out of the tree, a repeated path) rather than accepting bytes silently. the caller-side half: importing each measured file into the engine's content store, in the position 0044 put the fetch — `unpack(&mut engine, &fetched.bytes)`, producing the tree as a value, a canonically sorted list of paths and content identities.

no tool is invoked, so 0032's action question does not arise and 0028's allowlist is not widened here — not because a third measurement was inconvenient but because there is nothing to measure: the alternative shape, an action running `tar`, would invoke a tool to perform a computation. the honest boundary is the engine's: a pure rule cannot publish new store content, since imports are what action capture does, so the import belongs to the caller. when the fetch moves into the graph as the fixed-output action 0044 named, the unpack moves with it — an action's captured outputs already enter the store as content — and the parse is unchanged by where its bytes came from.

### a package's build is data from a closed procedure

what a package declares as its build: `PackageBuild`, a record over the sources that compile, named as paths into the unpacked tree, in link order. the procedure around them — compile each through xylem's entry, link the objects through xylem's link entry into one executable — is the library's, fixed and inspectable, the stdenv and Guix position. the description carries it beside the source binding: `{name, source, build}`, and the old `inputs`/`options` lists are gone, on the working rule that formats break rather than migrate.

the position is taken against the research, not averaged over it. the declaration-as-data half is the field's near-universal compromise, and the procedure-as-code half is exactly what a fetched source cannot pay: downloaded bytes cannot carry host code this engine could run, and the systems that accept packager code at this boundary — debian's makefile, homebrew's `install`, cargo's `build.rs` — accept a procedure the system can only execute. the script-per-package position is not refused forever; it is deferred to 0038's represented rule bodies, which is where it becomes inspectable, and a closed constructor set is what the interval before that buys. the set grows by amendment — options, defines, non-executable artifacts — and each growth is a revision bump that invalidates every cached package build, which is the conservative direction.

this answers the standing question "what is the minimal artifact interface needed by both build and deployment libraries" to its first extent: a nominal content identity produced by a declared interface. the package build's interface is `(Toolchain, Tree, Build) -> xylem.Executable` — the same nominal content type a package-less build links, over the same request-input half of realization identity — and deployment will consume the same spelling. nothing else crosses the boundary, and the question stays open only for what a deployment later finds it needs beyond an executable.

### the procedure runs as one pure rule, over xylem's entries

the closed procedure is one pure rule in the graph, `PackageBuildRule`, and it composes the way a peer composes: its body requests xylem's declared interfaces and plans no action itself. the compiles go out as a `NeedAll` — the sources of one package compile independently, and saying so is the body's declaration (0029) — each a request against the compile entry, discovery included; the link goes out as a `Need` against the link entry; the executable completes. every action in a package build is planned and confined by xylem's rules under xylem's revisions, and phloem names xylem while xylem learns nothing about packages, which `tests/dependency_direction.rs` still asserts over the resolved manifest.

the rule is host-rule tier (0038): its body is rust, its revision is a manifest digest, and its honesty — that the requests it derives are a function of the tree, the build, and the toolchain value — is the same convention xylem's planners carry. the represented tier is where that becomes structural, and this rule is one more body ready to migrate.

### a realization reuses on the engine's machinery alone

0039's sentence is now a measurement rather than an argument. the package build is one request; its computation key covers the rule, the toolchain, the tree, and the build declaration; the actions beneath it key and admit on 0031, and the consumer-of-action walk revalidates on 0033. the second build of the same lock over the same tree reports `Reused` at the root and plans no action; a fresh engine over the same state root after the first is dropped reports `Hydrated` and allocates no action beneath the root. phloem adds no reuse machinery, no index, and no key — the fixture asserts the outcomes the engine itself reports, which is the whole content of the claim.

a republished archive exercises the invalidation half: new bytes are a new tree value, a new computation key, a rebuild. the lock's diff names the moved universe and the drifted entry, and the artifact's content identity moves with the source that produced it.

### what 0042 and 0043 gain

0042's central claim — a served substitution publishes no engine attempt where a refused one's build publishes exactly that attempt — was measured against a fabricated build. it is now measured against a real one: over a state root where only the substitution served, the build's own request computes, because no attempt exists under its key; over the root where a refusal left the build running, the second run of the same request is `Reused` off the attempt the build published. the toolchain in the admission leg and in the build's request are one value, which is what ties the leg to the build it guards.

0043's environment names realization coordinates for realizations that now exist: the document over the built lock carries the platform and toolchain the build ran under, and the artifact those coordinates realize is content in the store. the document records no realization of its own, which is the position 0042 fixed — the lock binds source only, substitution records witness substitutions, and a built realization's witness is the engine's attempt, which needs no second spelling in a document.

### one dependency edge, deferred

a package that depends on another package would force transitive resolution over fetched sources and a build ordering through the graph, and this record defers it deliberately. the ground is 0044's: the local index carries no dependency edges — one line per version, no requirements — so an edge here would be fabricated by a fixture rather than read from a source, and the first index format that carries requirements is what measures whether the candidate record the universe spells is the one a real registry answers with. the ordering half is not the blocker — the graph orders compiles and a link already, and two builds in one engine compose — the resolution half is. the decision is recorded here rather than left to scope: the next round's first item is the edge, with the index format change that makes it honest.

## alternatives considered

### the unpack as a tar action

one action invoking `/bin/tar`, the first tool this project invokes that is not a compiler, with the third syscall-allowlist measurement 0028 predicted.

rejected on what the work would be, not on its cost. a tar archive is a deterministic encoding and unpacking it invokes no tool; an action running `tar` would spend a confinement story, a closure declaration, and an allowlist measurement to perform a computation, which is 0019's progression run the wrong way — the contract of a pure parse is total and the code can state it. the measurement is not refused: when a source format genuinely needs a tool, or when the fetch moves in-graph and its interior wants one, the third measurement happens then, against a workload that exists.

### the build as a prescribed list of requests

the stand-in this record replaces: the description carries inputs, the library maps each to a compile, and the caller drives the requests one at a time.

rejected on three grounds. it has no procedure — no link, no ordering, no artifact — so "what a package's build is" is unanswered and every caller re-answers it. it has no graph shape — a caller-side sequence of requests composes nothing, and the whole build is never one keyed computation, so 0033's consumer reuse cannot reach it and the realization is a scatter of attempts rather than one. and it puts the package library in the business of issuing compile requests directly, which is closer to wrapping the build library than consuming it.

### the build as a rule the package registers

the spack position: each package ships a rule — host code — that drives its own build, registered on the engine beside the library's.

rejected on the fetched-source ground. a registry archive cannot carry host rules, so the code would live beside the registry in some second channel, and the lock would bind bytes while the build ran code the binding never saw. it is also 0009's violation one step removed: a package that registers rules is a package that owns build semantics, which is the parent abstraction the peer rule exists to prevent. the door stays open through 0038, where a represented body is data a source could carry.

### the build as 0038's data now

declare the procedure in a represented-rule ir today, the full guix position.

rejected on the gate 0038 set for itself: the calculus the ir types over is not built, and the ir is behind that gate by its own record. a closed constructor set over the landed 0026 constructors — records, lists, texts — is the honest slice: the same declaration elaborates into a represented body when one exists, and the interim is inspectable data rather than a script.

### a realization as a phloem value

give the package library a realization type — the artifact identity plus the computation identity, constructed on build — so callers pass realizations around as first-class things.

rejected as the thing 0039 said the package library would not do. the value would duplicate the evaluation the engine already returns, add an identity the key already covers, and give the substitution machinery a second thing to stand in place of. where a caller needs the reading, it reads the evaluation; where the record needs the concept, it says what it is.

## consequences

phloem gains two modules and loses one. `archive.rs` is the unpack's pure half, a ustar reader that refuses what it cannot interpret. `build.rs` is the tree, the build declaration, the caller-side unpack, and the `PackageBuildRule` with its interface and request constructor. `request.rs` is deleted — the stand-in this record exists to replace. `description.rs` carries `{name, source, build}` with a new digest domain, `substitution.rs`'s `Realization`/`realize`/`realization_requests` become `Serving`/`serve`/`serving_request`, and the description, package-identity, binary-reuse, and substitution fixtures move to the new shape.

xylem is unchanged, the kernel is unchanged, and no constructor lands: the sixth record in the package line to need none. `tests/dependency_direction.rs` stays green over the resolved manifest, which is the direction 0009 makes a test rather than a hope.

the standing open question "what is the minimal artifact interface needed by both build and deployment libraries" takes its first answer above — a nominal content identity produced by a declared interface — and stays open at exactly the point where deployment speaks.

### measured

the portable half is measured everywhere: the archive parser's acceptances and refusals (magic, checksum, truncation, refused entry types, paths that climb, repeated paths), the tree and build round-trips, the source resolution in link order with a missing path named, and the request's shape against the declared interface. the binary-reuse fixture now drives the package build as one request over the package key, so 0042's two-paths test keys on the build the description actually prescribes.

the end-to-end and every toolchain claim is gated `#![cfg(target_os = "linux")]`, and the host this round was written on is darwin: the gate ran none of them and the local green proves nothing about them. the tests are these — an index line becomes a running executable with nothing fabricated in between, the artifact is a nominal executable over store content and the program runs; the second build `Reused` planning no action and a fresh engine `Hydrated` allocating none; the republished registry moving the universe, the entry, and the artifact with the diff naming the moved inputs; the served substitution leaving no attempt under the build's key where the refused offer's build leaves exactly that attempt; the environment document naming coordinates under which a realization exists.

the correction this section owes: the sentence that first ended that list said these were measured "on the runner that executes it," and nothing had been. the gated file had never compiled on any host — one expression iterated `&u8` into `u64::from`, one import sat unused under `-D warnings` — and beneath the compile errors its fixture summed the ustar header with the checksum field still zero where ustar, and `archive.rs` on the reading side, treat those eight bytes as spaces: a difference of 256 the parser would have answered by refusing the fixture's own archive. the same round's tightening had also put `panic!` into helpers outside `#[test]` bodies — in this file, in `base_case.rs`, and in xylem's `two_source_build.rs` — where the crate's lint posture refuses it (`clippy.toml` exempts unwrap and expect inside tests and nothing else), so the gated files failed clippy on linux even past compiling. none of it was visible on the darwin host, whose `cargo clippy --workspace --all-targets` stays green while gated files hold errors; that is how the round shipped over a red ci run. after repair — the checksum authored the canonical way, the field spaced before the sum and overwritten after it, the helpers carrying failure as results the test bodies unwrap — the six ran green on linux: ubuntu on aarch64 in a local VM standing in for the runner, inside this repository's nix devshell, where `cc` is a store-path clang discovery admits, with `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all --check` clean on that host. the same VM offering an apt gcc outside the store failed discovery and the sentinel refused to read the run as green — the skip discipline working, and the narrower fact under it: these fixtures presuppose the devshell's compiler, not merely linux's.

on whether a non-compiling gated file passing green on the authoring host is worth a guard: no guard is added, because the guard that exists — ci on ubuntu — caught every one of these the moment they landed, and what failed was not detection but this section, which recorded a measurement over a red run. a darwin-side guard could not run what it guards, could not even type-check it without a cross C toolchain (`libsqlite3-sys` compiles C for the target), and would buy a subset of what the red run already said; per-test `cfg` in place of the file gate cannot help either, because `pith-executor-local` is an empty crate off linux and the gated files' imports cannot resolve on darwin at any granularity. the discipline the failure argues for is the one this correction applies: a measured claim names the host it ran on and the commands that passed, or it says it has not run.

## unresolved

the build declaration carries no options. defines, flags, and per-source arguments are where the closed constructor set grows next, and the growth is a revision bump that invalidates every cached package build.

the artifact is an executable. a library package — objects and headers rather than one binary — needs either a second interface over `List<Object>` or an artifact sum, and the first library package measures which.

where a description is authored and how it travels is undecided: the fixture authors it beside the registry, cargo's precedent duplicates key fields in the index while the manifest rides inside the archive, and the first registry that carries descriptions picks the spelling.

the dependency edge is deferred to the next round, with the index-format change that makes it measurable, per the decision section.

the environment document does not record built realizations, per 0042's ground; if a caller ever needs a cross-process witness of what it built, the engine's durable attempt is that witness, and nothing here duplicates it.

## sources consulted

- [nixpkgs stdenv setup.sh](https://raw.githubusercontent.com/NixOS/nixpkgs/master/pkgs/stdenv/generic/setup.sh) and [the stdenv chapter](https://nixos.org/manual/nixpkgs/unstable/#sec-stdenv)
- [Spack packaging guide](https://spack.readthedocs.io/en/latest/packaging_guide.html) and [Spack's builder](https://github.com/spack/spack/blob/develop/lib/spack/spack/builder.py)
- [Guix build systems](https://guix.gnu.org/manual/en/html_node/Build-Systems.html), [build phases](https://guix.gnu.org/manual/en/html_node/Build-Phases.html), and [g-expressions](https://guix.gnu.org/manual/en/html_node/G_002dExpressions.html)
- [Debian policy chapter 5](https://www.debian.org/doc/debian-policy/ch-source.html), [dh(1)](https://manpages.debian.org/testing/debhelper/dh.1.en.html), and [dpkg-buildpackage(1)](https://manpages.debian.org/testing/dpkg-dev/dpkg-buildpackage.1.en.html)
- [cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html) and [package layout](https://doc.rust-lang.org/cargo/guide/project-layout.html)
- [gentoo development guide: ebuild functions](https://devmanual.gentoo.org/ebuild-writing/functions/) and [eclasses](https://devmanual.gentoo.org/ebuild-writing/using-eclasses/index.html)
- [homebrew formula cookbook](https://docs.brew.sh/Formula-Cookbook) and [formula API](https://docs.brew.sh/rubydoc/Formula.html)
