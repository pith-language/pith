---
schema: design-doc/v1
id: decision-0036-produced-program-as-content
title: a program the graph produced enters an action as content
summary: split ActionSpec::executable into a typed sum of host path and content identity, so a build product can be the program an action runs without laundering a content identity through a field meant for an external one
kind: decision
status: proposed
created: 2026-06-12
updated: 2026-06-12
tags:
  - actions
  - build
  - content
  - executor
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0003-explicit-effects
    - decision-0005-separate-identities
    - decision-0028-sandboxed-local-executor
    - decision-0030-toolchain-closure-as-declared-input
    - decision-0031-action-cache-identity
    - decision-0032-action-granularity
  supersedes: []
---

# a program the graph produced enters an action as content

> amends [0030: a toolchain enters an action as a declared closure of host paths](0030-toolchain-closure-as-declared-input.md), which replaced the executable blob with a host path. 0030 stands for the case it argued: a compiler is not one file. this record covers the case its argument does not reach, where the program is one file the graph itself produced.

## context

M-3 asks for tests and for generated input, and both need to run something the build made. `crates/xylem/tests/two_source_build.rs` does run the executable it builds, in `the_built_executable_runs_and_exits_with_the_expected_code`, by writing the captured bytes to a file in the test and spawning it. that run happens outside the engine entirely. it is not confined, not cached, not recorded in provenance, and not a dependency of anything. for the one artifact a build most wants to re-run on every change, the graph knows nothing.

the reason is `ActionSpec::executable`. 0030 made it a `Box<str>` host path that the executor `execve`s directly and never stages, and the argument for that was specific: `cc` is a driver that execs `cc1` and `as` and finds them through paths baked into its own binary, so a contract claiming "the executable is these bytes" is claiming something it cannot keep. that argument is sound and this record does not disturb it.

it also does not extend. a test binary is one file. a generator is one file. for those, "the executable is these bytes" is exactly true, and it is the only true thing: the bytes exist in the engine's content store, they were produced by an action the graph recorded, and no host path names them until something writes them somewhere.

[0032](0032-action-granularity.md) closes the obvious way around the problem. an `Action` is one invocation of one tool, and its list of examples is `gcc`, `as`, `ld`, `ar`, `rustc`, `javac`, `protoc` — things that "invoke once, read what they are given, and write what they are asked for." running a test binary under `/bin/sh -c 'prog; echo $? > status'` is two invocations with a shell choosing what happens between them, which is the shape 0032 reserves for `Opaque`. so the program has to be the thing itself.

## proposed decision

`ActionSpec::executable` becomes `ActionProgram`, a sum of two variants:

- `ActionProgram::HostPath(Box<str>)`, the absolute host path the executor `execve`s, with the rest of the program's installation declared in `ActionSpec::toolchain`. this is 0030's case, unchanged in behaviour and in what it claims.
- `ActionProgram::Content(ContentId)`, content the engine owns. the executor stages it and runs it from there.

the type is the point. [0005](0005-separate-identities.md) says the core "distinguishes semantic identity, computation identity, content identity, and external identity" and that "the type system prevents accidental substitution." a host path is an external identity: it names something outside the engine, and what those bytes are is a fact about the host rather than a claim the contract makes. a `ContentId` is a content identity the engine owns and can verify. keeping both in one `Box<str>` would put a content identity in a field typed for an external one, which is the substitution 0005 has a type system in order to prevent.

### where a content program runs from

the executor writes the bytes to `program` in the scratch root, beside the `work` and `tmp` directories 0030's staging already creates, and sets the executable bit. the scratch root is inside the landlock ruleset with the full access mask, so the exec is permitted; a declared input path can never collide with it, because declared paths are relative to `work`; and it cannot be mistaken for a declared output, for the same reason `tmp` cannot.

the bytes reach the executor the way an input's bytes do. `materialize_action` resolves the program from the content store and packs it into `ActionInvocation::program`, so the executor still never touches the store and still never sees a content identity it could fail to resolve, which is the property 0028 established for inputs. an invocation whose contract names content but carries no bytes is refused with a diagnostic rather than executed against whatever else the contract names.

### the runtime closure

a produced binary is dynamically linked. it names its loader in `PT_INTERP` and finds its libraries through `RUNPATH`, and under a nix toolchain both are store paths: the measured case is `PT_INTERP` at `/nix/store/…-glibc-2.42-67/lib/ld-linux-x86-64.so.2` and `libc.so.6` resolved through a `RUNPATH` naming that same glibc and the gcc support library. those paths are already in the toolchain closure the compile and link actions declare, so a run action declares the same closure and the loader resolves.

this is the `toolchain` field carrying a runtime closure rather than a compile-time one. the field's documented meaning is "host filesystem paths the action may read to find the rest of its toolchain," which fits: for a produced program, the rest of what it needs to run is the loader and the libraries it was linked against. the naming is uncomfortable and is named in "unresolved" below.

## alternatives considered

### a scratch-relative host path

leave `executable` a `Box<str>`, stage the produced program as an ordinary declared input, and let the field name a path relative to the working directory. this works today with no type change: `current_dir` is already the working directory and the landlock mask already grants execute beneath the scratch root.

rejected because it makes the field an untyped sum. the string would mean an external identity when it starts with `/` and a thing the graph produced when it does not, and a reader would have to know that rule to know which. 0005 puts identity kinds in the type system precisely so that a reader does not have to. [0015](0015-interface-rule-selection.md) refuses ambiguity in selection for the same reason: a distinction that a shape implies rather than a type states is a distinction that will eventually be read wrong.

### run it through the loader

keep `executable` a host path naming the glibc `ld-linux-x86-64.so.2` that is already in the declared closure, and pass the staged program as its first argument. no contract change at all, and it is an honest description of what `execve` of a dynamic binary does anyway.

rejected on what provenance would then say. every test action and every generator action would record its program as the loader, and the thing that actually ran would be one entry in an argument list. [0003](0003-explicit-effects.md) makes authority visible, and 0014 wants provenance to carry measured facts; a record in which every run action ran the same program is neither. it also bakes a per-toolchain loader path into every rule that runs a product.

### wrap the program in a shell

`/bin/sh -c './prog'`, with the product as a declared input. the smallest change, and it makes the exit status reachable as a written file at the same time.

rejected by [0032](0032-action-granularity.md): an `Action` is one invocation of one tool, and this is two with a shell deciding the order. it also adds a shell and a coreutils closure to the trusted set of every test, and makes the recorded program `/bin/sh` for all of them, which is the provenance objection above in a different costume.

### keep running products outside the graph

what the fixture does today: capture the executable, write it to a file in the test, spawn it there.

rejected because it is the status quo and it is what M-3 is asking to fix. a run that happens outside the graph gets no confinement, no cache entry, no provenance, and no place in the dependency record, so a test result cannot be reused and a generated source cannot be a dependency of the thing compiled from it.

## consequences

every `ActionSpecDigest` changes. the manifest now carries a tag byte before the program, so contracts that are otherwise identical to ones digested before this record hash differently, and persisted action cache entries stop matching. this is the same consequence as bumping a rule revision, and it is a one-time cost of putting the variant in the digest at all; the alternative, encoding the host-path case exactly as before so old digests survive, would leave the manifest with an implicit variant and reintroduce the shape this record rejects.

the storage encoding goes to version 2. 0024 versions the stored form independently of the digest manifest for exactly this reason, so the change is a version bump and a tagged read rather than a migration.

a produced program's identity is in the contract digest, so it is in 0031's request-side action key. rebuilding the program makes every action that runs it a different action, which is what a build wants: a test whose binary changed is a test that has not been run.

the executor stages a third entry in the scratch root. staging stays pure filesystem work with no `unsafe`, and the only new authority is the executable bit, which the executor already sets on tree entries that carry it.

## unresolved

the `toolchain` field now carries a runtime closure for a content program and a compile-time one for a host-path program. both are "host paths this action may read to find the rest of what it needs," which is why one field still fits, but the name says toolchain and one of the two is not a toolchain. whether to rename the field, split it, or leave it is open, and the answer probably wants more than one kind of run action to look at first.

a content program must be a blob. a program that is a directory — an interpreter beside the library it needs, a binary with a sibling data file — has no representation here, and `Content::Tree` would need a way to say which entry is the entry point. nothing in M-3 needs it, and inventing the entry-point convention before something does would be guessing.

the executable bit is the executor's decision rather than a property of the content. a `Content::Tree` entry carries its own executability, and a top-level blob does not, so "these bytes are a program" is expressed by which field of the contract names them rather than by anything about the content itself. that asymmetry may be right, since executability is a filesystem property rather than a content property, but it is worth stating that the two levels of the content model disagree about it.

whether an executor may refuse a content program it considers unsafe to run is not addressed. the local executor runs what the contract names, confined by 0028's two layers, and a produced program is confined exactly as a toolchain is. a remote executor accepting content programs from a cache is a trust question that belongs with 0024's remote-cache boundary rather than here.
