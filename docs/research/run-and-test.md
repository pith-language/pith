---
schema: design-doc/v1
id: research-run-and-test
title: what run and test execute
summary: how Nix, Cargo, Bazel, Zig, Deno, npm, just, make, Buck2 and Pants decide what a run or test command executes, how they cache test results and force reruns, and how development shells reach editors and direnv
kind: research
status: researching
evidence: preliminary
created: 2026-09-23
updated: 2026-09-23
tags:
  - research
  - cli
  - tooling
relations:
  informed_by:
    - research-tooling
  depends_on:
    - research-method
  supersedes: []
---

# what run and test execute

[0083](../decisions/0083-outputs-and-commands.md) makes `pith run` require an output declared as
`pith.Exec` and `pith test` one declared as `pith.Verdict`, computed in the graph. this note reads how
other tools find what to run, how they cache tests, and how development shells work. sources were read on
2026-09-23.

## Nix

`nix run` looks for `apps.<system>.<name>`, then `packages`, then `legacyPackages`. for a derivation it runs
`<out>/bin/<name>`, where the name comes from `meta.mainProgram`, else `pname`, else the derivation's name,
in one line of `src/nix/app.cc`. `nix run` guesses without warning. the deprecation warning is in
nixpkgs' `lib.getExe` (PR #246386, 2023-08-02): "this behavior is deprecated, because it leads to
surprising errors when the assumption does not hold." an attribute under `apps` must have type `app`;
anywhere else it must be a derivation.

`nix develop` "starts a bash shell that provides an interactive build environment nearly identical to what
Nix would use to build installable", recorded by building a modified derivation. the shell is bash and
inherits every stdenv variable. `nix print-dev-env` writes the environment as bash, with a hardcoded list of
variables it ignores and special merging of `PATH` and `XDG_DATA_DIRS` (#6809, #8253 record its
assumptions). nix#4609 (2021-03-06, open) asks for other shells and fewer variables: "all the extra
variables that get created confuse".

nix-direnv caches the `print-dev-env` output and reloads when `flake.nix` or `flake.lock` change. editors
get the environment by having direnv export it into their own process.

numtide's devshell makes the shell a program: "Devshells can be treated as executable packages",
`nix run '.#devShells.<system>.<myshell>' -- <command>`. its entry point has `--pure`, which re-executes
under `env -i` keeping `HOME` and `PRJ_ROOT`. it still ships `env.bash` and a setup hook, so direnv and
`nix develop` can import the environment into an existing process. `PRJ_ROOT` defaults to the working
directory, because a program in the store has no other way to know where the project is.

`nix flake check` builds the `checks` outputs. a passing check is a store path and is never run again; a
failing one is not recorded; `nix build --rebuild` forces a rerun. Nix does not distinguish a test that
failed from one that could not run.

## Cargo

`cargo run` runs the only binary, or the one named by `default-run`, and otherwise fails listing the
candidates: "`cargo run` could not determine which binary to run ... available binaries: a, b". arguments
after `--` go to the binary. `cargo test` does not cache results. a 2025 proposal to rerun only changed
crates (internals.rust-lang.org, 2025-01-03) met the objection that tests "can read files, access the
internet", and that "Cargo has no knowledge of tests".

## Bazel

"Executable rules define targets that can be invoked by a bazel run command. Test rules are a special kind
of executable rule whose targets can also be invoked by a bazel test command." a test is also runnable, so
it can be run under a debugger with `bazel run`. `bazel run` sets `BUILD_WORKSPACE_DIRECTORY` and
`BUILD_WORKING_DIRECTORY`, and `--script_path` writes a launcher instead of running.

with the default `--cache_test_results=auto`, a test reruns "if Bazel detects changes in the test or its
dependencies, the test is marked as external, multiple test runs were requested with --runs_per_test, or
the test failed": passes are cached, failures are not. `--nocache_test_results` forces reruns,
`--flaky_test_attempts` retries and marks `FLAKY`, the `external` tag exempts one test, and a cached result
prints `(cached)`. the test encyclopedia requires hermetic tests and decides pass or fail by exit code.

## Zig

steps are registered by name in `build.zig`. run steps are cached unless they are marked as having side
effects, and passing arguments after `--` "causes the step to be considered to have side effects, disabling
caching." zig #15635 (2023-05-09): "If zig thinks there was no change to the tests, it won't output any
information ... it could lead to tests going missing, where zig reports they were successfully executed."
the usual workaround is `has_side_effects = true`, which discards the cache.

## string task runners

npm scripts run through `/bin/sh` or `cmd.exe`, so one script means different things per platform. Deno
tasks use their own shell and cache only when `files` are declared, fingerprinting "the command, its
appended arguments, the contents of the matching files, and the values of any listed env vars". just is "a
command runner, not a build system" and warns that positional arguments defeat its typo checks. make will
report "`test' is up to date" if a file named `test` exists, which `.PHONY` patches.

## Buck2 and Pants

Buck2 runs targets with `RunInfo` and tests with `ExternalRunnerTestInfo`; test result caching is opt-in per
rule, and issue #183 (2023-04-19) complains that unchanged tests rerun. Pants caches and memoizes results,
`--force` reruns only the step that executes the test, and `--debug` runs a test interactively.

## where they differ

tools that find what to run by type (Bazel rule kinds, Buck2 providers) have no fallback chains; Nix's
attribute paths and name guess needed flake schemas and a deprecation warning. every tool that caches test
results added a force-rerun, and those that cache silently (Zig) had tests go missing. every tool
that includes arguments in a test's identity (Deno, Bazel reportedly, Zig by disabling the cache) treats changed arguments as a different test.

## result for this project

these results are recorded as edits to [0083](../decisions/0083-outputs-and-commands.md).

lookup by declared type is adopted, as in Bazel and Buck2. when a command gets an output of the wrong type,
the refusal names the type it found and lists the outputs that have the type it needs, the way `cargo run`
lists binaries. a bare `pith run` with no `default` output does the same.

tests are cached the way Bazel caches them by default. a `Passed` verdict is cached, and a `Failed` verdict
is reported and computed again on the next run. a test that could not run is a fault and never a `Failed`
verdict. `pith test --rerun` recomputes only the step that executes the test, as Pants' `--force` does, and
`--runs N` repeats it for flaky tests. every test is printed with whether it ran or came from the cache,
followed by a count, so a test that disappears from the project shows up. arguments that change a test are
part of its key, and `--` arguments are accepted only by `pith run`. the program a verdict is computed from
is itself a `pith.Exec` output or reachable as one, so a test can be started under a debugger.

`pith.Exec` carries its environment as data, and `pith env <name>` prints it for direnv and editors, as
`print-dev-env` and devshell's `env.bash` do. `pith run` passes through a small named set of variables by
default, has a pure mode, runs in the caller's working directory, and sets variables naming the project
root and the directory it was started from. none of these is part of any key.

## sources

- nix run manual: https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-run; `src/nix/app.cc`,
  `src/nix/develop.md`, `src/nix/flake.cc`; issues #4609, #6809, #8253, #12161: https://github.com/NixOS/nix
- nixpkgs PR #246386: https://github.com/NixOS/nixpkgs/pull/246386
- nix-direnv: https://github.com/nix-community/nix-direnv; numtide devshell:
  https://github.com/numtide/devshell; devenv: https://devenv.sh
- cargo run: https://doc.rust-lang.org/cargo/commands/cargo-run.html; internals thread:
  https://internals.rust-lang.org/t/cargo-idea-cli-option-to-only-rerun-tests-in-crates-with-changes/22108
- Bazel rules: https://bazel.build/extending/rules; user manual: https://bazel.build/docs/user-manual; test
  encyclopedia: https://bazel.build/reference/test-encyclopedia; EngFlow, 2023-09-18:
  https://blog.engflow.com/2023/09/18/bazel-testing-tips/
- Zig build system: https://ziglang.org/learn/build-system/; issues #15586, #15635, #23240
- npm run: https://docs.npmjs.com/cli/v11/commands/npm-run; Deno task:
  https://docs.deno.com/runtime/reference/cli/task/; just: https://github.com/casey/just
- Buck2 test execution: https://buck2.build/docs/rule_authors/test_execution/; issue #183; Pants test goal:
  https://www.pantsbuild.org/stable/reference/goals/test
