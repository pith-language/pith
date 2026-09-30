---
schema: design-doc/v1
id: decision-0086-names-are-scoped-per-file
title: each file names what it uses, and a name is visible only in the file that uses it
summary: the clauses `use ./x.pi [as n]` and `use <input>` at the head of a file replace `include` and `import`; a project's files are its project file and the files reachable through `use`, found without evaluation; every path is resolved against the project root; `x.name` needs `use x` in the same file, or an input of that name in the project file, and uses do not pass between files; a declaration's identity stays its project and name, so splitting a project across files is still a spelling
kind: decision
status: proposed
created: 2026-09-30
updated: 2026-09-30
tags:
  - modules
  - language
relations:
  informed_by:
    - research-project-declaration
    - research-language-frontend
  depends_on:
    - foundation-principles
    - decision-0023-rule-and-cache-identity
    - decision-0047-the-declaration-table
    - decision-0061-the-declaration-artifact
    - decision-0067-local-module-workspaces
    - decision-0079-a-project-is-one-file
    - decision-0080-inputs-are-parameters
    - decision-0081-paths-are-values
    - decision-0085-typed-literals
  supersedes: []
  amends:
    - decision-0061-the-declaration-artifact
    - decision-0067-local-module-workspaces
    - decision-0079-a-project-is-one-file
---

# each file names what it uses, and a name is visible only in the file that uses it

> amends [0079](0079-a-project-is-one-file.md): `include` and its one-level rule are replaced by `use`; the
> project's source content is its project file and the files its uses reach. the containment boundary of
> 0079's 2026-09-30 edit stands, and a `use` path is further refused inside another project's directory.
> amends [0061](0061-the-declaration-artifact.md): the `import` clause is removed, and imports are scoped to
> the file that writes them, as 0061 already required. amends [0067](0067-local-module-workspaces.md): the
> `import name` clause its source files keep is replaced by `use`.

## context

under 0079 the project file lists its includes, and an included file cannot include further files. every
declaration in any file of the project is visible in every other file: the witness
`a_declaration_in_one_file_is_visible_to_the_rules_in_another` holds that. a reader of one file cannot tell
from it where a name comes from, or which other files it depends on.

inputs reach a file through `import greeting` clauses, which 0061 describes as lexically scoped. the
implementation merges every file's imports into one list for the module (`merge_module_files`), so the
same `import` in two files is refused as a duplicate and an import in one file is visible in all of them.

what the project keeps from 0079 and 0067 is identity: a declaration is identified by its module and
name, file location does not participate (0023, 0047), and the slice that closed 0079 measured that moving
declarations between the project file and an include leaves the ABI unmoved, and that the project's body
digests match the same declarations in 0067's layout.

0067's manifest had a `use alias = subject from path "..."` binding, which 0070 let carry ranges. 0079
removed the manifest and that binding with it, so the keyword is free; the `use` below is a different
clause.

## decision

### use

any file may begin with `use` clauses, before its declarations:

```
use ./model.pi                 -- a file of this project, bound as `model`
use ./c/rules.pi as c_rules    -- bound under a chosen name
use greeting                   -- an input that is a project, bound as `greeting`
use target                     -- an input that is a value, bound as `target`
```

a file's name is the file name without `.pi` unless `as` gives another. a use is refused when the derived
name is not an identifier, when two uses in one file bind one name, or when the name is already taken in
that file by a declaration, a builtin or, in the project file, an input. the refusal names the clash and
suggests `as`.

the project file does not use its own inputs: they are declared there and in scope there. every other file
uses the inputs it needs. `pith` is always in scope and is neither used nor bound.

`include` and `import` are removed.

### the file set

a project's files are its project file and every file reachable from it through `use`. the loader finds
them by reading the `use` clauses at the head of each file, which are literals, without evaluating
anything. a file nothing uses is not part of the project. files may use each other in any direction;
declarations are resolved over the whole project, so a cycle of uses is not an error. no file uses the
project file.

a `use` path is a `Path` literal (0081, [0085](0085-typed-literals.md)) and is resolved against the project
root, as every path in the project is: `./c/rules.pi` names the same file from any file. no path needs
`..`, and 0079's containment boundary applies to it. a `use` path is further refused when it names a file
inside another project's directory, one holding a `pith.pi`, which 0079's boundary allows.

### names

in a file:

- a bare name is a declaration of that file, a builtin, or a value input the file uses or, in the project
  file, declares
- `x.name` needs `use x` in that file, or, in the project file, an input named `x`. it names a declaration
  of the file bound to `x` or an output of the input bound to `x`

a use is visible only in the file that writes it. `model.x` names only declarations written in the file
bound to `model`, never the files or inputs that file uses.

a local shorthand is an ordinary binding: `let greet = rules.greet`, `type Message = model.Message`. there is
no form that imports single names.

### identity

a declaration is identified by its project and its name, as before. a private name is unique within its
project, and a second declaration of it in another file is refused, naming both files. a use binding does
not survive elaboration: `rules.greet` and the same rule reached from any other file elaborate to one
coordinate, so moving a declaration to another file, or using its file under another name, changes no
digest.

## alternatives considered

keeping 0079's include list, with every declaration visible everywhere, leaves each file silent about what
it depends on, and the merged import list keeps 0061's lexical scoping untrue in practice.

a list of named files in the header, `include rules = ./rules.pi` or a block of them, makes the project file
say which files exist but still leaves each file silent about which of them it uses, and it is a second
place a file's dependencies would be written.

a form that imports single names, `use greet from ./rules.pi`, is a second way of binding a name that `let`
and `type` already provide, and a name imported that way no longer shows where it came from.

identifying a private declaration by its file as well as its name would let two files declare the same name.
it would also make moving a declaration between files change its identity and every key that reaches it,
against 0023, 0047 and the measurement that closed 0079.

resolving each path against the file it is written in would make paths in nested files shorter, and would
need `..` to reach a sibling directory, which 0079's containment boundary refuses. it would also give one
file two spellings from two places.

letting header names be visible in every file without a `use` would keep files shorter, and a reader of a
file would again have to open the project file to learn where a qualified name comes from.

## evidence

to be measured when the slice closes:

- the same declarations spelled in one file, and split across files joined by `use`, give the same ABI and
  body digests; moving a declaration from one file to another moves none
- a qualified name whose file has no `use` for its qualifier is refused, naming the missing clause
- a file that no `use` reaches affects no digest
- a name declared in two files of one project is refused, naming both files
- two files that use each other load
- a `use` path through `..`, through a symlink, or into another project's directory is refused
- the local-workspace example rewritten with `use` runs end to end

## unresolved

whether `pith fmt` given a file that is not a project file formats it alone or through the project that uses
it, which the slice that closed 0079 also left open.

whether a file can be used by two projects of one workspace. the boundary above refuses a path into another
project's directory; a file under the workspace root that belongs to no member is not yet addressed.
