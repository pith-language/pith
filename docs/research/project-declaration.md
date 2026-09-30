---
schema: design-doc/v1
id: research-project-declaration
title: declaring a project
summary: how Nix flakes, Nix path values, Zig, Bazel's bzlmod, Cargo, CMake, Go modules and Dhall declare dependencies, pass parameters to them, name files, and why several of them restrict the part of the file that declares dependencies
kind: research
status: researching
evidence: preliminary
created: 2026-09-23
updated: 2026-09-30
tags:
  - research
  - modules
  - language
relations:
  informed_by:
    - research-nix
    - research-module-distribution
  depends_on:
    - research-method
  supersedes: []
---

# declaring a project

> edit, 2026-09-30: the paths paragraph of the result below adopts Nix's resolution against the file's own
> directory. [0081](../decisions/0081-paths-are-values.md) and
> [0086](../decisions/0086-names-are-scoped-per-file.md) resolve every path against the project root instead,
> from whichever file it is written in.

[0079](../decisions/0079-a-project-is-one-file.md), [0080](../decisions/0080-inputs-are-parameters.md) and
[0081](../decisions/0081-paths-are-values.md) propose one project file whose `inputs` block is ordinary
pith with one scoping rule, inputs that are parameters with defaults, and typed paths. this note reads the
systems those records were compared with. sources were read on 2026-09-23.

## Nix flakes

flakes are the design the one-file project is closest to.

### pressure

Eelco Dolstra's introduction on the Tweag blog (2020-05-25) names three problems: evaluation was not
hermetic ("Nix files can access arbitrary files (such as `~/.config/nixpkgs/config.nix`), environment
variables, ... `$NIX_PATH`"), there was "no standard way to compose Nix-based projects", and projects lacked
a standard structure. RFC 49 was opened on 2019-07-15; the implementation was merged without the RFC being
accepted, which nix.dev lists among the criticisms.

### why inputs are literal

the stated reason is termination and cost when reading metadata, not reproducibility. Dolstra in nix#3966
(2020-08-28): the restriction "ensures that a command like `nix flake info` doesn't have to evaluate an
arbitrarily complex (and possibly non-terminating) Nix expression." again in nix#4945 (2021-06-25): "It's a
subset of Nix that doesn't allow computation in the flake metadata attributes ... to prevent arbitrarily
complex, possibly non-terminating computations while querying flake metadata."

`src/libflake/flake.cc` enforces this by forcing only trivial thunks, so a `let`, an `import` or a
concatenated URL fails with "expected %s but got %s". users report the result as "flake.nix is not
Nix" (#3966, #4945, #5373).

the second reason is editing by tools. Robert Hensing in #3966 (2020-10-10): "we don't have a way to write
back modified asts without touching unmodified whitespace and comments." RFC 193 (2025-12-07, open) moves
inputs into `flake.toml`, citing both the confusion of mixing a restricted and a full language in one file
and that "programmatically editing flake inputs requires Nix AST manipulation."

Hensing also argues (#4945, 2024-01-10) that static inputs do not enumerate a flake's sources as long as
the body can still fetch; their value is bootstrapping and locking.

### parameters were refused, then rebuilt on top

Dolstra in nix#3843 (2020-07-21): "Not passing in `system` (or any other arguments) is intentional." the
reasons given: it breaks hermetic evaluation, makes the flake's contents impossible to enumerate, makes
caching harder, and means "fully-qualified flake output attributes no longer uniquely determine their
evaluation result." in nix#5663 (2021-11-29) he adds that value inputs "are global": two values of one
parameter mean cloning the input. Hensing in the same thread (2024-03-03): "let's just call them
functions. Functions with optional defaults," and in #3843 (2025-01-04) that a `system` parameter should not
reach outputs such as `overlays` that do not depend on it.

users rebuilt parameters from locators: nix-systems reserves an input named `systems` pointing at a file
of system names, overridden with `--override-input` or `follows`.

### follows and the lock

`follows` redirects an input to another by a `/`-separated path from the root. its problems cluster in
nested use: #4808 (2021-05-14) on unclear semantics outside the root, #5790 (2021-12-17, open) on
transitive `follows`. zimbatm's "1000 instances of nixpkgs" describes the cause: every dependency
brings its own nixpkgs.

the lock records the whole graph transitively; generating it reads dependencies' locks, and an up-to-date
lock does not.

### outputs and run

the output schema is open. `nix run` tries `apps.<system>.default` then `packages.<system>.default`, and for
a named attribute `apps`, `packages`, then `legacyPackages`. a derivation is run as
`<out>/bin/<mainProgram>`, falling back to `pname` and then the name. per-system duplication produced
`forAllSystems`, flake-utils and flake-parts; flake schemas (nix PR #8892, 2023-08-31, open) exist because
tools only knew the built-in output kinds.

### paths and self

a flake inside a git repository sees only tracked files, and uncommitted changes make it "dirty". copying
the whole tree to the store before evaluating made `nix build .#hello` in nixpkgs slow (#3121, 2019-10-07);
it stayed open until #15711 was merged on 2026-04-27.

## Nix path values

paths are a type apart from strings. a relative literal resolves against the directory of the file it is
written in. `..` is removed lexically and symlinks are not resolved. using a path in a string copies it to
the store. with `src = ./.;` the store path is named after the directory, so "your build is no longer
reproducible, as it depends on the parent directory name" (nix.dev best practices); `builtins.path` with an
explicit `name` is the fix.

`lib.fileset` in nixpkgs replaces filter callbacks with typed file sets. its design notes record strict
existence checks "because you wouldn't be protected against typos anymore", with the example
`difference ./. ./sercet`.

## Zig

in the 2018 sketch in issue #943, dependencies were declared inside `build.zig`. by 2021-01-20 Andrew Kelley
wrote that "the set of *possible* dependencies will be purely declarative, so it will be practical to have
a 'fetch' step of package management that does not execute arbitrary code." PR #14265 (2023-01-11) states
the split: "The real, actual file that signifies a zig package is a build.zig, and the existence of this
extra file is bonus - it is for the case of declarative information that we want to expose without
requiring execution of zig code." it rejected TOML because a machine writing the file back meets
"multiple ways to convey the same meaning". PR #14523 moved to `build.zig.zon`, parsed by Zig's own parser.

in `build.zig.zon` the hash is the source of truth and the URL "just one of many possible mirrors"; the
manifest is the lock.

`b.dependency(name, .{ .target = t, .foo = x })` passes arguments to a dependency, the same map `-Dfoo=`
fills from the command line. instances are cached per package and argument set, so one package with two
argument sets is two instances, and an argument the dependency does not declare fails with "invalid
option". the dependency declares its options by calling `b.option` when its build script runs, so the set
of parameters is known only by running it.

`LazyPath` distinguishes a path in the package, a generated file, a path in a dependency, and
`cwd_relative`, documented as "uncommon ... Use of this tag indicates a dependency on the host system."
`b.path()` refuses absolute paths and `dirname` is "not allowed to escape the logical root".

steps such as `run` and `test` are registered by name in `build.zig`.

## Bazel bzlmod

`WORKSPACE`'s `load()` chains were sequential; fmeum in bazel#17880 (2023-03-24) calls them "a major
contributor to its subpar performance". the Bazel 5.1 bzlmod guide: "The MODULE.bazel file is similar to
BUILD files in that it doesn't support any form of control flow; it additionally forbids load statements."
overrides "can only be used by the root module". splitting the file was argued against in #14632
(2022-08-29) because "segmenting the file gives the impression that each file can function independently,
but that's not actually true"; the later `include()` is limited to the root and keeps bindings file-local.

a rule declares `executable = True` or `test = True`; test rules are executable and their names end in
`_test`.

## Cargo

`Cargo.toml` is data and `build.rs` runs after resolution. features are parameters to dependencies, and
Cargo takes "the union of all features enabled on that dependency", so features have to be additive;
mutually exclusive choices have been an open issue since 2016 (#2980). `[patch]` (RFC 1969) replaces a
dependency everywhere in the graph and is read only from the root manifest.

## CMake

`FetchContent_Declare` is first-to-record-wins: "If such details have already been recorded earlier in this
project ... this and all later calls ... are ignored." override power depends on the order the project
evaluates in. options reach subprojects through a global variable namespace (policy CMP0077). there is no
lock.

## Go modules and Dhall

Russ Cox chose go.mod's format so that tools could "read, modify, and write back, preserving comments",
having found JSON, TOML, XML and YAML lacking. `replace` "only appl[ies] in the main module's go.mod file".

in Dhall, imports are syntax, resolved before normalization, and "Imported expressions may not contain any
free variables." the one computed part, request headers written with `using`, must be a closed expression
evaluated during import resolution. Gabriella Gonzalez declined to let imports compute hosts, because an
import could then carry program state out, with the example `https://example.com"/badguy?secret=${secret}"`
(dhall-lang #378). `sha256:` pins the hash of the normalized expression, not of the text.

## where they differ

Bazel overrides, Cargo `[patch]` and Go `replace` let a consumer replace a dependency deep in the graph,
and each is read only from the root. Nix allows `follows` at any depth, and Nix's bugs about inputs cluster
there. CMake's first-to-record rule makes the outcome depend on order.

systems that pass arguments to dependencies either instantiate per argument set (Zig) or merge arguments
across the graph (Cargo features), and merging forces the arguments to be additive.

the reasons given for restricting the dependency section are termination (Nix), fetching without running
code (Zig), sequential loading (Bazel), and editing by tools (Nix, Zig, Go). only the last is independent of
the language's evaluation model.

## result for this project

pith keeps the scoping rule for the inputs block and makes a smaller part of it literal. pith's evaluation
is pure and bounded, so Nix's termination argument and Zig's argument about running code before fetching
do not apply to it. the editing argument does: `pith add` and `pith update` have to rewrite locators
without disturbing the rest of the file. Dhall's refusal of computed hosts applies as well, since a locator
computed from machine facts could carry them off the machine. locators and their revisions are therefore
literals, and defaults, arguments and grants stay ordinary pith. this is recorded as an edit to
[0080](../decisions/0080-inputs-are-parameters.md).

inputs as parameters follow Zig's mechanism. an instance is keyed by its argument values, and an argument
the input does not declare is refused. two consumers passing different values get two instances, and
nothing is merged, so arguments do not have to be additive the way Cargo features do. unlike Zig, where
the parameter set is known only after the build script calls `b.option`, a pith input declares its
parameters in its own file, so the set is known without running anything.

replacing a dependency below a direct input is left to the root project, as with Bazel's overrides, Cargo's
`[patch]` and Go's `replace`. `with` passes arguments to a direct input only, and there is no path of
nested redirections like `follows`. how the root names a deep input is left open in 0080.

Hensing's point that a parameter should not reach outputs that do not use it is covered by the engine's
dependency tracking: an output that never reads `target` does not have it in its key.

for paths, pith adopts Nix's resolution against the file's own directory and adds four rules taken from the
problems above. a path is admitted when it is used, not with the whole tree (nix#3121). store objects are
named by content only. a literal naming a file that does not exist is refused, as in `lib.fileset`. what a
path sees does not depend on version-control state. these are recorded as an edit to
[0081](../decisions/0081-paths-are-values.md).

## sources

- Dolstra, Nix Flakes part 1, 2020-05-25: https://www.tweag.io/blog/2020-05-25-flakes/
- RFC 49: https://github.com/NixOS/rfcs/pull/49; RFC 193: https://github.com/NixOS/rfcs/pull/193
- nix issues #3121, #3843, #3966, #4808, #4945, #5373, #5663, #5790, #15711 and PR #8892:
  https://github.com/NixOS/nix
- flake manual: https://github.com/NixOS/nix/blob/master/src/nix/flake.md; nix.dev on flakes:
  https://nix.dev/concepts/flakes.html; best practices: https://nix.dev/guides/best-practices
- nix-systems: https://github.com/nix-systems/nix-systems; flake-parts: https://github.com/hercules-ci/flake-parts
- zimbatm, 1000 instances of nixpkgs: https://zimbatm.com/notes/1000-instances-of-nixpkgs
- lib.fileset design: nixpkgs `lib/fileset/README.md`
- Zig issue #943: https://github.com/ziglang/zig/issues/943; PRs #14265, #14523, #19597;
  `doc/build.zig.zon.md` and `lib/std/Build.zig`
- Bazel bzlmod 5.1 guide: https://docs.bazel.build/versions/5.1.0/bzlmod.html; issues #14632, #17880;
  rules: https://bazel.build/extending/rules
- Cargo features: https://doc.rust-lang.org/cargo/reference/features.html; cargo #2980; RFC 1969
- CMake FetchContent: https://cmake.org/cmake/help/latest/module/FetchContent.html; policy CMP0077
- Cox, Defining Go Modules, 2018-02-22: https://research.swtch.com/vgo-module
- Dhall imports standard: https://github.com/dhall-lang/dhall-lang/blob/master/standard/imports.md;
  dhall-lang #187 and #378
