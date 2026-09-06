---
schema: design-doc/v1
id: planning-modules-workspaces
title: module workspaces
summary: explicit manifests, declared subjects, path dependencies, and workspace membership — how several modules in one checkout become one runnable project
kind: planning
status: draft
created: 2026-09-06
updated: 2026-09-06
tags:
  - planning
  - language
  - modules
relations:
  informed_by:
    - planning-modules-system
    - planning-cli-surface
  depends_on:
    - decision-0063-the-frontend-graph-tier
    - decision-0065-entry-evaluation-and-the-cli-query-surface
    - decision-0066-frontend-diagnostics-carry-source-identity
  supersedes: []
---

# module workspaces

This is the first implementation slice of M-14, not the whole milestone. A person can declare two
local modules, import a dependency through a manifest binding, run a pure entry, and inspect its reuse
through the existing CLI. The acceptance fixture is a checked-in project, exercised by the CLI tests.

The slice has run. [0067](../../decisions/0067-local-module-workspaces.md) is its accepted record and
[measured](../measured.md#m-14-the-module-system--the-local-workspace-slice) holds the evidence, so the
choices below are recorded decisions rather than proposals. The plan is kept as written because it is
what the implementation was reviewed against; one correction it earned is at the end.

The full milestone still owes registry publication and local/registry IR equivalence, domain authority,
configuration refusals, source adapters, version resolution, locks, and compatibility diffing. Those
remain in [the module system](system.md) and [milestones](../milestones.md).

## the result a person gets

The fixture has this shape:

```text
examples/local-workspace/
  module.pi
  src/main.pi
  modules/greeting/module.pi
  modules/greeting/src/types.pi
  modules/greeting/src/rules.pi
```

The root manifest uses the proposal's syntax, with quoted filesystem paths:

```text
module example/hello 0.1.0

workspace {
  members: ["modules/greeting"],
}

use greeting = example/greeting from path "modules/greeting"
```

The dependency manifest declares `module example/greeting 0.1.0`. Source files use `import greeting`
and the existing notation to declare a pure entry named `hello`. The fixture's exact entry body is
compiled and tested when it lands, rather than introducing another proposed expression spelling here.

From the root, the intended commands are:

```sh
pith check module.pi
pith explore module.pi
pith fmt --check module.pi
pith run hello
pith run hello
pith graph deps hello
pith explain hello
```

The first run computes; a new process over the same isolated store hydrates the result. The dependency
has multiple source files so the fixture cannot pass through the existing single-file shortcut.

## decisions to record before implementation

One focused decision record should settle local identity, manifests, workspace membership, source
discovery, and alias scoping. It should identify the amendments to 0047, 0061, 0063, and 0065 that the
actual implementation needs. Public domain ownership is deliberately not inferred from local paths.

- A manifest declares a `(domain, name)` subject, canonically spelled `domain/name` in coordinates.
  Paths, dependency aliases, and manifest versions do not enter that identity. For the initial grammar,
  use two nonempty lowercase ASCII segments starting with a letter and continuing with letters,
  digits, or hyphens. Any later domain grammar expansion is explicit. A local declaration is a claim
  within the loaded program, not proof of publishing authority. Existing builtins retain `pith`;
  users cannot bind the import name `pith` or redeclare its builtin subject.
- `module.pi` is a manifest only. It cannot import source modules or contain computed declarations.
  Source files cannot contain manifest clauses. Extend the existing lexer/parser with a document
  context, a manifest AST, and a manifest printer; do not add a second configuration language.
- A module owns regular `.pi` files recursively under its `src/`, with canonical module-relative
  paths sorted before parsing. Refuse symlinks inside that source set in this slice, rather than
  allowing an implicit external source tree. Refuse an empty source set with a manifest diagnostic.
  Workspace members and dependency paths name directories containing `module.pi`.
- Paths resolve relative to the declaring manifest. `..` is permitted for explicit path dependencies.
  Canonical filesystem paths detect repeated locations and cycles; they never become semantic keys.
  Two distinct locations declaring one subject are refused, even if their bytes happen to match.
  Repeated routes to the same canonical location are loaded once and share one subject.
- Workspace members are explicit and unique. Membership supplies locations, never imports or
  selection precedence. Member manifests cannot declare another workspace in this slice. Loading
  begins from the supplied manifest, without upward directory searches or ambient user configuration.
  Evaluation binds only the selected root's dependency closure; unrelated members contribute no rules.
- Each module gets its own alias-to-subject environment from its own `use` clauses. Transitive
  dependencies cannot be imported without a direct binding. A missing binding reports the import
  source and the owning manifest; subject mismatches, duplicate aliases, and cycles name the relevant
  manifest clauses. A cycle diagnostic includes the dependency chain.
- Manifest versions are parsed and retained as metadata. Path dependencies carry no version range,
  trigger no solver, and create no lock. They remain live local inputs and make no witnessed-content
  claim. Registry, git, and archive dependencies receive an explicit unsupported-source diagnostic.

Keep explicit standalone source-file loading for existing M-13 callers during this slice. Passing
`module.pi` selects manifest mode unconditionally; a malformed manifest cannot fall back to source mode.
The legacy file-relative resolver is reachable only through standalone mode, never as a workspace
fallback. The README names this temporary distinction and the remaining migration work.

## identity and graph obligations

There are two current seams to change. `pith-query::source::read_module` derives identity from a file
stem, while `pith-query::program` locates neighboring files and builds a shared import environment.
Manifest mode replaces both with declared subjects and module-local bindings.

An alias edit must leave elaborated coordinates and body digests unchanged. The current ABI path also
encodes import binding names: `load.rs` collects names from scoped imports, and `abi.rs` hashes them.
The proposed semantic ABI instead encodes a sorted, deduplicated set of imported subject/ABI pairs;
local binding names remain in elaboration inputs and tooling sidecars. Apply the same basis to
`InterfaceSurface` encoding and validation. This is a digest-basis amendment under 0047 and 0048:
price the affected digest domains explicitly and update the elaborator revision and fixtures. Keeping
pre-release format versions pinned does not authorize silently rebasing a digest domain.

Do not promise every frontend key survives an alias edit. The edited source and its import environment
are different frontend inputs. Their semantic output can still be identical, allowing downstream cutoff.
Likewise, an edited dependency body may change an entry result even though its consumer's elaborated
body remains reusable. Test frontend reuse and runtime dependency revalidation as separate properties.

Multi-file support already exists in the graph tier through `merge_module_files` and `ModuleFiles`.
The direct loader still constructs a single-file `LoadedModule`. Factor the common module elaboration
and projection logic so direct queries and the graph rules agree. Preserve per-file positions,
definition locations, documentation, entry spans, and host-refusal diagnostics; concatenating source
strings and attributing every error to the first file is not an implementation of this boundary.

## implementation sequence

Each step is a reviewable change with its own focused checks. The acceptance fixture closes the slice
only after all steps land.

1. Record the local-module decision and its refusal table. Add the fixture layout and planned command
   expectations to its documentation. Identify the precise digest-domain amendments before changing
   encodings. Do not mark the decision measured until its witnesses pass.
2. Add manifest syntax and canonical formatting in `pith-syntax`, with manifest data beside the
   existing frontend syntax types. Cover the subject/version grammar, quoted paths, `use`, and
   `workspace`; preserve source spans. Reject clauses in the wrong document context and duplicate
   singleton fields. Manifest formatting must work without resolving dependencies.
3. Add a local workspace loader under `pith-loader`. Separate filesystem acquisition from validation
   of the collected manifests and graph. Return a deterministic dependency order, declared subjects,
   module-relative source sets, and per-module bindings. Validate all listed member manifests for
   membership/identity errors, but acquire and elaborate source only for the selected dependency
   closure. No dependency on phloem, xylem, or stele is introduced.
4. Unify multi-file elaboration and implement binding aliases independently of subjects. Replace the
   single-source assumptions in `LoadedModule` and query projections. Amend ABI/surface identity
   consistently and feed the resolved source sets and environments to the existing frontend graph
   witness. Keep builtins explicit through `import pith`.
5. Route manifest targets through `pith-query` for check, explore, entry execution, explanation, and
   graph queries. Share one program construction and binding path between engine authorities.
   `--module` continues to select the root manifest for entry commands. `fmt module.pi` formats that
   manifest and its own source files, reporting each file; it does not edit dependencies or other
   members. `fmt --check` writes nothing. Extend the output DTO contract and snapshots if needed.
6. Finish the CLI fixture, migration documentation, and regression checks. Correct the README's stale
   language/CLI claims. Link the slice's measurements from M-14 without closing the full milestone.

## acceptance evidence

| witness | required result |
| --- | --- |
| two-module CLI fixture | check/explore succeed; pure entry computes, then hydrates in a fresh process; graph and explanation commands address that entry |
| alias rename in manifest and source | equal declaration coordinates, body digests, semantic ABI, and interface surface; frontend input keys may change |
| body-only dependency edit | dependency bodies change, its interface is reusable, consumer bodies remain reusable; a changed called result revalidates runtime consumers |
| public representation edit | ABI/surface change and consumer elaboration invalidates or diagnoses the incompatible use |
| relocation and enumeration permutation | equal semantic artifacts and canonical frontend inputs when module-relative paths and bytes are unchanged |
| module-local aliases | two modules may use the same alias for different subjects without leakage; an undeclared transitive import is refused |
| diamond dependency | shared dependency is loaded and registered once; no duplicate-rule ambiguity is introduced |
| member isolation | an unrelated member's source edit cannot affect the selected root's semantic artifacts or registered rules |
| refusals | duplicate alias/member/subject, mismatched subject, dependency cycle, missing source set, malformed manifest, and unsupported source each have specific source-bearing diagnostics |
| multi-file diagnostics | a declaration error in the dependency's second file and a bad root import both point to the correct file and local offsets |
| read-only commands | check, explore, fmt --check, and graph select create no state database; query failures still produce useful diagnostics |
| formatting | manifest/source formatting is idempotent and preserves resolved subjects, dependency graph, module ABI, and body digests |

Extend the peerhood guard to cover new executable/configuration inputs appropriately: user manifests
may name their dependencies, while loader/compiler code must not give particular domain crates a
special resolution route. Do not turn a blanket ban on names in manifests into a ban on the explicit
workspace locator this slice introduces.

Run focused syntax, loader, query, and CLI tests while implementing. At the end run the repository's
full local `just ci`, including documentation, determinism, elaborator-digest, lint, and workspace
checks. Use isolated store/state roots for CLI witnesses. Record failures and platform limitations
instead of treating unrun checks as evidence.

## what the plan got wrong

The acceptance table asks a body-only dependency edit to leave "consumer bodies reusable", and the
implementation first satisfied that by elaborating the workspace twice and comparing digests. Equal
semantic outputs are not a reusable lookup — a frontend that recomputed everything would pass the same
check — so the claim was weaker than the row's wording. Closing the slice therefore added what step 4
implied but did not name: a projection from a resolved workspace onto the frontend graph tier, and a
witness that asks the engine for the evaluation's own source. The row now holds in the sense it was
written in.

The projection is not yet on the CLI's path; `pith-query` still elaborates a manifest program directly.
That is deliberate scope, and it is the seam registry loading has to feed, so it exists before an
acquired source depends on it.

## the next slice

The next slice takes the same resolved-subject and source-set boundary through a lock and temporary
registry, and measures local/registry IR equivalence plus the two domain-registry configuration
refusals. It decides public domain authority, index versus pinned revisions, and the module-specific
resolution protocol before implementing their consequences. It also owns the `pith.lock` naming
amendment, witness admission, and the applicable add/update/diff commands.

Host implementation binding and represented action projection remain explicit follow-on work. This
slice executes pure represented entries; a host action still produces the existing coordinate-bearing
refusal. Registry delivery alone will not turn that declaration into an executable action.
