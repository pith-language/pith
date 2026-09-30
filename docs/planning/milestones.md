---
schema: design-doc/v1
id: planning-milestones
title: milestones
summary: what comes next, what is parked, what the completed milestones still owe, and how the order got here
kind: planning
status: draft
created: 2026-03-23
updated: 2026-09-30
tags:
  - planning
  - milestones
relations:
  informed_by:
    - foundation-scope
  depends_on:
    - planning-open-questions
    - planning-measured
  supersedes: []
---

# milestones

the goal for now is to finish pith's design with one end-to-end case: a domain written outside this
repository is loaded and runs its rules, including actions. after that, pith is marked experimental
([0077](../decisions/0077-pre-release-pith-is-experimental.md)) and interfaces are added as the first
outside domains need them ([0078](../decisions/0078-kernel-surface-grows-on-demand.md)).

> edit, 2026-09-30: named so it stays a decision rather than an omission: between experimental and
> usable by a stranger sit the install story, the outsider-facing documentation, and the error messages
> a person meets before any design does. [usability](../requirements/usability.md) holds the
> requirements; none of it is scheduled, under 0078's rule that the first outside domains set the
> order, and the trigger to plan it is the first such domain arriving.

## the order

M-17 is next, then M-16. nothing else is scheduled. the rest of M-14, and M-15, M-5b, M-6 and M-7, are
parked and have no order among themselves.

> edit, 2026-09-29: M-17 was added ahead of M-16, because M-16's witness is written in the project form
> M-17 introduces.

M-1 to M-5a and M-8 to M-13 are complete, and M-14 is complete as far as local workspaces, the acquisition
boundary and the first routing checks. the evidence is in
[what the completed milestones measured](measured.md).

labels identify milestones and do not encode their order, which is stated in this section.

> edit, 2026-09-30: the order carries no dates either, by the same discipline, and two size facts are
> worth writing down while both milestones are still ahead. M-17 and M-16 are each larger than any
> completed milestone: M-17's six steps are each milestone-sized against a history in which M-8 was one
> mechanism. and the completed record is the only honest forecast there is — M-1 to M-13 closed between
> late february and late august, with the M-14 slices running on into september — which puts
> experimental months away, not weeks. the convention that follows: when each of the two opens, state an
> appetite, how much time it is worth, and descope against the appetite rather than a deadline, so the
> calendar serves the design instead of the reverse.

## M-17: the explicit project model

a project is one file with its inputs, declarations and outputs; inputs are parameters with defaults;
paths are values; rules are called by name and functions are values; commands need an output of a declared
type; builtins are the language's own vocabulary, and facts about the machine enter only through inputs.

the records:

- [0079](../decisions/0079-a-project-is-one-file.md): one project file, `include`, the lock beside it
- [0080](../decisions/0080-inputs-are-parameters.md): inputs as parameters, `with`, grants on inputs
- [0081](../decisions/0081-paths-are-values.md): `Path`, `RelPath` and `HostPath`
- [0082](../decisions/0082-rules-are-called-by-name-and-functions-are-values.md): calls by name and
  function values, replacing selection by type
- [0083](../decisions/0083-outputs-and-commands.md): outputs, `pith eval`, `pith run`, `pith test`, test
  caching and `pith env`
- [0084](../decisions/0084-builtins-and-machine-facts.md): builtins and machine facts

the evidence behind them is in [declaring a project](../research/project-declaration.md),
[passing behaviour](../research/passing-behaviour.md) and [what run and test execute](../research/run-and-test.md).

*measured claim:* the local-workspace example rewritten as two project files, one using the other's
outputs and passing it an argument, gives the same results as today's form. an edit to one source file
recomputes exactly the computations that read it, a lambda with a bound value is cached and recomputed when
the value changes, and each refusal the records list is a test.

the likely order of work inside it:

1. the project file grammar, `include`, and the lock, replacing the manifest and `src/` loading
2. path values and their admission into the content store
3. calls by name, function values and `map`, and removing selection by type from the engine and the CLI
4. inputs as parameters, `with`, the replacement check, and grants on inputs
5. outputs and the commands, including test caching and `pith env`
6. the builtin rules and machine facts

> edit, 2026-09-30: M-17 closes the way M-14 does, slice by slice. each step above is a slice with its
> own measured claim, recorded in [measured](measured.md) when it closes, and the milestone's claim is
> the six together. the reason is size: against a history in which M-8 was one mechanism, each of the
> six steps is milestone-sized on its own, and a milestone that closes nothing for months is a drift a
> solo project has no instrument to notice.
>
> edit, 2026-09-30: slice 1 is complete. the project file grammar, `include`, and the lock replace the
> manifest and `src/` loading and the standalone mode, and [0079](../decisions/0079-a-project-is-one-file.md)
> is accepted against the slice's measurements in
> [measured](measured.md#m-17-the-explicit-project-model-the-project-file-slice). slice 2, path values
> and their admission into the content store, is next.
>
> step 3 is the riskiest item in the plan: it removes selection by type from the engine and the CLI,
> the mechanism [0015](../decisions/0015-interface-rule-selection.md) marked accepted and four domains
> exercised. the parity fixtures are the gate — the declaration tables, the rule revisions and the
> elaborated digests must agree across the change before step 4 starts, the same agreement that held
> through the M-14 slices.
>
> edit, 2026-09-30: [0085](../decisions/0085-typed-literals.md) and
> [0086](../decisions/0086-names-are-scoped-per-file.md) are proposed against the remaining slices. slice 2
> builds 0085's token rule and tag set with the path values it already owns, and respells the header. a
> slice 2b follows it for 0086: `use` replaces `include` and `import`, visibility becomes per file, and the
> project's file set becomes the closure of its uses. both keep the parity fixtures' digests, so they can
> land before step 3.
>
> edit, 2026-09-30: [0087](../decisions/0087-function-types-carry-effects.md) lands with step 3,
> with function values. the second edit to [0080](../decisions/0080-inputs-are-parameters.md), on the lock
> and one version per subject, lands with step 4, and [0088](../decisions/0088-outputs-are-public.md)
> and [0089](../decisions/0089-exec-and-platform-types.md) land with step 5.

## M-16: an external domain runs

> edit, 2026-09-29: M-16 now follows M-17, and its witness is written as a project file. the `host` clause
> and grants move into that file (edits on 0071 and 0072).

a module outside the workspace declares its types and rules in `.pi`, ships its host bodies as a
WebAssembly component, is loaded by path, and runs.

the records:

- [0071](../decisions/0071-host-bodies-bind-through-host-adapters.md): host adapters, the manifest's
  `host` clause, request lists, and revisions derived from the artifact
- [0072](../decisions/0072-action-plans-are-authorized-against-consumer-grants.md): grants written by the
  consuming project, replacing `AllowAllActions`
- [0073](../decisions/0073-the-component-interface-is-generated-from-declarations.md): the WIT world
  generated from declarations, the type mapping, and checking of returned values
- [0074](../decisions/0074-the-component-step-protocol-is-wasi-async.md): the step protocol on WASI 0.3
  async
- [0075](../decisions/0075-components-run-in-a-confined-worker.md): the confined worker and Wasmtime
  configuration
- [0076](../decisions/0076-component-execution-is-deterministic-and-bounded.md): determinism, bounds, and
  faults that are never cached

[the research note](../research/wasm-components.md) holds the evidence they rest on.

*measured claim:* `example-domain` rewritten as `.pi` plus a component, outside the workspace, gives the
same contract-test results as the rust crate and plans an `ActionSpec` with the same digest. its action is
reused on a second run and explained after an edit, refused without a grant, and run confined with one.

three costs are measured, each against a target stated before measuring: instantiating a component per
computation, one step across the worker boundary, and a batch of requests compared with the same requests
one at a time.

M-16 uses only the module system that exists now, path dependencies and workspaces, and none of the parked
M-14 work.

the likely order of work inside it:

> edit, 2026-09-30: a spike now opens the milestone, for the reason the frontend spike set before
> M-10: the research note reads the component model as of one day, and the step protocol on WASI 0.3
> async is the assumption every later step builds on.

1. a throwaway spike: one hand-written component driven through wasmtime on WASI 0.3 async, taking one
   step across the boundary and returning, then discarded. it answers in code what the research note
   can only answer in reading, before the adapter boundary and the worker are built against it
2. the host adapter boundary and the rust adapter, with `bind_module` binding through it
3. grants and the refusal of ungranted plans, which also applies to represented bodies and in-tree crates
4. WIT generation and the type mapping, tested against the corpus's declarations
5. the worker process and its measured seccomp allowlist
6. the component adapter, the step protocol and the witness
7. the three measurements, the fuzzing harness, and the experimental notice from 0077

## parked

the milestones below keep their labels and their text, and can be taken up again after M-16. once host
bodies can be components, the domain milestones among them can be written as modules the same way an
outside domain would be.

`xylem`, `phloem` and `stele` stay in the tree as kernel witnesses and keep their tests. they get no new
features while their milestones are parked.

### the rest of M-14: the module system

the parts still to do are the registry round trip, transitive resolution over a scoped universe, the Git,
archive and live-path adapters, compatibility reports and the publication gate, the CLI commands that
need a lock, and the registry security work. the designs stay in
[module distribution](modules/distribution.md), [the module registry](modules/registry.md) and
[registry integration](modules/registry-integration.md), and
[0069](../decisions/0069-module-authority-is-consumer-configuration.md) and
[0070](../decisions/0070-module-acquisition-resolution-and-replay.md) remain the records.

`pith-query` still elaborates a manifest program directly, so no person-facing command reuses a frontend
computation. routing it through the workspace projection onto the frontend graph tier is the first thing
to do when this resumes.

*measured claim:* publish `example-domain` to a temporary registry and get byte-identical elaborated IR
compared with the local-path entry, plus the two configuration refusals.

OCI is the likely transport for components and for modules that carry them, since the wasm ecosystem has
moved to it (see the research note).

### M-15: the generic builder

a peer domain library in the shape of `stdenv.mkDerivation`, authored in `.pi`, with host bodies as
components where it needs them. 0045's shape, a package's build running as one pure rule over xylem's
compile and link entries file by file, proves the identity, lock and substitution machinery; it is not a
packaging design.

`Opaque`'s step protocol lands here, because a foreign build is the caller that needs it. `Opaque`
currently consists of `pub struct Opaque;` and `effect_category!(Opaque, false)`. a foreign build can run
as a plain `Action`, but an action's contract claims what happened, and for a foreign build that claim is
false, so its provenance would claim more than happened.

two things are prerequisites: M-8's backstop, because a configure script runs hundreds of small probe
programs and a hung one is otherwise unbounded, and the allowlist widening 0028 predicted and has measured
twice.

> edit, 2026-09-29: notes from the discussion of packaging toolchains with pith, for when this resumes.
>
> - packaging toolchains, recipes, the bootstrap seed and the choice of install location are domain work.
>   the kernel is asked for two things, under 0078 when a domain needs them: action inputs and outputs at
>   declared absolute paths inside a private mount namespace, and fetching allowed only with a declared
>   output digest the engine checks.
> - a configure-and-make build sandboxed over declared trees has an accurate contract, so it is a coarse
>   `Action`, not an `Opaque`. 0032's rule that one action is one tool invocation assumed confinement by
>   host paths and needs amending. `Opaque` stays for work that cannot be sandboxed.
> - 0019 says an `Opaque` is cached like a Nix derivation, while `effect.rs` has `CACHEABLE_AS_RESULT =
>   false`; the record that builds `Opaque` settles which.
> - with toolchains as content, xylem no longer needs `nix path-info` at run time; importing a Nix closure
>   becomes one way to obtain a toolchain tree, as 0020 intends.

### M-5b: Linux system activation

install a composed artifact onto a running machine and switch to it. this needs `Mutation`, and uses
observations as M-9 designed them. the costs are known: new variants in the step and resumption
vocabulary, and a `computations` table that stores its binary as a nullable digest-column pair and needs a
shape for five categories. under 0048 no encoding version moves for it; the pre-release database is
discarded.

### M-6: deployment

observe one machine, derive a plan, apply it, confirm the result, and return to an earlier realization.
secrets use references resolved at the target.

this is blocked by a contradiction in the foundation. S-4, S-5 and S-6 want a derived operation sequence
with destructive effects, temporary states, preconditions, invariants, compatibility windows, completion
evidence and rollback limits. the graph's only sequencing construct is data dependency: `Need` orders a
child because its value is required, and `NeedAll` declares a batch independent. neither says "this must
happen before that, and if it fails, undo it". [scope](../foundation/scope.md) says the project "is also
not an ordered task runner", which is why no ordering primitive exists.

the first work is a research round and a record deciding whether a precondition and a rollback limit can
be declared inputs and derived values, keeping scope intact, or whether scope's sentence needs amending.
systemd's distinction between ordering and requirement dependencies is the closest primary source, with
Terraform's plan graph and Kubernetes' level-triggered reconciliation as the alternatives. that round
depends on nothing else here and can run at any time.

> edit, 2026-09-30: that round is the one parked item with a standing invitation rather than a gate.
> it is the cooldown task: cheap, independent of the sequence, and it settles a foundation
> contradiction before any deployment work is built on it. it runs in a gap between M-17's slices or
> after M-16, whenever a break from either is wanted.

### M-7: broader execution

remote execution, additional operating systems, multi-machine placement, continuous reconciliation, and
richer transition protocols.

## what the completed milestones still owe

the evidence is in [measured](measured.md); this section lists the remaining work.

M-1 owes nothing. operational support for `Observation` landed in M-9; `Mutation` and `Opaque` wait on
M-5b and M-15.

M-2 owed timeouts and partial cancellation. timeouts are done in M-8 and
[0059](../decisions/0059-a-caller-declared-run-bound.md). partial cancellation is still open, as 0059's
unresolved section says, and it is what keeps 0022 from being accepted.

M-3 owes nothing it named. what remains is scale: the fixture is a handful of files, not a small project.
multi-language targets and a check concept distinct from a test have no milestone, and M-7 holds remote
execution.

M-4 owes nothing, after the rounds that followed it: 0044, 0045, 0046, 0053 and 0054.

M-5a owes nothing.

M-8 owes nothing it named. 0059's unresolved section lists what sits beside it: partial cancellation, a
bound and a cancel signal in one run, the rlimit half of the resource bound, and whether the pure-only
entry point should carry a bound now that represented bodies run on it.

M-10 owes nothing it named. the remaining kernel-change items keep their owners.

M-11 owed the text-splitting constructor both waiting bodies needed, which 0064 supplied as `TextBreak`
and `TextJoin`.

M-13 left entry arguments open: an entry is a name bound to a request with no parameters, so
`pith run test --filter foo` has nowhere to land. an argument is an input and moves the computation key,
so it needs its own record.

## the completed milestones

M-1, the semantic prototype. M-2, the action prototype. M-3, the first build library, `xylem`. M-4, the
package and environment libraries, `phloem`. M-5a, Linux system composition, `stele`.

M-8, the backstop limit: a caller-declared run bound
([0059](../decisions/0059-a-caller-declared-run-bound.md)).

M-9, observation identity and freshness
([0060](../decisions/0060-observation-identity-and-freshness.md)). one thin file-mtime observation
shipped as the prototype, so the step, the async adapter boundary, the durable record and freshness
admission were exercised.

M-10, the declaration artifact ([0061](../decisions/0061-the-declaration-artifact.md)): the `.pi`
declaration grammar, the loader, and the ABI and revision digests. the four crates' live tables and
their `.pi` counterparts agree digest for digest.

M-11, the IR constructor set ([0062](../decisions/0062-the-ir-constructor-set.md)), with the interpreter
that followed it. every corpus rule body that could be expressed was expressed.

M-12, the elaborator and the frontend graph tier
([0063](../decisions/0063-the-frontend-graph-tier.md)). editing a body in A leaves `bodies-of(B)`'s key
byte-identical and its lookup reusable.

M-13, the surface notation and the CLI ([0064](../decisions/0064-text-breaking.md),
[0065](../decisions/0065-entry-evaluation-and-the-cli-query-surface.md),
[0066](../decisions/0066-frontend-diagnostics-carry-source-identity.md)). `pith check`, entries,
`pith run` and `pith explain` exist, and `example-domain` as one `.pi` file gives byte-identical interface
encodings.

M-14's first slices: local module workspaces
([0067](../decisions/0067-local-module-workspaces.md)), the acquisition boundary, and the first routing
checks, with the published-surface probes of
[0068](../decisions/0068-published-surfaces-are-context-bound.md).

## how the order got here

the order has changed three times. each change is kept here with the reason given at the time.

### domains first, until M-5a

the first sequence built domain libraries out to deployment before anything read them. M-1 gave the
reason: "the first implementation should test the kernel through real domain libraries instead of
polishing syntax around an unproven engine." M-4 made that testable, and M-5a answered it: `stele` declared
twelve types using only constructors that already existed and needed no engine, core or encoding change.

### the frontend before more domains, from 2026-08-21

after M-5a the open questions were no longer about the kernel. `pith-cli` was 240 lines and depended on
none of `xylem`, `phloem` or `stele`, so the three libraries, about 23,600 lines, were reachable only from
their own tests, and `explain_invalidation` was implemented in the engine, both state adapters and the
conformance suite and reachable from no command.

the order was changed to put the frontend ahead of further domain libraries, as M-10 to M-14. three
mechanisms that had been deferred from milestone to milestone got owners: the backstop limit (M-8),
`Opaque`'s step protocol (M-15), and the read-only state adapter the CLI needed (M-13). the observation
record moved ahead of the IR constructor set as M-9, because a variant added to `PureStep` after the IR
encoding was fixed would have changed every represented body's digest.

before M-10, a spike tested the frontend architecture's latency assumption: that re-elaborating an edited
module is fast enough without an in-process incremental layer. it used two hundred generated modules, a
throwaway parser and elaborator, and the 50 ms keystroke target. the edited module's path measured
105.6 microseconds at p50 and 117.3 at p99, and the whole set elaborated cold in 21.9 ms. the number is
recorded in [the frontend architecture](frontend/architecture.md) and the code was discarded.

the labels were not renumbered, because records cite `M-5b`, `M-6` and `M-7` by name and would have had to
be edited to express an order.

### finishing the design, from 2026-09-29

the order after M-13 went on to the rest of the module system and then to more first-party libraries:
the generic builder, system activation, deployment. none of these is needed to show an outside
domain running, and each would have added first-party code that an outside domain could not match, since
a host body could only be rust compiled into the binary. M-16 removes that restriction, and the rest is
parked until it is done.
