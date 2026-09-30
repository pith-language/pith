---
schema: design-doc/v1
id: decision-0088-outputs-are-public
title: outputs are declared in the project file and are all that another project or the command line can reach
summary: only the project file declares outputs, as values, rules or types; an output's signature mentions only output types, input types and pith's own; the ABI is computed from the outputs; another project reaches only outputs; outputs are grouped in records, and command-line paths descend into them; `pith test` reports every verdict under the outputs it is given; only the root's outputs run or are tested; an output and an input may not share a name
kind: decision
status: proposed
created: 2026-09-30
updated: 2026-09-30
tags:
  - language
  - modules
  - cli
relations:
  informed_by:
    - research-run-and-test
    - research-language-frontend
  depends_on:
    - foundation-principles
    - decision-0061-the-declaration-artifact
    - decision-0065-entry-evaluation-and-the-cli-query-surface
    - decision-0068-published-surfaces-are-context-bound
    - decision-0079-a-project-is-one-file
    - decision-0082-rules-are-called-by-name-and-functions-are-values
    - decision-0083-outputs-and-commands
    - decision-0086-names-are-scoped-per-file
  supersedes: []
  amends:
    - decision-0061-the-declaration-artifact
    - decision-0079-a-project-is-one-file
    - decision-0082-rules-are-called-by-name-and-functions-are-values
    - decision-0083-outputs-and-commands
---

# outputs are declared in the project file and are all that another project or the command line can reach

> amends [0083](0083-outputs-and-commands.md): outputs are declared only in the project file, as values,
> rules or types, and command paths descend into records. amends [0079](0079-a-project-is-one-file.md),
> which lists outputs among the declarations any file may hold. amends
> [0082](0082-rules-are-called-by-name-and-functions-are-values.md): a rule is reached from another project
> only through an output. amends [0061](0061-the-declaration-artifact.md): the ABI is computed from outputs.

## context

under 0079 and 0083 an output is a declaration like any other, so it can sit in any file of the project.
under [0086](0086-names-are-scoped-per-file.md) every name a file sees is written in that file, but the names
the project shows to others would still be spread over its files.

the frontend's surface notation says every declaration is public and there is no export list. its reason
was selection by type: a peer declaring the same interface reached the same bucket, so privacy would claim a
boundary the engine did not keep. 0082 removed selection, and the reason went with it.

0083 lets another project name an input's outputs, `greetings.standard`, and says nothing about rules and
types reached across projects. 0082 calls `greetings.message(g)`, a rule of an input, directly.

the surface notation also records that an entry may be run only from the root module, because a dependency
that could contribute something to run is npm's `postinstall`. 0083 does not carry that rule forward.

## decision

### where outputs are declared

only the project file declares outputs. an `output` in any other file is refused, naming the project file.

### the three forms

```
output objects : List<cpackage.Object> = cpackage.build(sources.all)
output rule build = steps.build
output rule greet(who: Text) -> Text = { concat("hello ", who) }
output type Source = model.Source
```

a value output is declared with its type. an output rule is either a rule declared in place or a rule of a
used file published under a name; either way its signature is written in a rule declaration. an output type
is a type declared in place or a type of a used file.

### what an output may mention

an output's type, or an output rule's signature, may mention only output types of the same project, types
reached through its inputs, and `pith`'s own types. a signature that mentions a private type is refused,
naming the type, so another project can always name every type it receives.

### what other projects reach

`x.name` for an input `x` names an output of `x`. rules, types and values of `x` that are not outputs are not
reachable, and naming one is refused as not an output. calling an output rule is the same computation as
calling the rule by name inside its own project: it has the same key.

the project's ABI keeps 0061's items 1 and 3, item 3 as 0067 amended it. items 2 and 4 are computed from
the outputs: each output's name, form and type, and the category and interface pairs they provide. a
private declaration changes no ABI digest; its body digest still reaches every key that depends on it.

### contracts between projects

a project that wants other projects to provide something publishes a type, and a project provides it by
declaring an output of that type:

```
-- devshell
output type Shells = shells.Shells

-- an application, with `use ./dev.pi`
output shells : devshell.Shells = dev.all

-- a tool built over several applications
output all : List<devshell.Shells> = [app.shells, site.dev_shells]
```

the check that the application provides the contract is the type check of its own output; the tool names
each project's output explicitly, so projects do not have to agree on output names. no command searches for a
project by the type of its outputs; `pith explore` lists outputs with their types for a person to read.

### records and paths

outputs that belong together are one output of record type:

```
output shells : { default: pith.Exec, rust: pith.Exec } = { default: env.base, rust: env.rust }
output tests : checks.Tests = checks.all
```

a command's argument is a path: an output name, or an input name followed by one of that input's outputs,
then any record fields: `shells.rust`, `c.source`. an output may not share a name with an input of the same
project, so the first segment always means one thing.

### commands

| command | accepts | notes |
| --- | --- | --- |
| `pith eval <path>` | any path, including an input's outputs | as 0083 |
| `pith explain <path>` | any path | as 0083 |
| `pith run [path] [-- args]` | a path to a `pith.Exec` among the root's outputs | a bare `pith run` uses the output `default` |
| `pith test [path]` | the root's outputs | reports every `pith.Verdict` under the path, by its path |
| `pith env <path>` | a path to a `pith.Exec` | as 0083 |

`pith test` with no path runs every root output whose type is a verdict or a record whose fields are, at
every depth, verdicts. given a path, it walks the record at that path and reports the verdicts it holds,
skipping fields of other types. each verdict is reported by its full path, `tests.build`, with whether it
ran or came from the cache, as 0083's edit requires.

`pith run` and `pith test` accept only the root's own outputs. a dependency's outputs run only when the root
publishes them itself, `output tests : app.Tests = app.tests`, so what can run is written in the root
project file.

## alternatives considered

outputs in any file, as 0079 and 0083 allow, spread what a project shows over its files and let a public
name clash be caused by a file the reader of the project file has not opened.

an export list beside the outputs would be a second way to make something reachable. a rule is a value
under 0082, so publishing it is an output.

a separate construct for contracts, a named set of outputs a project claims to provide, would compare
projects by a second rule beside the type checker, and would need output names to be agreed between
projects. an output's type already says what it provides, and the consumer names the output it uses.

letting commands run a dependency's outputs directly would let a dependency contribute something that runs,
which the surface notation refuses for entries.

## evidence

to be measured in the slice that builds outputs:

- an `output` in a used file is refused, naming the project file
- an output signature mentioning a private type is refused, naming it
- naming an input's private rule from another project is refused as not an output
- calling an output rule from another project has the same computation key as calling the rule inside its
  own project
- moving a private declaration between files, or renaming it, changes no ABI digest
- `pith test` reports each verdict of a record output by path, and a verdict removed from the record
  disappears from the count
- `pith run dep.x` is refused, and runs once the root publishes it
- an output named like an input is refused

## unresolved

whether an output rule can take arguments from the command line. 0083 leaves arguments after `--` to the
program; an output rule would need its parameters written as literals, which 0085's `--input` rule could
cover.

record fields are fixed, so a set of tests generated from a list of files cannot be a record. either a
builtin keyed collection is added, or such sets are lists of `{ name: Text, verdict: pith.Verdict }` that
`pith test` also walks. the second hard-codes a record shape in the command.
