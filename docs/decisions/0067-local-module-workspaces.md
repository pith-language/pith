---
schema: design-doc/v1
id: decision-0067-local-module-workspaces
title: a local module is a declared subject with a manifest, a src tree, and module-local import bindings, loaded from an explicit root without a solver
summary: module.pi declares a domain/name subject and path dependencies, a module owns the .pi files under its src recursively, each module binds imports through its own use clauses, and the ABI names imported subjects rather than local binding names, so an alias edit is invisible to every digest
kind: decision
status: accepted
created: 2026-09-06
updated: 2026-09-30
tags:
  - modules
  - language
  - identity
  - workspaces
  - manifests
relations:
  informed_by:
    - planning-modules-workspaces
    - planning-modules-system
  depends_on:
    - decision-0047-the-declaration-table
    - decision-0048-pre-release-version-pinning
    - decision-0061-the-declaration-artifact
    - decision-0063-the-frontend-graph-tier
    - decision-0065-entry-evaluation-and-the-cli-query-surface
    - decision-0066-frontend-diagnostics-carry-source-identity
  amends:
    - decision-0061-the-declaration-artifact
    - decision-0063-the-frontend-graph-tier
    - decision-0065-entry-evaluation-and-the-cli-query-surface
  supersedes: []
---

# a local module is a declared subject with a manifest, a src tree, and module-local import bindings, loaded from an explicit root without a solver

> edit, 2026-09-30: the `import name` clause that source files keep here is replaced by per-file `use`
> ([0086](0086-names-are-scoped-per-file.md)), and an input is visible only in the files that use it.

> edit, 2026-09-30: the project-file round that implemented [0079](0079-a-project-is-one-file.md) broke
> this record's kept invariant, "its refusal to follow symlinks": the include read checked only the
> final path component, so a symlinked directory component was followed, and no check anywhere held a
> path inside the directory of the project that named it, so an include, an input route, or a member
> spelling `..` or an absolute path read outside it. both are restored at the store boundary: every
> component of every path a project names is inspected, and `..`, absolute spellings, and symlinked
> components are refused (`E-3072` for the escape, `E-3039` for the symlink). this also amends the
> paragraph below that says "`..` is permitted" for dependency paths: under 0079 a route stays inside
> the declaring project's directory, which is the boundary [0081](0081-paths-are-values.md) generalizes
> to path values.

> edit, 2026-09-21: amended by [0079](0079-a-project-is-one-file.md). a project is one file with its
> inputs and declarations, split only by `include`; the manifest/source split, source discovery under `src/`
> and standalone mode are replaced. the declared subject, the subject-based ABI and the refusal to follow
> symlinks stand. `use ... from path` clauses become the `inputs` block of
> [0080](0080-inputs-are-parameters.md).

> amends [0061](0061-the-declaration-artifact.md): the module ABI manifest no longer encodes imported module
> names in module-name order. it encodes a sorted, deduplicated set of imported subject/ABI pairs. a local
> binding name is an elaboration input and a tooling sidecar, not a semantic fact; renaming a `use` alias
> leaves the ABI, the interface surface, and every body digest unchanged.
>
> amends [0063](0063-the-frontend-graph-tier.md): `ImportEnv` values remain keyed by binding for scoping, but
> each import now carries its declared subject, and the surface encoding and validation compare subjects.
> `FrontendImport` keeps its binding half, which continues to feed only elaboration.
>
> amends [0065](0065-entry-evaluation-and-the-cli-query-surface.md): `--module` now names a manifest, and
> passing `module.pi` selects manifest loading unconditionally. the file-relative neighbor resolver of
> standalone mode is reachable only when the caller explicitly loads a `.pi` source file, never as a
> workspace fallback.

## context

M-13 delivered a surface notation whose module identity is a file stem and whose imports resolve against
neighboring files. a person can write one module and run its entries, but the two seams that would have to
grow into the module system are already in the wrong shape: `read_module` derives identity
from a filename, and the shared import environment keys every module by its binding name, so an alias and
its subject are the same string and neither can move.

the module system proposal argues the destination (subjects, manifests, source adapters, registries,
locks), but its first slice has to be small enough to measure: two local modules, one path dependency, one
pure entry, exercised through the existing CLI. this record settles what that slice fixes: local identity,
manifest structure, workspace membership, source discovery, and alias scoping. everything the slice defers
is named in unresolved with its owner.

## decision

### identity is a declared subject

a manifest's `module` clause declares a `(domain, name)` subject, canonically spelled `domain/name` in
coordinates. paths, dependency aliases, and manifest versions do not enter that identity. for the initial
grammar each segment is nonempty lowercase ASCII starting with a letter and continuing with letters,
digits, or hyphens; any later domain grammar expansion is an explicit amendment, not a silent widening. a
local declaration is a claim within the loaded program, not proof of publishing authority; who may
declare a domain remains the next slice's question.

the builtins keep the subject `pith`. a manifest cannot bind the alias `pith`, and no subject can
redeclare the builtin module's single-segment spelling, which the two-segment grammar already refuses.

### `module.pi` is a manifest, not a module

a manifest holds a `module` subject/version clause, at most one `workspace { members: [...] }` clause, and
any number of `use alias = subject from path "..."` clauses. it cannot import source modules or contain
computed declarations; source files cannot contain manifest clauses. both refusals are one diagnostic each,
naming the document kind the clause belongs to.

the manifest extends the existing lexer and parser through a document context rather than a second
configuration language, with the same token stream grammar, the same spans, the same diagnostic machinery,
and a canonical printer with one contract: printed text re-parses to the same manifest, and formatting
needs no dependency resolution. duplicate singleton clauses are refused; `use` clauses keep source order,
and their alias, subject, and path are validated where they are parsed, so a parsed-and-clean manifest
cannot hold an invalid subject spelling, an invalid version, or an unsupported source kind. registry, git,
and archive dependencies, and the version-ranged default source, parse to an explicit unsupported-source
diagnostic naming the clause; this slice admits only `from path`.

### a module owns its `src` tree

a module owns the regular `.pi` files under its `src/` directory, recursively, with module-relative paths
sorted before parsing so the merged surface does not depend on directory enumeration order. symlinks
inside that source set are refused rather than followed, so a module cannot implicitly own an external
tree; an explicit external dependency is a `use ... from path` clause in a manifest. an empty source set is
refused with a manifest diagnostic. workspace members and dependency paths name directories that contain a
`module.pi`.

### paths are routes, not identities

dependency paths resolve relative to the declaring manifest, and `..` is permitted. canonical filesystem
locations detect repeated routes and dependency cycles; they never become semantic keys: the digested
source identity of a module is its subject and its module-relative source set. two distinct locations
declaring one subject are refused even if their bytes match. repeated routes to one canonical location
load once and share one subject.

### workspaces are explicit

`workspace { members: [...] }` lists member directories explicitly and uniquely. membership supplies
locations, never imports or selection precedence: an unrelated member contributes no bindings and no
rules to a run that does not reach it through a `use` clause. a member manifest cannot declare another
workspace in this slice. loading begins from the supplied manifest path: there is no upward directory
search and no ambient user configuration.

### alias environments are module-local

each module elaborates under its own alias-to-subject environment, built from its own `use` clauses.
source files keep the existing `import name` clause; the name resolves against the owning manifest's
bindings. transitive dependencies cannot be imported without a direct binding: the environment holds
exactly what the manifest declared. a missing binding reports the import's source file and the owning
manifest's clause; a duplicate alias, a builtin-name alias, a subject mismatch between two routes, and a
dependency cycle each name the relevant manifest clause, and a cycle diagnostic includes the chain.

### versions are metadata here

manifest versions are parsed and retained (dotted integer segments) but nothing resolves against them.
path dependencies carry no version range, trigger no solver, and create no lock; they are live local
inputs and make no witnessed-content claim. this is deliberately narrower than the proposal's source
adapters: a lock and a temporary registry arrive with the equivalence measurement in the next slice.

### the semantic ABI names subjects, not bindings

the ABI digest of a module now encodes a sorted, deduplicated set of imported subject/ABI pairs instead of
binding-name/digest pairs, and the interface surface encodes the same basis through one shared
construction, an `ImportedAbis` value that sorts, deduplicates, and *refuses* a subject arriving with two
different digests, so neither representation can disagree with the other or resolve a conflict by input
order; its validation compares declared subjects.

precisely what changed in the bytes: item 3 of 0061's ABI manifest was `encode_str(binding)` followed by
the imported digest, in binding order; it is now `encode_str(subject)` followed by the digest, sorted by
subject with one entry per subject. for a standalone module the two spellings are identical (binding and
subject are the same string, the binding order was already sorted, and no subject repeats), so every
existing standalone ABI is byte-stable across the amendment, and the M-10 parity fixtures moved only
through the elaborator revision below. the interface surface's import list changed identically, including
its two-aliases-one-subject case, which used to encode an entry twice and fail its own decoder. nothing
else in either encoding moved: versions, lengths, and ordering rules are
unchanged, and the surface's format byte stays 1.

compatibility: none is owed and none is claimed. no release was cut, so under 0048 the pre-release state
databases and contracts are discarded rather than migrated; the query API version and the elaborator
revision both read 1, and the elaborator's golden digest is re-recorded against that revision. the digest
domains stay domain-separated under 0047: the amendment changes what participates in the import digest
item, prices the change through the revision, and leaves the other items untouched.

an alias edit therefore leaves elaborated coordinates, body digests, the semantic ABI, and the interface
surface unchanged. the edited source text and its import environment remain different *frontend inputs*:
their canonical input keys may move, which is correct, since the graph tier re-elaborates and discovers the
semantic outputs identical, and downstream cutoff still applies. likewise an edited dependency body may
change an entry result while the consumer's elaborated bodies stay reusable; frontend reuse and runtime
dependency revalidation remain separate measured properties.

### standalone source loading remains explicit

loading a bare `.pi` file keeps the M-13 behavior: file-stem identity, the file-relative neighbor
resolver, one shared import environment. it is reachable only through standalone mode. passing
`module.pi` to any command selects manifest mode unconditionally; a malformed manifest cannot fall back
to source mode. the README names this temporary distinction and the remaining migration work.

## the refusal table

each refusal is one specific, source-bearing diagnostic. source-bearing means the diagnostic selects the
offending clause in the manifest that caused it, or the offending import in the source file that made the
request, with the owning manifest named when the two differ.

| refusal | where it is diagnosed | code |
| --- | --- | --- |
| a manifest clause in a source file, or a source clause in a manifest | parse | E-3027 |
| a duplicate `module` or `workspace` clause | parse | E-3028 |
| a subject outside the two-segment lowercase grammar | parse | E-3029 |
| a version outside dotted integer segments | parse | E-3030 |
| a `from` source this slice does not admit | parse | E-3031 |
| a dependency path that is empty or absolute | parse | E-3032 |
| a `use` alias bound twice in one manifest | parse | E-3033 |
| a member directory listed twice | load | E-3034 |
| a member or dependency path naming no manifest-bearing directory | load | E-3035 |
| a member manifest declaring its own workspace | load | E-3036 |
| two locations declaring one subject | load | E-3037 |
| a dependency cycle | load | E-3038 |
| a symlink inside a module's source set | load | E-3039 |
| an empty source set | load | E-3040 |
| a `use` clause whose path declares another subject | load | E-3041 |
| a source file that cannot be read or listed | load | E-3042 |
| a source path that is not a regular file (a fifo, a socket, a device), including `src/` itself | load | E-3043 |
| an import with no binding in the owning manifest | elaborate | E-3008 |
| an alias binding the builtin name | load | E-3017 |

E-3008 and E-3017 are the existing unknown-import and builtin-shadowed codes; the missing-binding message
gains the owning manifest's location. the loader separates filesystem acquisition from validation of the
collected manifests and graph, so every validation refusal is a pure function of what was acquired, with
diagnostics attached at clause spans. acquisition admits only regular files (reading a fifo blocks, and a
loader that tried one would hang) and validates the source root itself before walking it, so neither a
symlinked `src/` nor a special file spelling a source name reaches a read.

## the acceptance fixture

a checked-in project, exercised by the CLI tests:

```text
examples/local-workspace/
  module.pi
  src/main.pi
  modules/greeting/module.pi
  modules/greeting/src/types.pi
  modules/greeting/src/rules.pi
```

the root manifest declares `module example/hello 0.1.0`, one workspace member, and one path dependency
`use greeting = example/greeting from path "modules/greeting"`. the dependency declares
`module example/greeting 0.1.0` and owns two source files, so the fixture cannot pass through the
single-file shortcut. from the root, the intended commands are `pith check module.pi`,
`pith explore module.pi`, `pith fmt --check module.pi`, `pith run hello` twice, then
`pith graph deps hello` and `pith explain hello`. the first run computes; a new process over the same
isolated store hydrates the result.

`fmt` over a manifest formats that manifest and the root module's own source files, reporting each file;
it does not edit dependencies or unrelated members, and `fmt --check` writes nothing.

## alternatives considered

### derive identity from the filesystem

rejected because every route a person takes to move a project would then move identity: renaming a
directory, relocating a workspace, or publishing the same bytes to a registry would all mint new modules.
the subject/ABI pair is the stable half and the path the volatile half; deriving identity from the
volatile half inverts that.

### a second configuration language

TOML or JSON for manifests was rejected on the module system proposal's own ground: two grammars mean two
span models, two diagnostic tables, and a formatter that cannot share the declaration grammar's
conventions. the document context keeps one lexer, one parser shape, and one refusal table.

### implicit workspace discovery

walking upward from the invocation directory to find a root manifest was rejected: loading must begin
from the supplied path so that a check of one manifest is one deterministic input set, and ambient
discovery reintroduces the user-configuration ambiguity the registry round has to settle instead.

### keep binding names in the ABI

the current basis (import binding names hashed into the module ABI) was retained as late as possible
because moving it reprices digest domains. rejected because the alias edit is exactly the edit a person
makes while refactoring, and on the current basis it invalidates every consumer of the module for a
change that asserts nothing new. binding names stay in elaboration inputs where scoping needs them.

### follow symlinks in the source set

rejected because a symlink is an undeclared dependency: following it would let a module silently own
another tree's contents while its manifest claims a local source set. the explicit form, a `use` clause
with a path, already exists.

## evidence

the slice's implementation has landed with its witnesses passing, and the round's measurements are linked
from [milestones](../planning/milestones.md) and stated in
[measured](../planning/measured.md#m-14-the-module-system--the-local-workspace-slice), as 0061 through
0065 do. the record is accepted against that evidence.

the checked-in fixture `examples/local-workspace` is exercised end to end by
`the_local_workspace_fixture_checks_explores_and_hydrates`: check and explore succeed, `fmt --check`
reports the manifest and the root's own source file and writes nothing, the first `run` computes while a
fresh process over the same store hydrates, and the dependency and explanation commands address
`example/hello::entry.hello`. `graph_select_over_a_manifest_creates_no_state_database` pins the read-only
half.

the digest-basis amendment is witnessed by
`renaming_an_import_alias_leaves_every_digest_it_can_unchanged`: two consumers binding one subject under
different aliases elaborate to equal ABIs, equal interface-surface bytes, equal body digests, and equal
coordinates. multi-file attribution is witnessed in `a_declaration_in_one_file_is_visible_to_the_rules_in_another`
and `an_error_in_the_second_file_names_that_file_and_local_offsets`; the loader's graph properties (the
diamond loaded once, member isolation, relocation and enumeration permutation) in `a_diamond_loads_the_shared_dependency_once`,
`an_unrelated_member_is_validated_but_its_sources_are_never_acquired`, and their neighbours; every row of
the refusal table has a test asserting its code in the manifest and workspace suites.

frontend reuse is measured through the graph tier rather than inferred from equal outputs.
`Workspace::project_onto_frontend` is the resolved workspace's route into `interface-of` and `bodies-of`,
and `workspace_graph::a_dependency_body_edit_leaves_the_consumer_bodies_reusable` shows a dependency body
edit recomputing that module's `bodies-of` while the consumer's inputs stay equal and its `bodies-of` is
served `Reused`; `a_public_representation_edit_moves_the_consumer_inputs` is its control and
`relocation_leaves_the_projected_inputs_identical` carries the relocation property through to the graph
tier's keys.

the reuse table's edit rows run as witnesses: `a_body_edit_in_a_dependency_keeps_the_consumer_elaboration`
holds the dependency's ABI and interface surface and the consumer's ABI and entry revision while the
edited rule's digest moves; `a_public_representation_edit_invalidates_the_consumer` moves the dependency's
ABI and refuses the consumer's use at its own file; and `an_edited_dependency_body_revalidates_the_entry`
drives the runtime half end to end: compute, edit, recompute rather than serve stale, then hydrate.

## unresolved

domain authority (who may declare `example/*`) is deliberately unanswered; a local declaration is a
claim, not authority, and the registry slice decides publication. registry, git, and archive sources,
version ranges and the solver, the lock, and local/registry IR equivalence are the next slice, which also
owns the `pith.lock` naming amendment and the `add`/`update`/`diff` commands this slice leaves as
refusals. host implementation binding and represented action projection remain 0065's follow-on work;
this slice executes pure represented entries only. the peerhood guard gains coverage for manifests naming
dependencies without banning the explicit workspace locator.
