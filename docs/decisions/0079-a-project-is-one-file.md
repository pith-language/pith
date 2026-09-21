---
schema: design-doc/v1
id: decision-0079-a-project-is-one-file
title: a project is one file holding its inputs, declarations and outputs, and splits only by naming its other files
summary: a project file starts with an optional module clause, its inputs and its host clause, and continues with types, rules, private values and outputs; other files join only through include; there is no manifest separate from source, no source discovery under src, and no standalone mode; a lock is written beside the file
kind: decision
status: proposed
created: 2026-09-21
updated: 2026-09-21
tags:
  - modules
  - language
  - usability
relations:
  informed_by:
    - research-project-declaration
    - research-language-frontend
    - research-module-distribution
  depends_on:
    - foundation-principles
    - decision-0061-the-declaration-artifact
    - decision-0067-local-module-workspaces
    - decision-0069-module-authority-is-consumer-configuration
    - decision-0070-module-acquisition-resolution-and-replay
  supersedes: []
  amends:
    - decision-0067-local-module-workspaces
---

# a project is one file holding its inputs, declarations and outputs, and splits only by naming its other files

> edit, 2026-09-21: the research this record said was owed is [declaring a project](../research/project-declaration.md). its consequences are in the edit section below.

> amends [0067](0067-local-module-workspaces.md): its manifest/source split, its ownership of every `.pi`
> file under `src/`, and standalone mode are replaced. its declared subject, its subject-based ABI, its
> refusal to follow symlinks, and its rejection of implicit workspace discovery stand.

## context

under 0067 a module is a directory: `module.pi` is a manifest that may not contain declarations, and the
module owns every `.pi` file under `src/`, discovered by listing the directory. nothing in either file says
which source files belong to the module. 0067 rejects implicit workspace discovery on the ground that
"loading must begin from the supplied path so that a check of one manifest is one deterministic input set",
but finds source files by listing a directory. it gives no argument for keeping manifest clauses out of source
files, and it already put both in one grammar ("a document context rather than a second configuration
language").

the goal set on 2026-09-21 is a project that is as easy to start as a single Nix file or a
`CMakeLists.txt`: one file, dependencies declared in one place as flakes do, and outputs other projects can
use.

## decision

### the file

a project is one file. it contains, in this order:

- an optional `module` clause naming the project's subject and version (0067's identity). it is required
  for a project that others depend on by subject and optional for one they do not.
- the `inputs` block ([0080](0080-inputs-are-parameters.md)).
- an optional `host` clause naming the component that serves the project's `= host` rules
  ([0071](0071-host-bodies-bind-through-host-adapters.md)), written with a path value
  ([0081](0081-paths-are-values.md)).
- `include` clauses naming other files that belong to the project.
- declarations: types, rules, private `let` values, and outputs
  ([0083](0083-outputs-and-commands.md)).

### splitting

a project that outgrows one file names its other files with `include ./rules.pi`. an included file holds
declarations only; the header clauses appear once, in the project file. an included file cannot include
further files. the project's source content is exactly the project file and the files it includes.

### the lock

the first command that resolves inputs writes a lock beside the project file, recording the content
identity of every resolved input, on 0070's terms.

### what goes away

- the separate manifest
- source discovery under `src/`
- standalone mode and the file-relative import resolver. a file with header clauses is a project; a file
  without them is valid only as an include.


## edit, 2026-09-21: after the research

[declaring a project](../research/project-declaration.md) leaves the decision unchanged and is the research
the unresolved section asks for. it adds one requirement: tools that add or update inputs (`pith add`,
`pith update`) have to rewrite the file without disturbing the rest of it. Nix (RFC 193), Zig (PR #14265)
and Go (go.mod) all ran into this. so the parser keeps comments and layout, and
[0080](0080-inputs-are-parameters.md) makes locators literals, which tools can rewrite without evaluating
anything.

## alternatives considered

keeping 0067's directory layout leaves the problem in the context: no file says which source files make up
a module.

one file per module with no `include` would force a large project to split into several modules, changing
its identity and ABI only because its source no longer fits in one file.

letting an included file include others would make the file set a tree to walk. with one level, the project
file lists every file in the project.

## evidence

the witness for [M-17](../planning/milestones.md) is written in this form: a project file with inputs, an
included file, and outputs used by a second project.

- a project whose source content is its file plus its includes gives the same ABI and body digests as the
  same declarations in 0067's layout
- a `.pi` file in the project directory that no include names does not affect any digest
- an included file containing a header clause is refused, naming the clause

## unresolved

the conventional file name is not chosen; `pith.pi` is the placeholder used in examples.

the research note on how other systems split dependencies from build logic (flakes, Zig's `build.zig` and
`build.zig.zon`, Bazel's `MODULE.bazel`, Cargo, CMake, Dhall) is owed and should be written before this
record is accepted.

workspaces, several projects developed together, keep 0067's explicit membership; how a workspace is
spelled in the one-file form is left to the implementation round.
