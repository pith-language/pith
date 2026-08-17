![Pith — a computation kernel for build, package, environment, and system tooling](docs/assets/banner.svg)

Software passes through a chain of tools before it runs. One resolves
dependencies, another builds them, another assembles an environment, another
describes a machine, another deploys it. Each tool rebuilds a version of the
same graph under its own identities, caches, and effect rules, and information
is lost at every handoff. Pith looks for the mechanism those tools could have
shared, and for the point where that mechanism has to stop and hand a domain to
a library.

[Design notebook](docs/index.md) &nbsp;·&nbsp; [Problem](docs/foundation/problem.md)
&nbsp;·&nbsp; [Principles](docs/foundation/principles.md) &nbsp;·&nbsp;
[Milestones](docs/planning/milestones.md) &nbsp;·&nbsp; [Decisions](docs/decisions)

## One graph, and what it lets you skip

Every value is identified by its content and every computation by what it was
derived from, so a second build is a question about identity rather than about
timestamps. Two C sources over a shared header, from
[`crates/xylem/tests/two_source_build.rs`](crates/xylem/tests/two_source_build.rs):

![A cold build plans five action computations, editing one source plans three, nothing changed plans none and is reused, and a new process over the same store plans none and is hydrated.](docs/assets/ledger.svg)

Editing one source leaves the other source's header scan and compile answered
from the durable index; the link runs only because the object it consumes moved.
The last row is the one that needed a database: close the engine, open a new one
over the same store, and the build hydrates from the attempt the first engine
recorded rather than from a copy of it.

## Effects are where the kernel draws its line

Pure computation and external work share one graph without being given the same
semantics. Five sealed categories in
[`pith-core`](crates/pith-core/src/effect.rs) decide what may be cached, what
must be declared, and what the scheduler may do with it:

| | | cacheable | today |
| --- | --- | --- | --- |
| `Pure` | computes from immutable values, terminating by construction | indefinitely | runs |
| `Action` | bounded external work with declared inputs, outputs, platform, and capabilities | when the executor honored the contract | runs, confined |
| `Observation` | reads external state, recording source, revision, and freshness | no | designed |
| `Mutation` | changes external state | no | designed |
| `Opaque` | unmodeled work behind a fixed-output boundary; the escape hatch | no | designed |

The categories are sealed on purpose. Adding one is a kernel change argued in a
decision record, not something a library can reach for.

An action's contract is checked rather than trusted. The Linux executor stages
declared inputs and the toolchain closure into a scratch root, runs the child
under a Landlock ruleset and a seccomp allowlist measured syscall by syscall
from real compilers, and captures only the declared outputs. An undeclared
header is unreachable rather than quietly stale.

## Domains are libraries, ours included

Values, rules, dependencies, effects, identities, content, and provenance belong
to the kernel. Builds, packages, services, and systems do not. A domain library
is two registration calls:

```rust
impl ExampleEngine for Engine {
    fn register_example_domain(&mut self) {
        self.register_action_rule(RenderAction::rule(), RenderAction);
        self.register_rule(RenderRule::rule(), RenderRule);
    }
}
```

Incremental reuse, cross-process hydration, and contract inspection follow from
those two calls and nothing else. `xylem` (builds), `phloem` (packages and
environments), and `stele` (system composition) are clients on exactly these
terms — they take their names from the tissues around a stem's pith — and the
kernel has no built-in notion of a package, service, machine, or deployment.

That claim is held to a test rather than asserted.
[`crates/example-domain`](crates/example-domain) is a library the kernel knows
nothing about, depending on no other domain and named nowhere else in the
workspace, and it collects the same properties the first-party domains are
measured on.

## What runs today

A working prototype, not a tool for general use. There is no source language,
and the command line covers an evaluation stub and content materialization.
What exists is one vertical slice, deep enough that the parts push back:

- typed rule selection, incremental evaluation, and equality-based pruning when a
  recomputed dependency lands on the value it already had
- content-addressed blobs and trees, engine state in SQLite, and two state
  adapters held to each other by a generated conformance suite
- confined local execution on Linux, with an 80-syscall allowlist whose every
  entry was measured from a tool that asked for it
- builds under two real toolchains with discovered header dependencies; package
  resolution, locks, and admitted binary substitution; development environments
  over a lock; and an immutable Linux tree composed from files, users, units,
  and boot configuration

Linux system activation is next, and it waits on a question rather than on code.
An observation has no computation key, so equality pruning has no analogue on an
observation edge and every consumer of one would be permanently non-reusable.
What an observation's identity and freshness are is the next record to write.

[`docs/planning/milestones.md`](docs/planning/milestones.md) is the boundary
between demonstrated and proposed. Each milestone records what was measured,
what it still owes, and which of its own claims later rounds overtook.

## Reading the notebook

Most documents describe a direction rather than released behavior, and their
status separates accepted decisions from proposals, research, and living
references.

| | |
| --- | --- |
| [Problem](docs/foundation/problem.md), [scope](docs/foundation/scope.md) | what this is for, and what it refuses to become |
| [Design overview](docs/design/overview.md) | the four layers and the kernel boundary |
| [Principles](docs/foundation/principles.md) | the constraints the architecture answers to |
| [Research](docs/research/index.md) | Nix, Bazel, Terraform, CUE and others, read closely |
| [Decisions](docs/decisions) | numbered records, each closed by a measurement |

A record here usually opens with a primary source and closes with a test.

## Working in the repository

The development environment is pinned with Nix.

```sh
nix develop
just test
```

`just` lists the available commands. `just check` runs formatting and static
analysis, `just ci` runs the full local suite. The Linux executor and system
fixtures compile to zero tests elsewhere, so a green run on another platform has
not exercised those paths.

Contributions are guided by [CONTRIBUTING](CONTRIBUTING.md), including the
project's authorship and AI-tool terms.

Pith is licensed under [Apache 2.0](./LICENSE). Third-party notices are
collected in [NOTICE](./NOTICE).
