---
schema: design-doc/v1
id: decision-0083-outputs-and-commands
title: an output is a named value other projects and the cli can use, and each command needs an output of one declared type
summary: output replaces entry and can be referenced from other projects; pith eval shows any output, pith run needs a pith.Exec and starts it after evaluation, pith test needs a pith.Verdict computed in the graph; no command converts a value; arguments after -- go to the program; a bare pith run uses the output named default
kind: decision
status: proposed
created: 2026-09-21
updated: 2026-09-21
tags:
  - cli
  - language
  - tooling
relations:
  informed_by:
    - research-run-and-test
    - research-tooling
  depends_on:
    - planning-cli-surface
    - decision-0065-entry-evaluation-and-the-cli-query-surface
    - decision-0079-a-project-is-one-file
    - decision-0082-rules-are-called-by-name-and-functions-are-values
  supersedes: []
  amends:
    - decision-0065-entry-evaluation-and-the-cli-query-surface
---

# an output is a named value other projects and the cli can use, and each command needs an output of one declared type

> edit, 2026-09-21: after [what run and test execute](../research/run-and-test.md), test caching, reruns, interactive tests, the environment of `pith.Exec`, and what `pith run` passes through are specified. see the edit section.

> amends [0065](0065-entry-evaluation-and-the-cli-query-surface.md): `entry` becomes `output`, `pith run`
> starts a program instead of showing a value, `pith exec` is removed, and `pith eval` and `pith test` are
> added.

## context

under 0065 an entry is a name bound to a request, evaluated as a synthetic represented pure rule, and only
the CLI can name it. `pith run` evaluates and shows the value, and `pith exec` requires the builtin
`pith.Exec` and replaces the process with it.

a first version of the new model let a command convert a value it was given, by finding a rule from the
value's type to the type the command needs. that is selection by type again, and
[0082](0082-rules-are-called-by-name-and-functions-are-values.md) removes it.

## decision

### outputs

`output name : Type = expression` declares a named value. it computes nothing until something asks for it.
an output differs from a `let` in that the CLI and other projects can name it, and it is a root in the
graph: evaluating it gets a computation key, a cached result, provenance and an explanation. two projects
using the same output of a third ask for the same computation.

### commands

| command | needs | does |
| --- | --- | --- |
| `pith eval <name>` | any output | evaluates it and prints the value |
| `pith run <name> [-- args]` | an output declared as `pith.Exec` | evaluates it, then starts the program with `args` appended |
| `pith test [name]` | an output declared as `pith.Verdict` | evaluates it and reports; with no name, every such output |
| `pith explain <name>` | any output | says what was computed, what was reused, and why |
| `pith check` | nothing | loads and type-checks the project |

a command given an output of another type refuses and names the type it found and the type it needs. no
command looks for a conversion.

`pith run` starts the program after evaluation, as a caller effect: it sees the terminal and the project,
and it is never cached. `pith test`'s verdict is computed inside the graph, where running a test binary is an
action, so an unchanged test is not run again.

arguments after `--` go to the program and do not move any computation key.

a bare `pith run` uses the output named `default`.

an input's outputs are named through the input: `pith eval greetings.standard`.

### the two types

- `pith.Exec` is a program and its arguments, as in 0065. running something pith built will need it to
  carry the closure and environment as well; that waits on the milestone that runs built trees.
- `pith.Verdict` is a sum, `Passed | Failed(Text)`.

### what this replaces

`entry`, `pith exec`, and the meaning of `pith run` as evaluate-and-show. a development shell needs no
command of its own: it is a program, so a library that builds one provides a rule producing a `pith.Exec`,
and `pith run dev` enters it.


## edit, 2026-09-21: after the research

the research adds the following to the decision above, whose original text is kept, and changes one point:
arguments after `--` are accepted only by `pith run`.

tests:

- a `Passed` verdict is cached. a `Failed` verdict is reported and computed again on the next run, as Bazel
  does by default. a test that could not run, because it crashed the executor, timed out or was refused by
  the sandbox, is a fault and never a `Failed` verdict.
- `pith test --rerun` recomputes only the step that executes the test, and `--runs N` repeats it, for flaky
  tests.
- every test is printed with whether it ran or came from the cache, together with a count, so a test that
  disappears from the project is visible (zig #15635).
- arguments that change what a test does are part of its key. `pith test` takes no `--` arguments; a
  filter, if one is added, is an input to the verdict.
- the program a verdict is computed from is reachable as a `pith.Exec` output, so `pith run` can start
  a test under a debugger, as Bazel lets `bazel run` start any test.

running:

- `pith.Exec` carries its environment as data alongside the program. `pith env <name>` prints it for
  direnv and editors, which import an environment into a process that already exists; numtide's devshell
  ships `env.bash` for the same reason.
- `pith run` passes through a small named set of variables from the caller by default, has a pure mode
  that passes through only those it must, and runs in the caller's working directory. it sets variables
  naming the project root and the directory it was started from, as Bazel's `BUILD_WORKSPACE_DIRECTORY`
  and devshell's `PRJ_ROOT` do. none of this is part of any key.
- a refusal lists the outputs that have the type the command needs, as `cargo run` lists binaries, and so
  does a bare `pith run` when there is no `default`.

## alternatives considered

converting a value to the needed type by rule selection was the first version. it is selection by type,
which 0082 removes.

keeping `pith run` as evaluate-and-show and `pith exec` for programs would keep today's names. `run` is the
name most people try first for starting a program, as in `nix run` and `cargo run`.

## evidence

- `pith run` on an output that is not a `pith.Exec` refuses with the diagnostic above
- `pith test` on an unchanged project reports every verdict without running a test action
- an output of one project is evaluated from a second project and the result is reused between them

## unresolved

`pith explore` should list every output with its type and the commands it can be given; the listing is
derived from declared types and does not need any configuration.

remote references, such as `pith run git+https://...#hello`, are left for later.

[the cli surface](../planning/cli-surface.md) needs its tables updated to match.
