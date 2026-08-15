---
schema: design-doc/v1
id: research-package-builds
title: package builds
summary: how Nix's stdenv, Spack's package.py, Guix's build systems, Debian's debian/rules, Cargo's .crate and build.rs, portage's ebuild phases, and Homebrew's formula each get from a fetched source to a built artifact, and where each sits on the axis the disagreement actually runs: whether a package's build is data the system can inspect or code it can only run, and how much the packager writes versus inherits
kind: research
status: researching
evidence: preliminary
created: 2026-07-13
updated: 2026-08-14
tags:
  - research
  - packages
  - build
relations:
  informed_by: []
  depends_on:
    - research-method
    - research-nix
    - research-build-systems
    - research-binary-reuse
  supersedes: []
---

# package builds

every system that ships packages performs the same move: it has fetched bytes identified by a digest, and it must turn them into an installed artifact. the move has two halves the systems disagree about. who owns the procedure — the package's author or the system — and in what form the procedure exists: data the system can inspect before running, or code it can only execute. the two disagreements are related but not the same, and the field sorts differently on each.

## Nix: stdenv's phases, a fixed inherited script

the build procedure is one bash library, `pkgs/stdenv/generic/setup.sh`, and the derivation's builder is a one-line script that sources it and calls `genericBuild`. the phases are fixed by `definePhases` — unpack, patch, configure, build, check, install, fixup, installCheck, dist — and `genericBuild` loops over them; `runPhase`'s override mechanism is that it will "evaluate the variable named $curPhase if it exists, otherwise the function named $curPhase," so a derivation attribute can shadow any phase, and every phase body is `runHook preX ... runHook postX` for finer entry points. `src` — the fixed-output derivation `fetchurl` produced — enters as an environment variable the unpack phase errors on if absent, and `buildInputs` are consumed at setup time by sourcing each input's setup hook and extending `PATH`.

a vanilla autotools package writes zero build code: `pname`, `version`, `src`, `buildInputs`, and everything else is inherited. but the procedure is code — shell carried as strings in environment variables that setup.sh evals. the derivation record (`Derive(...)` in ATerm, visible via `nix show-derivation`) holds the builder, arguments, and environment, so the phase *overrides* are inspectable in principle, as strings; the procedure they override is a store path the record only points at.

## Spack: build systems as python classes

a package is "a Python module `package.py` … [that] contains a package class and sometimes a builder class that define its metadata and build behavior," and "essentially, a package translates a spec into build logic." version lines carry the checksums ("Spack uses these checksums to verify that downloaded source code has not been modified"), `depends_on` accepts full specs, and the build system a package inherits defines its phases: "each build system defines a set of phases that are executed in a specific order … These are Python methods with a sensible default implementation that can be overridden by the package author." after concretization, `spack install` instantiates a builder that "behaves like a sequence, and when iterated over return[s] the `phases` of the installation in the correct order," running each phase method with `@run_before`/`@run_after` hooks around it.

the phase sequence is inspectable metadata — `spack info --phases m4` prints it — but the phase bodies are python methods, code the system can only call. a typical packager writes version lines, dependencies, and maybe `configure_args()`; the phases come from the base class. spack's own process stages, fetches, and runs the phases; there is no derivation and no daemon, so "inspect before running" means introspecting python objects.

## Guix: phases as first-class data over code

guix is the field's unusual entry: the procedure is *both* data and code, because scheme is homoiconic. a package names source (an `<origin>` whose sha256 "is mandatory"), a build system ("the build-system field specifies the procedure to build the package"), and arguments as g-expressions. `%standard-phases` is "standard build phases, as a list of symbol/procedure pairs" — an alist a package rewrites with `modify-phases`, deleting a phase or inserting one before another, and the manual's own gloss on g-expressions is that it "boils down to manipulating build code as data." declaring a build system also injects "implicit inputs … because package definitions do not have to mention them."

the lowering is explicit: the build system's `lower` turns package plus spec into a bag whose builder is a gexp staged "for later execution" by the offload daemon. the phase alist is composable and inspectable before lowering; each phase's value is a procedure that runs at build time. this is the only surveyed system where "the build is data" is literally true of the procedure and not only of its parameters.

## Debian: the build is a makefile the packager writes

a source package is the upstream tarball plus `debian.tar.xz` described by a signed `.dsc`, and "this file must be an executable makefile": `debian/rules` "contains the package-specific recipes for compiling the source (if required) and constructing one or more binary packages." dpkg-buildpackage drives the required targets — clean, build, binary — and policy constrains behavior ("required targets must not attempt network access to other hosts") rather than content. the archive can run the makefile; it cannot read it as a plan.

debhelper's `dh` sequencer is where inheritance returns: a one-line `dh $@` works "for packages where the default sequences of commands work with no additional options," and override targets (`override_dh_command`, `execute_before_`/`execute_after_` hooks) patch single steps of a roughly fifty-step sequence. so the debian position is code at the boundary — the makefile — with a data-shaped inheritance convention layered on top of it, and build-dependencies as the other honest data half (`debian/control`, checked by the driver).

## Cargo: the toolchain owns the procedure, the package ships a script beside it

the published `.crate` is a normalized source tarball — `cargo package` "will create a distributable, compressed `.crate` file," rewriting and normalizing the manifest — carrying `Cargo.toml`, `Cargo.lock`, and sources by convention. cargo itself owns the compile and link: the package declares targets and metadata, and even the tool invocations are configurable only as decoration (the `[build]` table's `rustc`, `rustflags`, `jobs`).

`build.rs` is the exception that carries code: "placing a file named build.rs in the root of a package will cause Cargo to compile that script and execute it just before building the package," it "may perform any number of tasks," its outputs live in `OUT_DIR`, and it talks back only through stdout directives ("cargo will interpret each line that starts with `cargo::` as an instruction"). the manifest is data; the build script is code the package ships, which cargo must compile and run before it can even know the link flags. the dependency the build places in the graph is therefore not inspectable until the script has run — the cost of code at that position.

## portage: bash phases with inherited defaults

"ebuilds are just bash scripts that are executed within a special environment." the declarative variables — `SRC_URI`, `DEPEND`, `RDEPEND` — are data inside the script, and the phase functions (`src_unpack`, `src_prepare`, `src_configure`, `src_compile`, `src_install`, …) run in a fixed order with system defaults: "the default pkg_nofetch and src_* phase functions are accessible via a function having a name that begins with `default_`," and a redefined `default` inside an override falls through to the inherited body. eclasses are "a collection (library) of functions or functionality that is shared between packages," inherited at the top of the ebuild, and supply whole phase bodies — cmake, autotools — that the ebuild overrides piecemeal. code, but structured as overridable phases over inherited defaults, the same shape as spack's and stdenv's inheritance wearing bash.

## Homebrew: the formula writes the whole procedure

a formula is "a package definition written in Ruby" whose `url`, `sha256`, and `depends_on` are data — "to verify the cached download's integrity and security we verify the SHA-256 hash matches what we've declared" — but `install` is a ruby method "overridden in Formula subclasses to provide the installation instructions," and the cookbook's idiom is the author shelling out by hand: `system "./configure", "--prefix=#{prefix}"`, `system "make", "install"`. nothing is inherited except the environment — download, checksum, extract to buildpath, staging to the cellar — and the bottle is the payoff for that bareness: "bottles are simple gzipped tarballs of compiled binaries," poured unless absent, stale, or `--build-from-source, so most users never run the code path the formula author wrote.

## the disagreement, stated

on the procedure's form: **data at the parameters, code at the procedure** is the near-universal compromise — nix (attributes are data, phases are shell), spack (specs are data, phases are python), debian (control is data, rules is a makefile), cargo (manifest is data, build.rs is code), portage (variables are data, phases are bash), homebrew (url and sha256 are data, install is ruby). **data all the way down** exists once: guix, where the phase alist is manipulable data whose elements are procedures, possible because scheme is homoiconic — and even there the procedures run as code at build time.

on who writes it: **the system owns a fixed procedure the packager parameterizes** — stdenv, debhelper, eclass defaults, cargo's pipeline, spack's and guix's build systems — versus **the package owns the procedure** — debian/rules, homebrew's install, an ebuild's or package.py's overridden phases, build.rs. the first position keeps every package's build inspectable and uniform at the cost of expressiveness; the second can build anything at the cost of a procedure the system cannot check before running it. cargo straddles: the pipeline is the system's, the pre-build hook is the package's, and the hook's effects reach the build through a data channel (stdout directives) precisely so the system can keep reading them.

the axis is 0038's. "rule bodies are data" is the same disagreement at the level of one engine: a host rule is debian/rules, and a represented rule is the gexp position. a package build that had to choose today chooses between a procedure the library can inspect, key, and invalidate, and a script it can only execute — and a fetched source settles part of it, because downloaded bytes cannot carry host code the engine could run at all.

## questions for this project

- is a pith package's build a declared procedure from a closed set (the stdenv/guix position), a script the package ships (the debian position, needing an `Opaque` or 0038's represented bodies), or something the package registers as a rule (the spack position, host code)?
- what is the minimal artifact interface needed by both build and deployment libraries — is "a nominal content identity produced by a declared interface" the whole of it, and does anything else cross that boundary?
- where does the unpack sit: an action invoking a tool, a pure parse over fetched bytes, or a caller-side effect beside the fetch? which systems even separate it — stdenv's unpackPhase runs inside the derivation; cargo unpacks the .crate in-process; guix's unpack is a phase.
- when a procedure is data, what invalidates it — a revision digest over the declaration, the way 0023 separates identity from revision for rules?
- where do per-package options (defines, flags, features as build inputs rather than coordinates) enter without reopening the arbitrary-script door?

## sources

- [nixpkgs stdenv setup.sh](https://raw.githubusercontent.com/NixOS/nixpkgs/master/pkgs/stdenv/generic/setup.sh)
- [nixpkgs stdenv chapter](https://nixos.org/manual/nixpkgs/unstable/#sec-stdenv)
- [nix derivation execution semantics](https://nix.dev/manual/nix/2.19/language/derivations)
- [Spack packaging guide](https://spack.readthedocs.io/en/latest/packaging_guide.html)
- [Spack builder.py](https://github.com/spack/spack/blob/develop/lib/spack/spack/builder.py)
- [Guix defining packages](https://guix.gnu.org/manual/en/html_node/Defining-Packages.html)
- [Guix build systems](https://guix.gnu.org/manual/en/html_node/Build-Systems.html)
- [Guix build phases](https://guix.gnu.org/manual/en/html_node/Build-Phases.html)
- [Guix g-expressions](https://guix.gnu.org/manual/en/html_node/G_002dExpressions.html)
- [Debian policy chapter 5: source packages](https://www.debian.org/doc/debian-policy/ch-source.html)
- [debhelper dh(1)](https://manpages.debian.org/testing/debhelper/dh.1.en.html)
- [dpkg-buildpackage(1)](https://manpages.debian.org/testing/dpkg-dev/dpkg-buildpackage.1.en.html)
- [cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)
- [cargo package layout](https://doc.rust-lang.org/cargo/guide/project-layout.html)
- [cargo package command](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
- [gentoo development guide: ebuild writing](https://devmanual.gentoo.org/ebuild-writing/functions/)
- [gentoo development guide: eclasses](https://devmanual.gentoo.org/ebuild-writing/using-eclasses/index.html)
- [homebrew formula cookbook](https://docs.brew.sh/Formula-Cookbook)
- [homebrew formula API](https://docs.brew.sh/rubydoc/Formula.html)
- [homebrew bottles](https://docs.brew.sh/Bottles)
