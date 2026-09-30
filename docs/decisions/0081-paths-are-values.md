---
schema: design-doc/v1
id: decision-0081-paths-are-values
title: paths are values of three types, and a project path is identified by the content it names
summary: Path names a file or directory inside the project and is taken into the content store when the project loads; RelPath names a location inside a tree and is never absolute and never climbs; HostPath names a location on the machine and is usable in action contracts only under a grant; a glob over the project's own files is allowed because those files are a declared input
kind: decision
status: proposed
created: 2026-09-21
updated: 2026-09-30
tags:
  - types
  - language
  - identity
relations:
  informed_by:
    - research-project-declaration
    - research-nix
  depends_on:
    - foundation-principles
    - requirements-security-and-trust
    - decision-0026-generic-typed-calculus
    - decision-0072-action-plans-are-authorized-against-consumer-grants
    - decision-0079-a-project-is-one-file
  supersedes: []
  amends:
    - decision-0026-generic-typed-calculus
---

# paths are values of three types, and a project path is identified by the content it names

> edit, 2026-09-30: [0085](0085-typed-literals.md) makes `rel"..."` and `host"..."` two tags of its closed
> set of tagged literals. every path is resolved against the project root, as this record already resolves
> `Path`, and [0086](0086-names-are-scoped-per-file.md) applies the same rule to `use` paths written in any
> file. the unresolved question of how `ActionSpec`'s string fields become typed is answered by
> [0089](0089-exec-and-platform-types.md).

> edit, 2026-09-21: after [declaring a project](../research/project-declaration.md), a path is admitted when it is used, a literal to a missing file is refused, and loading does not depend on version-control state. see the edit section.

> amends [0026](0026-generic-typed-calculus.md): the calculus gains `Path`, `RelPath` and `HostPath`.

## context

pith has no way for a project to hand its own files to a rule; M-16's first review listed that gap. action
contracts carry locations as text: `ActionSpec::toolchain` is a list of strings, `ActionInput::path` and
`ActionOutput::path` are strings, and `pith.Exec.program` is text. a string cannot say whether it names
something in the project, inside a tree, or on the machine.

Nix has a path type, which is how a single Nix file names its own sources: `./src` is a value, and using
it copies the files into the store.

## decision

### three types

| type | literal | names |
| --- | --- | --- |
| `Path` | `./src/main.c`, `./include` | a file or directory inside the project |
| `RelPath` | `rel"bin/hello"` | a location inside a tree |
| `HostPath` | `host"/usr/bin/cc"` | a location on the machine |

### Path

a `Path` literal is resolved against the directory of the project file. when the project loads, the file
or directory it names is taken into the content store, and the value's identity is that content. moving the
project does not change any key; editing a file changes the keys of exactly the computations that read it.

a `Path` cannot name anything outside the project directory. a literal that resolves outside it is refused
at load, and so is one that crosses a symlink, as 0067 already refuses for sources. files outside the
project reach it as an input or as a `HostPath`.

a glob, `./src/*.c`, evaluates to a `List<Path>` sorted by relative path. it runs over the project's own
directory, which is the declared input, so it is not an ambient read.

`read(p)` gives a file's bytes, through the existing `NeedBlob` step.

### RelPath

a `RelPath` names a location inside a tree: where an input is staged in an action, where an output is
captured. it is never absolute and never contains `..`, so a value of this type cannot leave the tree it is
applied to.

### HostPath

a `HostPath` names a location on the machine, outside the content store. it has no content identity. it is
usable in action contracts and in `pith.Exec`, and an action contract that names one is authorized only if a
grant covers it (0072). because host locations have their own type, a plan's host dependencies show up in
plans and provenance.


## edit, 2026-09-21: after the research

the research changes one point of the decision above, whose original text is kept, and adds two.

- a `Path` is admitted into the content store when a computation first uses it, not when the project loads.
  Nix flakes copied the whole tree first, which made them slow on large repositories for six years (nix#3121, fixed by
  #15711 on 2026-04-27). the value's identity is still its content.
- a `Path` literal naming a file or directory that does not exist is refused at load, as `lib.fileset` does,
  so a typo cannot silently name nothing or the wrong thing.
- what a `Path` sees does not depend on version control: an untracked file is visible and there is no
  notion of a dirty tree, unlike flakes. store objects are named by content alone, never by the project
  directory's name (nix.dev best practices).

## alternatives considered

paths as text, as today, leave the three meanings to convention and let a string that names a host file
flow into a field that expects a project file.

one path type with a flag for its kind would be an unnamed boolean deciding what a value means. with three
types, a wrong use is a type error.

letting a `Path` climb out of the project with `..` would make the project's content depend on its
surroundings.

## evidence

- editing one file under `./src` recomputes exactly the computations that read it, and `pith explain` names
  the file
- moving the project directory does not change any computation key
- `./../x`, and a `Path` through a symlink, are refused at load
- an action contract naming a `HostPath` without a grant is refused under 0072

## unresolved

the canonical encodings of the three types, and how `ActionSpec`'s string fields become typed, are the
implementation round's.

whether a `Path` to a directory is taken into the store as one tree or entry by entry affects how much a
change to one file recomputes, and should be measured.
