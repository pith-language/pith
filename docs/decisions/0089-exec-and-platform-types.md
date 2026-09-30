---
schema: design-doc/v1
id: decision-0089-exec-and-platform-types
title: the command line's types hold paths, arguments, environments and platforms as typed values
summary: pith.Exec is a program, a list of typed arguments and an environment; an argument is text, a project path or a host path; an environment entry sets a variable or builds a search path from a list; pith.Platform is a record of declared sums; ActionSpec's string fields take typed forms, with a named argument sum, a platform requirement of any or exact, and network hosts as URLs without a path; capability names stay open text
kind: decision
status: proposed
created: 2026-09-30
updated: 2026-09-30
tags:
  - cli
  - types
  - actions
relations:
  informed_by:
    - research-run-and-test
    - research-typed-literals
  depends_on:
    - foundation-principles
    - decision-0036-produced-program-as-content
    - decision-0065-entry-evaluation-and-the-cli-query-surface
    - decision-0078-kernel-surface-grows-on-demand
    - decision-0081-paths-are-values
    - decision-0083-outputs-and-commands
    - decision-0084-builtins-and-machine-facts
    - decision-0085-typed-literals
  supersedes: []
  amends:
    - decision-0065-entry-evaluation-and-the-cli-query-surface
    - decision-0081-paths-are-values
    - decision-0083-outputs-and-commands
    - decision-0084-builtins-and-machine-facts
---

# the command line's types hold paths, arguments, environments and platforms as typed values

> amends [0065](0065-entry-evaluation-and-the-cli-query-surface.md) and
> [0083](0083-outputs-and-commands.md): `pith.Exec` changes shape. amends [0081](0081-paths-are-values.md):
> its question of how `ActionSpec`'s string fields become typed is answered. amends
> [0084](0084-builtins-and-machine-facts.md): `pith.Platform` gets its fields.

## context

`pith.Exec` is `{ arguments: List<Text>, program: Text }` (0065), declared in the loader's builtin module and
read by field name in the query layer. 0083's edit makes the environment part of it as data, printed by
`pith env`, and defers a program pith built to the milestone that runs built trees. 0084 puts
`pith.Platform` under `pith.` and gives it no fields.

`ActionSpec` names its toolchain, arguments, input and output paths, environment and network hosts as
strings, and its platform requirement as a sum whose `Exact` case holds an operating system and an
architecture as strings. it checks them with total functions (`is_valid_host_path`, `is_valid_action_path`)
after the fact. 0081 left open how these fields become typed.

a path passed to a program as text loses what the engine knows about it: which file of the project it is,
what content it had, and that `pith explain` should name it. the run-and-test note records `nix
print-dev-env` special-casing `PATH` and `XDG_DATA_DIRS`, because a search path held as text has no defined
way to be combined with another.

## decision

### pith.Exec

```
type Exec = { program: Program, arguments: List<Arg>, env: Env }

sum Program = | installed(HostPath)
sum Arg = | text(Text) | project(Path) | machine(HostPath)

type Env = List<EnvEntry>
sum EnvEntry = | set({ name: Text, value: Arg }) | search({ name: Text, entries: List<Arg> })
```

`installed` names a program on the machine. `pith run` is a caller effect (0083) and takes no grant. a
program pith built is added as a second constructor by the milestone that runs built trees, as 0083 defers;
this record does not anticipate its shape.

`pith run` turns each argument into text only when it starts the program: a `project` path becomes the
location of that file under the project root, a `machine` path its own text. a `search` entry is joined with
the platform's separator at the same point. `pith env` prints the environment the same way. none of this is
part of any key.

an argument's path stays a path until then, so `pith explain` names the file a program was given.

### pith.Platform

```
type Platform = { arch: Arch, os: Os, abi: Abi }
```

`Arch`, `Os` and `Abi` are declared sums whose constructors are fixed per language version. the first set is
the platforms the executor supports; a constructor is added when an executor for it exists.
`pith.native` is the one value pith reads from the machine, and 0084's rule that it appears only as an input
default stands.

details beyond these three fields, such as CPU features or a libc version, are types of the libraries that
need them, built around `pith.Platform`. a tool's triple string is produced by the library that runs the tool,
at that boundary.

### ActionSpec

`ActionSpec`'s fields take these types:

```
type ActionSpec = {
  executable: ActionProgram,        -- | host(HostPath) | content(Blob), as today
  toolchain: List<HostPath>,
  arguments: List<ActionArg>,
  inputs: List<RelPath>,
  outputs: List<RelPath>,
  environment: List<EnvEntry>,
  platform: PlatformRequirement,    -- | any | exact(pith.Platform)
  network: List<pith.Url>,
  capabilities: List<Capability>,
}

sum ActionArg = | text(Text) | machine(HostPath) | input(RelPath) | output(RelPath)
```

the sketch shows field types, not the full record: inputs and outputs keep the other fields they have
today. in the environment, a search path may also list `input` and `output` locations. a network URL is
refused at load if it has a path, a query or a fragment, and one without a port uses its scheme's default.

the checks `is_valid_host_path` and `is_valid_action_path` become the checks of `host"..."` and `rel"..."`
under 0085, defined once and used by both.

capability names and scopes stay text. the kernel's list of capabilities is open so that libraries can add
their own without changing the kernel, and a fixed type would close it.

### where these types live

0078 lets the kernel gain an interface only when a domain being written needs it. here the consumers are
pith's own commands and executor: `pith run`, `pith env` and `pith test` read these types, and the executor
places and runs actions by them. none of their fields carries meaning that only a domain library would need.

## alternatives considered

keeping arguments and environments as text, with paths and search paths spelled into strings, is what Nix
does with string context: the type rides inside the text, and a way out and a rule for leaks follow. Buck2's
`cmd_args`, a list of text and artifacts, is the typed form this record takes.

a platform as a triple string, as LLVM, Rust and Zig use, has needed normalization rules, renaming and a
growing grammar in each of them ([typed literals](../research/typed-literals.md)). a record of declared sums
is checked by the type checker, and a misspelled constructor is refused at load.

## evidence

to be measured in the slice that builds outputs and commands:

- an output of the new `pith.Exec` shape runs through `pith run` with a `project` argument, and `pith explain`
  names that file
- two environments whose `PATH` entries are `search` lists combine into one list, and `pith env` prints the
  joined value
- an `ActionSpec` argument that names an undeclared output location is refused before planning
- a malformed host path in a toolchain is refused by the same check `host"..."` uses, with the same code
- a network host written with a path is refused, naming the host

## unresolved

how a `project` path argument is presented when the program must not see the project directory, such as a
test run under the executor's confinement. the executor stages inputs for actions; whether `pith run` stages
them too is open.

the first constructor sets of `Arch`, `Os` and `Abi`, fixed when the slice that builds machine facts lists the
executor's platforms.

whether a platform can be given on the command line, which 0085 leaves open.
