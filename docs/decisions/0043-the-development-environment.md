---
schema: design-doc/v1
id: decision-0043-the-development-environment
title: the development environment is a value over the lock, and materializing and entering it are caller effects
summary: an environment is one resolution plus the realization coordinates it declares — the lock 0041 fixed, held unchanged, beside the platform, the toolchain, and the substitution records 0042 returned to the caller; "reproducible" asserts the determinism of the selection and the rendering, never bit-for-bit reproducibility of the packages; computing the environment is pure, writing its artifacts and entering it are caller effects; one lock per resolution, named for the environment that holds it; confinement is claimed nowhere, because a development shell is not a sandboxed action
kind: decision
status: proposed
created: 2026-07-08
updated: 2026-08-14
tags:
  - packages
  - environments
  - effects
  - provenance
relations:
  informed_by:
    - research-dev-environments
  depends_on:
    - decision-0003-explicit-effects
    - decision-0014-reproducibility-properties
    - decision-0020-nix-as-adapter-not-substrate
    - decision-0024-persistent-engine-state
    - decision-0026-generic-typed-calculus
    - decision-0027-retention-and-gc
    - decision-0028-sandboxed-local-executor
    - decision-0030-toolchain-closure-as-declared-input
    - decision-0032-action-granularity
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0042-binary-reuse-as-admitted-substitution
  supersedes: []
---

# the development environment is a value over the lock, and materializing and entering it are caller effects

> closes M-4, and takes the three handoffs it owes: 0041's "the file's placement and naming in a project, and whether one project holds one lock or several when it runs several resolutions"; 0042's "whether they enter an engine store, a caller-side file, or the development environment's own artifact"; and 0032's seam between a repository that resolves through phloem and one that resolves through a foreign toolchain inside an `Opaque`.

## context

M-4 asked for "a reproducible development environment", and nothing in the notebook designs one. the three records before this built the parts: 0039 fixed what a package and a realization are, 0040 fixed resolution as a pure function of four recorded inputs, 0041 fixed the lock that records a selection, and 0042 fixed the substitution that serves a realization. an environment is the thing all four were for — the place a project says which packages it develops against and under which toolchain — and it is also the place where the honest answers are hardest, because the word "reproducible" and the word "shell" each carry promises the machinery cannot back.

the precedents disagree about what the thing even is. Nix's `mkShell`, read from its source, is a `stdenv.mkDerivation` wrapper "only meant to be consumed by the nix-shell" whose single phase writes an `$out` carrying a warning that its existence is not guaranteed — a derivation that exists to have an environment, not an output — and `nix develop` obtains that environment by building a modified derivation that records the environment and exits. Spack's environment is the opposite position in one respect: it owns a lock (`spack.lock`) beside its manifest (`spack.yaml`), with the guarantee scoped honestly to "the same or a compatible machine". conda's `environment.yml` spent years as a manifest whose resolution lived only on the machine that resolved it, and exact recreation arrived as a separate lockfile format only in conda 26.5. PEP 405's venv is state rather than declaration: a directory whose contents record nothing about how they came to be. rustup's `rust-toolchain.toml`, asdf's `.tool-versions`, and mise's `mise.toml` pin tool versions and resolve nothing else; devcontainers standardize image-plus-metadata with no package resolution at all; devenv, devbox, and Flox compose manifests, locks, processes, and publishing into three different claims about what a dev environment adds over a shell. and `nix print-dev-env` states the field's cleanest effect-boundary position: evaluate the environment to data — shell code, or json "for consumption by another program" — and let the consumer apply it.

on isolation the field is equally divided and equally quiet. no surveyed system confines an interactive environment by default. guix offers `--container` — network cut to loopback, filesystem cut to the working directory, dummy home and passwd entry — as an explicit option with its own costs, and devcontainers confine by construction because an image is their unit. an augmented shell, which is what mkShell, direnv, rustup, and mise produce, makes no isolation claim at all, and mostly does not say so.

## decision

### what an environment is

a development environment is a value: the environment document, composed over the lock 0041 fixed. it holds exactly one resolution's lock, unchanged — same document, same digest, same file — plus the realization coordinates the environment declares, and the substitution records the environment served. its identity is a digest over its canonical encoding, domain-separated the way every phloem digest is.

the composition answers the three-way question the research surfaces rather than averaging it. the environment is a lock *consumer*, not a lock of its own: 0041's lock already binds selections, and forking a second lock shape for environments would fork its merge story too. and it is not a materialized directory: a directory is state whose identity is the machine it sits on, exactly the venv position, and nothing about it can be checked against what was declared. materialization survives as a projection — a deterministic rendering of the document, on the same terms as the lock's file — and entering does not exist in M-4 at all.

a project declares an environment: a name, a constraint set over package coordinates, the platform and toolchain its realizations run under — the same request-input half of realization identity 0039 fixed, carried as the value the build requests carry — and the origins whose binary offers it admits, which is 0042's local policy finding its home: the authorization was always "a person's configuration", and the environment is that person's declared thing. resolving the declaration through the ordinary solver and locking the answer produces the environment's lock; realizing the lock's entries against offers produces the substitution records; the document is the three together.

two environments over different declarations are different documents even when their constraints overlap, and one environment re-resolved under a moved input is a different document whose diff names the input that moved. one consequence is worth stating because it answers 0039's multi-platform question at the environment level: two platforms under one declaration share one lock — the lock binds source only — and differ in the document's realization coordinates, so per-platform environments are per-platform realizations of one selection rather than re-selections.

### what "reproducible" asserts, in 0014's vocabulary

the first two of 0014's three properties, and only those. content-addressed identity holds by construction: the document is a value with a digest, and the lock it holds is one. clean-build equivalence of the *selection* holds on 0040's determinism contract: the resolution is a pure function of four recorded inputs, every one of which is in the lock's header or in the declaration, so any process resolving the same declaration against the same universe reaches the same environment. the rendering adds the same property one level out, on 0041's render discipline: the same document renders to the same bytes anywhere.

bit-for-bit reproducibility of the packages themselves is not asserted and cannot be. that is a property of build instructions and build environments, the engine verifies it rather than producing it, and a development environment that claimed it would be claiming its contents are deterministic when all it did was select them. the rebuild stays what 0014 and 0042 made it: the way a claim becomes a measurement, available as policy, never the default. what the environment commits to is checkable and checked: same declaration, same universe, same document digest and same rendered bytes across separate engines and durable states, and a moved input moving the digest with the input named.

### the effect boundary

computing an environment is pure: resolving through the engine, locking, realizing entries against offers, digesting, rendering. writing the lock and the environment's own record are caller effects at 0003's boundary, on exactly the ground 0041 put the lock's write on, published through the same discipline. entering an environment — mutating a caller's `PATH`, exporting variables, running hooks in a person's shell — is an effect too, and M-4 ships none of it; `print-dev-env` is the precedent for what ships instead, the environment rendered as data for a consumer that has not been written. the mistake this section exists to prevent is a pure rule that touches a path or reads the process environment during evaluation, and the prototype holds the line the way 0041's did: resolving an environment through the engine touches no path until a caller acts. the toolchain a declaration carries is discovered before any request exists, on 0028 and 0030's terms — `Toolchain::discover` is caller-side host configuration — and nothing inside evaluation re-discovers it.

### the lock questions 0041 handed over

one lock per resolution, and an environment is one resolution. a project that runs several environments — a default environment for the repository's own development and a named one for a cross-compilation target, say — holds several locks, each the complete record of one resolution, each independently mergeable under the union behaviour 0041 fixed for entries. merging two locks into one file is not offered and would be a fabrication if it were: the merged document would record a selection no resolution made.

placement is derived from the declaration rather than negotiated per call: the environment's lock lives in the project root, named `pith.lock` for the default environment and `<name>.pith.lock` for a named one, with the environment's own record beside it as `pith.env` / `<name>.pith.env`. the derivation is a pure function the library owns, so the convention is one spelling rather than a habit, and the publication functions stay caller effects that take the derived path.

### where substitution records persist

in the environment's own artifact — 0042's third option. the record carries the admitted substitutions beside the lock it realized against: each is the value 0042 fixed, every input the admission test consulted, and together they are the environment's answer to "which binaries did this machine admit". the lock is untouched by this and stays source-only, byte-identical when a substitution serves, because a substitution is a realization-level fact of one environment and the lock's refusal of binaries was argued on exactly that ground.

an amendment from the first prototype round: the refusal a rejected offer carries returns beside the document from the resolve, not inside it. a refused offer changes nothing about what the environment serves — the build runs, the same realization an absent offer produces — so a refusal in the document would make a stale mirror's rejected offer part of what "the same environment" means. the explanation still arrives every time, naming the clause and both sides of the comparison; it just does not move the digest. the initial implementation dropped it at this boundary entirely, which is the failure the amendment closes.

the relation between the two artifacts is naming, not nesting: the record names the lock by digest, the substitutions name the bindings they realize, and each file keeps the merge shape natural to it — the lock's entries union-merge because selections are sets, and the record's substitution lines are keyed by binding, where a union that produced two admissions for one binding would be the same conflict a double bind is in the lock. M-4 renders the record and round-trips it as a value; a reader for the rendered text waits until something consumes it.

### confinement, stated plainly

a development environment confines nothing. 0028's landlock and seccomp confine *actions* — processes the executor forked under a declared contract — and 0030's closure confines the toolchain those actions declared. an interactive shell is the caller's own process: it is not exec'd by the executor, it has no declared contract, no `AccessVerification` applies, and its authority is the user's. guix's `--container` shows what the honest version of the opposite claim costs — namespaces, network policy, a passwd entry to fabricate — and devcontainers get confinement only by making the image the unit. an environment that let a reader infer a sandbox would be 0014's "an action marked hermetic without evidence" wearing a new word, so the record says it outright: no isolation claim is made, and the confinement machinery the executor owns is never invoked on a person's shell.

### coexistence with a foreign toolchain

0032's seam composes, because the two halves touch at a value rather than a mechanism. a repository whose environment is pith's and whose build is `cargo` inside an `Opaque` holds: a pith environment (declaration, lock, record) that vouches for the toolchain and packages in scope, and an `Opaque` target that declares itself foreign, with cargo's own `Cargo.lock` sitting beside pith's lock as a second, provenance-tier-visible resolution. the categories are visible per target, which was 0032's whole requirement, and the environment does not pretend to key the foreign interior: a changed pith environment invalidates nothing inside the `Opaque`, because the seam between them is the tool paths in scope, not a computation key. the cost is named rather than hidden — the repository resolves twice, in two systems whose provenance claims are not comparable, and keeping the two honest is a person's job, on the same terms 0032 left it.

### retention

the environment adds one written artifact class to the repository — the record — and the repository's retention is version control's, on 0041's ground for the lock. the engine's two stores see nothing new: the resolution's attempts sit under their computation keys and follow 0027's ordinary axes, and the substituted binaries are ordinary content-addressed bytes, collectable and re-servable, because content identity is intrinsic in 0039's sense and a collected binary is re-offered and re-admitted rather than re-resolved. nothing the environment owns is a pin. the document is reconstructible from the declaration and the lock, and the record from re-realizing the lock against the offers, which is the same position the lock already holds: the file is the durable half because it crosses processes, and everything engine-side remains cache-shaped and collectable.

## alternatives considered

### the environment as its own lock

Spack's shape: `env.yaml` as manifest, `env.lock` as the environment's own lockfile, with its own concretization and its own guarantees.

rejected because 0041's lock already is that artifact. a second lock format would fork the merge story, the staleness diff, and the witness question across two files that record the same kind of fact, and the environment-specific content — realization coordinates, substitutions — is precisely what 0042 kept out of the lock for argued reasons. Spack needs its own lock because its manifest is not a resolution input in the same sense; phloem's declarations already are.

### the environment as a materialized directory

the venv position: the environment is the directory its tools populate, and its identity is what is installed.

rejected as identity. a directory cannot be diffed against what was declared, cannot cross processes as anything but itself, and answers "is this the environment" by inventory rather than by digest. materialization survives as a projection of the value, which keeps the directory-shaped future open: a materializer would be one more caller effect rendering the document into a tree, checked against the document's digest, not the environment itself.

### entering as an engine-scheduled effect

the activation as an action or effectful computation the engine runs, with the mutated shell as a declared output.

rejected on the same ground 0041 rejected the write as an action, with 0032's test added: an interactive shell has no declarable contract — it wants the network, the home directory, and whatever the person runs next — so it is the `Opaque` case, not the `Action` case, and dressing a person's shell in admission machinery would be claiming a contract by costume. the boundary stays: the engine computes data, the caller applies it.

### substitution records in the engine store

file the admitted records under an engine key so they survive the process like attempts do.

rejected because the engine learns nothing about offers, on 0042's terms, and because the records are package-domain values about a person's policy, not engine observations. an engine-keyed record would also inherit invalidation semantics that describe nothing: no computation moved when an origin was admitted.

### one lock per project

merge every resolution's entries into a single repository-wide lock.

rejected as fabrication. two resolutions over different constraint sets are different documents even when their entries overlap, and a file holding both would record a selection nobody made, which is exactly the union-merge conflict 0041's reader refuses — promoted to a file format.

## consequences

phloem gains the environment slice: the declaration as a value, the document as a value over the lock, the realization of a declaration against offers producing the document with the refusals beside it, the deterministic rendering, the diff that names the moved input across the lock, the realization coordinates, and the substitution set, and the pure path derivation for the lock and the record. no kernel constructor lands, which makes this the fourth M-4 record to need none. the engine is unchanged, and xylem is unchanged beyond the toolchain value the declaration reuses.

M-4 is closed by this record: package identity, constraints, lock data, binary reuse, and the environment have each landed a record and a prototype.

### measured

the prototype checks the record's claims directly. an environment declared as a value resolves through the engine and locks through 0041's lock, and the document carries the lock unchanged. two materializations of one declaration, through separate engines over separate durable states, produce the same rendered record and the same content identity — the sameness asserted over the rendered bytes and the digest, never over the declaration both came from. a changed declaration moves the environment, and the difference names the input that moved: a moved universe or preference list arrives as the lock diff's named change, a moved toolchain or platform as the environment diff's own. the lock's placement is what the library derives — `pith.lock` for the default environment, `<name>.pith.lock` beside it for a named one — and two environments in one project hold two locks. resolving an environment through the engine touches no path until the caller writes. a served substitution persists in the environment's record while the rendered lock stays byte-identical, and a refused one leaves the record without it and returns its refusal beside the document, naming the clause and both sides of the disagreement. the record round-trips through its value, and the declaration round-trips through its own.

## unresolved

entering the environment — applying the rendered record to a shell, direnv-style or otherwise — is unwritten, and so is a reader for the rendered record. both wait on a consumer that exists, and neither changes the boundary this record draws.

the declaration names processes and services not at all. devenv's whole ground — supervised process sets as environment content — is unmodeled here, and whether it belongs to the package library or to a composition over it is a question for a repository that needs it.

what an environment declares about machines, placements, and deployment targets — the M-5 and M-6 compositions — is untouched, on the terms 0040 left M-6: the protocol is there when those domains declare their own constraints.

the environment record's text format carries no version line and no merge tooling. pre-release it breaks freely on the lock's terms; when the lock's file becomes a stable contract, this file's question returns with it.
