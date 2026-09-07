---
schema: design-doc/v1
id: planning-modules-distribution
title: module distribution
summary: how a module leaves its author's checkout — publication, admitted sources, reproducible locks, transitive resolution, compatibility reports, supply-chain admission — and the dependency order that builds it
kind: planning
status: draft
created: 2026-09-06
updated: 2026-09-06
tags:
  - planning
  - modules
  - milestones
relations:
  informed_by:
    - research-module-distribution
    - research-registry-supply-chain-security
    - research-language-frontend
    - research-index-formats
    - research-dependency-resolution
    - research-artifacts-and-trust
  depends_on:
    - planning-milestones
    - planning-modules-system
    - planning-modules-workspaces
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0044-the-first-source-adapter
    - decision-0067-local-module-workspaces
    - decision-0068-published-surfaces-are-context-bound
    - requirements-security-and-trust
  supersedes: []
---

# module distribution

A module is distributed when it is reproducible outside its author's checkout: published, resolved,
its selection and evidence recorded, that exact content acquired in a clean consumer, and the same
program elaborated there. This document holds the contracts that make that true, and the dependency
order that builds them. It covers all four source kinds and the CLI, not only the first registry fixture.

It is the remainder of M-14 after [workspaces](workspaces.md); the milestone stays open until the
obligations below have witnesses.

This is an implementation plan, not an accepted decision. Recommended choices are stated so work can
start without rediscovering the alternatives, but the opening records must settle their contracts before
dependent encodings land. [The targeted research](../../research/module-distribution.md) separates external
evidence from deductions about Pith's existing code.

[Registry integration](registry-integration.md) states how far the registry is a part of Pith rather than
a service beside it: one language, one engine and resolver protocol, one admission mechanism, one store,
one artifact, and consumer-written policy. Its integration tests belong to the slices below, and the seams
it lists are the cheapest wrong commits in this sequence.

[The module registry](registry.md) holds the registry's own model, on
[the incident and specification evidence](../../research/registry-supply-chain-security.md) gathered for it:
a signed, append-only, metadata-only index whose every cached field is derived from the revision it names.
Supply-chain admission is part of this milestone rather than a later hardening pass, and it is small
because three structural choices — source modules, self-authenticating revision pins, and domain-bound
routing — remove most of what a registry would otherwise have to defend. Nothing about it is measured yet.

## starting point

The working copy contains the local-workspace slice: manifest grammar, declared subjects, path
dependencies, multiple source files, module-local imports, query integration, and a CLI fixture. Slice 0
below has since closed it on evidence — full `just ci`, the acceptance table audited row by row, the two
uncovered rows given witnesses, and the frontend-graph gap that audit found now filled. 0067 is accepted
and the local-workspace plan is reconciled; the evidence is in
[measured](../measured.md#m-14-the-module-system--the-local-workspace-slice).

Slice 1's records and grammar are in the working copy: [0069](../../decisions/0069-module-authority-is-consumer-configuration.md)
and [0070](../../decisions/0070-module-acquisition-resolution-and-replay.md) are written as proposed, the
lexer and manifest grammar carry ranges, registry bindings, and the remaining source kinds, and the
refusal codes they allocate exist. Slice 2 is implemented against them, on the evidence in
[measured](../measured.md#m-14-the-module-system--the-acquisition-boundary-slice); both records stay
proposed until their own measurements run. Do not infer completion from the presence of test names in
any slice that follows.

## the contracts to settle first

| concern | recommended M-14 contract | required decision or amendment |
| --- | --- | --- |
| subject | keep 0067's `domain/name`, one selected version per subject, aliases local to their module | public authority supplements local identity; paths and URLs remain provenance |
| public authority | consumer configuration appoints a registry for each domain; publication requires an explicit publisher/domain grant at that registry | define the trusted publisher-identity boundary and evidence; no global ownership service is needed for the fixture |
| routing | exact domain lookup, explicit subject source binding, no registry search or default fallback | duplicate domain bindings fail before I/O; conflicting explicit routes to one subject fail across the whole resolution |
| configuration | project bindings win over user bindings as the existing proposal states; collisions name both; `strict` operation requires project-owned bindings | dependency manifests cannot alter global authority; record effective bindings and policy provenance |
| resolution | indexed module candidates, numeric-segments ordering, newest-first preference, deterministic search budget | argue the exact-revision alternative; modules declare their own constraint model under 0040 |
| release and ABI | immutable source release plus advertised surface with its dependency ABI context and elaborator revision | amend the context-free reading of the candidate ABI in module-system.md; never compare artifacts built in different contexts as if identical |
| module bytes | canonical module-relative tree containing the manifest and owned `src/` files; archive-byte integrity is separate | define file kinds, modes, extraction prefix, and exclusion of VCS/cache/build output consistently across adapters |
| lock | canonical document value with deterministic text projection; selected content and origin, effective bindings, resolver identity, scheme, preferences, and snapshot references | define module-specific fields and path entries; apply 0041's caller-write and staleness principles without importing package coordinates |
| workspace | one lock for the declared workspace, solving the union of member/root manifest requirements; evaluation still loads only its selected root closure | explicitly distinguish shared version agreement from rule membership; a member source edit cannot inject rules into another root |
| publication | a release is a signed index entry naming an immutable revision and subpath, every cached field of which is derived from that revision; nothing is uploaded | disagreement between an entry and its source is a refusal, not an authority rule; a locator stays outside the identity |
| admission | every admitted source, lock entry, and reusable artifact carries a recomputable record naming content, evidence, policy, time context, and reason | admission is never a permanent flag beside the bytes; revocation and expiry change the result without changing content |
| freshness | resolution time is a declared input with a recorded cutoff; withdrawal lives in the index and is read locally, never frozen into the lock | rollback counters are exempt from 0048's pin-at-1 rule; a lock replays a selection, it does not replay a judgment about that selection's fitness |
| compatibility | exact surface change report over the module's own declarations, conservative classification of additions, optional evidence for a named consumer closure | withdraw the added-rule-is-minor claim as measured; a surface difference is not attributable to the edited module until the imported-ABI region is subtracted; structural equality is not behavioral compatibility |

Two of the three probes have been run, they moved the contracts above rather than confirming them, and
[0068](../../decisions/0068-published-surfaces-are-context-bound.md) records the result with its limits.
They live in `crates/pith-loader/tests/probes.rs` and `crates/pith-cli/tests/end_to_end.rs`.

**A published surface is context-bound, and so is a published ABI.** With a consumer's source bytes
unchanged, a public addition to its dependency moves the dependency's ABI, the consumer's ABI, and the
consumer's *interface surface* — and what moved in the surface is exactly the dependency's ABI digest,
established by redacting it and finding the two encodings equal. A dependency body edit moves none of them.
So a registry field holding one ABI or one surface per released version means "under the context this
release recorded", never "under every resolution its requirements admit", and `pith diff` must subtract the
imported-ABI region before attributing any surface difference to the edited module.

**An added rule breaks consumers, but not where the earlier objection said.** Within one module, two rules
providing one interface are refused at elaboration with `E-3012`, so a publisher cannot build that break.
Across modules, the closure elaborates cleanly — `check` passes — and the consumer fails only at evaluation
with `E-1102`. The break is therefore a property of a consumer's whole closure, not of the edited module;
the publisher's own `check` cannot detect it; and no comparison of two versions of one module can prove an
addition safe. Conservative classification is forced, not chosen.

The third probe is not yet runnable: withdrawing an admitted, locked selection and replaying the lock needs
registry machinery that does not exist. It stays an obligation of slice 3.

A third probe belongs beside them because it decides a format field: take an admitted, locked selection,
withdraw it in the index, and replay the lock. The selection must still be the recorded one and
the run must still refuse. That fixes the separation between what a lock freezes and what it does not
before any lock bytes exist outside this repository.

The dependency-context recommendation permits source re-elaboration under a consumer's selected closure.
The published surface is checked under its recorded publication closure; reuse in another closure needs
matching imported ABIs and elaborator revision. A different resolved context is not source tampering.
Candidates include the advertised artifact and context in their canonical metadata. A lock records the
selected source binding; resolved frontend artifacts carry the actual context. Do not make normal local
body edits require republishing a registry ABI or rewriting immutable dependency pins.

## a forge is an implemented interface, not a table

[The module system](system.md) already settled that forge shorthands are configured sugar over the general
git route, that the host comes from configuration rather than the compiler, and that adding `codeberg:` is
therefore not a language change. This sharpens *what* configuration means, because a table of base URLs is
the weaker form of the right idea.

A forge is a **declared interface implemented in `.pi`**, with typed options. `github`, `gitlab`, and
`forgejo` are first-party implementations shipped with the distribution as ordinary peers, and a
self-hosted instance instantiates the same implementation with its own options rather than being handled
by a special case. This is 0056's peerhood test applied to routing: the shipped forges come through the
door a third party's would, and if any of them needs a private hook the extension model is missing
something.

Two constraints make it work, and both are load-bearing.

**A forge implementation is pure.** It computes locators; it does not fetch. Given a reference and its
typed options, it returns where a revision may be found and how a symbolic reference resolves — values.
The acquisition adapter consumes those as declared inputs and performs the effect, on the caller side,
exactly as 0044 requires. No forge implementation can run during evaluation, and none can be a channel
for ambient network access.

**The general git route needs no forge.** Bootstrap would otherwise be circular: a forge implementation is
a module, and modules are acquired through routes. Because a plain revision and locator are a complete
route on their own, a third-party forge implementation can be acquired without itself being needed, and
the first-party ones ship in the checkout. Any forge shorthand must remain expressible in the general form.

The degenerate case unifies the two answers this design had. A forge with no special behavior is only a
base URL, and its implementation is one line — so `forge acme = git "https://git.acme.internal"` and a
full provider with typed options are the same mechanism at two points, not a config path and a code path
competing. Typed options are the reason to prefer the interface: a provider's authentication style, API
flavor, and archive capability become named fields with declared types rather than stringly configuration
a reader has to guess at.

What a forge may never be is dependency-supplied. Like routing and admission policy, forge declarations
are consumer configuration; a dependency's manifest cannot introduce, replace, or extend one.

The locator fix in [the registry](registry.md) makes this safer than the shorthand originally could be.
Because a locator is a hint outside the identity and content is checked against the normalized tree digest,
a forge's archive endpoint and a clone of the same revision produce the same measurement — so a fast path
is interchangeable rather than a second lock shape. Nix's `github:` wart, where the shorthand and the git
URL fetch and lock differently, is not reachable from here.

## implementation slices

The sequence below is the dependency order. Each slice can be several focused commits; the checkpoints
are measured outcomes, not a requirement to make a single large change. Candidate/lock codecs wait for
slice 1. The differ can be implemented after that decision while acquisition work continues, but its
publication gate waits for the round trip.

Slice 3 will likely split: the round trip, the signed-entry publication path, and the key-set and
append-only client are three reviewable pieces. Split them by boundary, not by deferring the verification
half — a round trip that admits content on the registry's word first, and grows authorization after, is
exactly the retrofit this ordering avoids.

### 0. close local workspaces on evidence

Done. The audit found the gap this step predicted: `tests/edits.rs` compared two direct elaborations, and
the frontend graph tier had no caller outside its own test file, so nothing measured a reusable lookup
over a resolved workspace. `Workspace::project_onto_frontend` is the missing route — source sets published
by content identity, import environments naming each dependency's published surface — and
`tests/workspace_graph.rs` measures the cutoff through it, with a representation-edit control and a
relocation case. Runtime revalidation stays the separate CLI witness it already was. Two acceptance rows
that had no test gained one: module-local alias scoping with an undeclared transitive import refused, and
the read-only guarantee over all four commands rather than `graph select` alone.

`just ci` passes at 1083 tests across 112 suites. The outcomes are in `measured.md`, linked from M-14;
0067 is accepted and the local-workspace plan records the correction. This closed a slice, not M-14.

What it deliberately did not do: route `pith-query` through the projection. No person-facing command
reuses a frontend computation yet. Slice 2's requirement that registry loading feed the same path as local
loading now has a path to feed, which is why the projection was worth building first.

### 1. record authority, distribution identity, and replay semantics

Write focused records for authority/routing and module acquisition/resolution/locks. Keep compatibility
as its own record if the two probes require enough argument to obscure either concern. Extend the
existing lexer and manifest AST with ranges, registry bindings, and the missing source clauses; keep
source-file imports unchanged. All examples become parser fixtures before subsequent code relies on them.

Settle the table above, including numeric-equivalent version spellings, source conflicts through a
diamond, publication of path dependencies (refused), and manifest/index disagreement (refused).
The authority record covers the pinned root key, domain key sets, signed entries, and the consumer-verified
append-only check; take its argument and its threat model from [the registry](registry.md) rather than
restating them. Settle there that an index entry is a derived cache of the manifest at its pinned revision,
so a disagreement is a refusal rather than an authority rule, and that a locator is a hint outside the
identity. Record the rollback-counter carve-out against 0048 in the same place, before an encoder exists to
inherit the wrong rule.
Declare how local module paths and a registry snapshot are selected without upward search. If a workspace
member is selected from another directory, name the workspace root explicitly rather than rediscovering it.

Define both meanings currently attached to `pith diff`: workspace/lock drift and comparison of two module
surfaces. Preserve bare `diff` for workspace drift; give surface comparison an explicit operand form or
subcommand and pin the help contract.

Exit: accepted-for-implementation contracts, three executable probes, grammar fixtures, and a refusal table
with source-bearing diagnostics. The refusal table includes the authority and admission cases, each named
at its own boundary rather than folded into a later generic check. Records remain proposed until their
full implementation measurements.

### 2. establish reusable primitives and an acquisition boundary

Implemented, on the evidence in
[measured](../measured.md#m-14-the-module-system--the-acquisition-boundary-slice). The paragraph below
is the contract as written before the work; the notes after it record what landed and what stayed with
later slices.

Keep `pith-loader::Workspace`'s resolved subjects, source sets, and per-module environments as the input to
elaboration. Separate acquiring a module from assigning its aliases and compiling it. Registry loading
must feed the same path as local loading, including diagnostic file identity and frontend graph inputs.

Introduce a module-domain implementation for constraints, candidates, resolution answers, and locks.
The compiler/query driver links the protocol and module frontend support; it must not link phloem,
xylem, or stele as a shortcut. Register the module resolver explicitly on the same engine API available
to peers; its bootstrap declarations cannot require resolving their own source first.

Audit small reusable pieces in phloem: `identity::NumericSegments`, the range operations in `constraint`,
`witness` inclusion verification, `archive` parsing, and atomic publication. Extract only domain-neutral
mechanics that have two concrete callers. Preserve phloem's declaration names and digest behavior behind
its adapters; its candidate, lock, index, solver explanations, and preference values stay domain-specific.
Crate names and grouping follow the dependency graph, not a plan to move the whole package library.

Unify admission rather than writing a second one. `crates/phloem/src/substitution/` already holds
0042's `Admission`, `admit`, and `Refusal` for binary offers, and a module source's admission is the same
decision over different evidence. One refusal type with a clause per claim keeps T-5's separate claims
separate while sharing the machinery.

Exit: a local source still produces identical semantic artifacts through the new boundary; phloem's
conformance fixtures retain their identities; a dependency-closure guard proves frontend code contains
no first-party package/build/system domain. Shared mechanisms are exercised through both callers. The
module resolver registers through the engine API peers use, and the solver performs no I/O — removing
network access changes only whether a universe could be acquired, never a solved answer.

What landed. Acquisition is a `ModuleStore` — locate, manifest, sources over a store's own location
type — with the local filesystem as one implementation and a registry-shaped `Route` that carries the
subject a registry serves by; `Workspace::resolve` runs any store through the same graph resolution,
admission, and source acquisition `Workspace::load` uses, and a module's provenance is a rendered
location for diagnostics, never a semantic key. A module source's admission runs through the same
machinery a binary substitution admits under: `pith-constraint` holds the interval algebra over an
abstract ordering and the `Refusal<Clause>` envelope, phloem's `Range` and `admit` delegate to it
behind adapters, and the module clauses — subject, version against a written range — refuse at
acquisition under 0070's `E-3063`. The module resolver is a `modules.resolve` pure rule over
constraints, universe, preference, and budget, with typed segment versions, canonical-form identity,
a derived rule revision, and an own deterministic backtracking search whose four outcomes match the
protocol; it registers through `RegisterModuleResolver` on the engine's public call, and the
dependency-closure guard holds every kernel crate's linked dependencies to no first-party domain.

What stayed with later slices. The module lock document and its text projection land together in
slice 3, which is also when the registry client — and with it the remaining admission clauses over
content identity, witness evidence, and policy — replaces the fixture store the tests use. The audit's
other candidates — `NumericSegments`, witness inclusion, archive parsing, atomic publication — each
still have one concrete caller, so none was extracted; the range algebra and the admission machinery
were the two with two. No person-facing command consumes the resolver or the projection yet; slice 7
routes them.

The signed-index slice ran the audit again and found two more with two concrete callers: the
line-oriented tokenizer phloem's lock text had grown and the registry's line format needed, now
`pith-diag`'s `text` module with phloem delegating to it; and the hexadecimal codecs that digests,
keys, and signatures all spell, now `pith-ids`' `encode_hex` and `decode_hex_into` with
`ContentDigest`, phloem's digest reader, and the registry's key spellings sharing one
implementation. The range-token codec stayed apart deliberately: phloem's bounds carry opaque
scheme-spelled versions parsed by the domain, the registry's carry typed segments parsed by the
grammar, and one codec would force a single payload semantics on both.

### 3. complete an exact-version registry and lock round trip

The first routing boundary is implemented. Root domain bindings are checked before member or dependency
acquisition; duplicate domains name both clauses, unconfigured domains and registry names refuse without
fallback, and adapters receive a `RegistryRoute` carrying the consumer-selected locator and root key.
Its private fields keep unchecked registry clauses out of that adapter route. Dependency-supplied authority
is diagnosed before walking that dependency's own requirements. `tests/routing.rs` records adapter calls
for these refusals; `tests/acquisition.rs` consumes the configured route in the existing equivalence fixture.
Explicit user configuration, project precedence, the project-owned authority check, and whole-closure
route agreement now have witnesses in
[measured](../measured.md#m-14-consumer-configuration-and-route-agreement). Full operation modes, signed
entries, and locks remain open.

The signed-index half has since landed, on the evidence in
[measured](../measured.md#m-14-the-module-registry--the-signed-index-slice): the line format and its
ed25519 signatures, publication that derives every cached field from the manifest it releases and
refuses path dependencies, the client's four checks with rotation and rollback fixtures, the
directory host, and the store whose acquisition pipeline refuses a wrong cache, a wrong tree, an
unreachable index, and a withdrawn release each at its own boundary — with the round trip that
elaborates the same bytes identically from a local directory and through the signed index. What
remains of this slice is the lock document with its text projection, the withdrawal-replay probe
that needs it, and the admission-value calculus; the store serves exact selections until slice 4
wires the version universe through `modules.resolve`.


Start with a temporary directory registry and exact selections. Publish `example-domain` as an ordinary
manifest module. Generate index metadata from the manifest, store its source tree and advertised surface,
and exercise the registry's explicit publishing grant. Register no special case for that domain.

Publication uploads nothing. `publish` derives an entry from the manifest at a pinned revision, measures
the normalized tree at its subpath, signs the result, and proposes it; admission appends the registry's
time. Implement the client's four checks here — the pinned root key over domain key sets, key-set
generations, per-entry signatures, and the consumer's own per-subject append-only record — together with
key rotation and generation-rollback fixtures. Retrofitting recovery into a deployed trust root is the
change this ordering exists to avoid.

Use one acquisition pipeline: locate, read bounded content, verify transport integrity, normalize the
module tree, measure its identity, validate subject/version/requirements, verify applicable witness
evidence, then expose the admitted source set. Never turn a reference or the registry's claimed digest
into measured evidence without reading bytes. Close 0044's named checkpoint gaps in this pipeline —
signed checkpoints, consistency against a persisted last-seen checkpoint, and a configured witness policy
whose quorum requirement is stated — rather than shipping a second boundary on the pinned-checkpoint
footing 0044 called a degradation.

Normalization executes nothing. No install or publication hook, no native host code, no executable Git
filter, no submodule hook, and no editor or agent configuration in a dependency tree may run or take
effect. This is the strongest control M-14 has and it is free only while it is defended by a bounded
file-kind allowlist and an explicit refusal for everything else.

Acquisition produces an admission record, not a boolean: content identity, evidence consulted, policy in
force, time context, and reason. It is recomputed rather than stored as a trusted bit, so a later
revocation or an expired snapshot changes the answer without changing the bytes.

Implement the module lock value and its text projection together. Exact-version resolution is already
a small instance of the module resolver protocol; it may initially have one candidate per subject.
Writing the lock remains a caller effect. Failed acquisition, validation, or resolution leaves the prior
lock intact. Preserve the evidence needed to reacquire content after deleting the engine database;
snapshot identities must have locators or retained data, not only unrecoverable hashes.

Exit: the exact same module loaded locally and through the registry has equal declarations, ABI,
interface surface, and represented IR under the same dependency context. A fresh consumer uses the lock
without resolving a new version. A mirror mismatch, rewritten binding, invalid proof, and unauthorized
publication each fail at their own boundary. Both domain-configuration refusals run before any network
attempt. An entry signed by an unenrolled key, an entry whose cached fields disagree with the manifest at
its pinned revision, a key set whose generation moved backwards, and a previously seen entry that is now
altered are each refused distinctly — wrong cache, rollback, fork, and an unreachable index are four
outcomes, not one. Add a pure executable companion fixture; `example-domain`'s host action need not become executable
to prove the milestone's IR-equivalence claim.

### 4. implement transitive resolution over a scoped universe

Acquire a finite, consistent metadata snapshot outside the engine. Its scope is the closure of subjects
reachable through eligible candidate requirements, including alternatives that could change the answer,
not merely the versions eventually selected. Bound acquisition separately from deterministic solver work;
I/O failure and an absent candidate are different outcomes. A Git commit can provide the first registry
snapshot boundary; a local publisher must expose an equivalent stable snapshot or detect a concurrent edit.

Implement the module's host resolver with 0040's four inputs and explicit solved, unsatisfiable,
underdetermined, and exhausted outcomes. Start with deterministic bounded backtracking at fixture scale;
use exhaustive small-universe enumeration as an independent correctness oracle. Do not introduce a larger
solver dependency without a measured need. The phloem implementation is evidence for the protocol,
not a module model to rename wholesale. A search limit is never evidence of unsatisfiability.

Resolution time is a declared input, not a wall-clock read inside the solver. `update` takes a fixed
cutoff, records it, and does not drift as time passes. A cooldown is then a consumer policy comparing that
cutoff against the registry's admission time — a rule the consumer writes, not a mechanism the registry
operates. Withdrawal is consulted here as data, so a withdrawn candidate is never selected in the first
place.

Solve one version per subject across workspace requirements. Trace every added constraint back to its
manifest/index clause. Dependency-added constraints must also be checked against already chosen subjects.
Keep the scheme, preference, budget, candidate provenance, advertised surface context, and resolver revision
in the computation's declared inputs or rule revision as appropriate. Lock serialization follows the
successful answer and acquired evidence, with 0041's deliberate budget omission retained.

Exit: a release a consumer's cooldown policy excludes is not selected, and the fixed cutoff appears in
the lock;
a diamond shares one version; a case requiring backtracking succeeds; incompatible ranges report
the actual chain; equal-order competing versions refuse unless canonicalization already makes them one
candidate; exhaustion is distinct. Input permutations produce the same answer and lock. An unrelated
registry subject leaves the scoped universe unchanged; a relevant candidate or requirement edit moves it.
The resolver reuses and hydrates through the engine, and an input change explains why its answer moved.

### 5. complete registry, Git, archive, and live-path adapters

Extend the admitted-source contract through all four source kinds promised by M-14, including archives:
not everything a person depends on lives in a repository, and vendored or generated distributions are the
case that keeps them. Implement the forge interface above here, with `github`, `gitlab`, and `forgejo` as
its first three implementations and a conformance fixture that runs all three plus a locally declared
instance through one code path. Directory registries are
the test host; Git can distribute the same index snapshot, with remote transport behind the adapter.
Git module dependencies resolve a tag/reference at update time, record the full commit and module subpath,
and measure the selected tree. Locked reads never resolve the tag again. Forge shorthand expands to the
general Git route without changing fetch or lock semantics.

An explicit Git or archive route names one module, not an index. Acquire its manifest outside the solver
and present it as a singleton candidate with its declared requirements; check any written version range
against that manifest version. Do not scan tags to invent a version universe. Its transitive registry
dependencies then use the ordinary metadata closure. This is the necessary acquisition cost of sources
without a separate index, not permission for the pure solver to perform I/O.

Archive dependencies check the archive digest and then the normalized tree, supporting a documented
bounded initial format rather than silently accepting unknown members. Define behavior for symlinks,
special files, duplicate paths, traversal, submodules, and Git LFS pointers. These are the normalizer's
security boundary as much as its format boundary: a symlink escape, a traversing path, and an executable
Git filter are attacks, and each is refused by name. An explicit refusal is valid
for unimplemented source-tree features; a plausible but incomplete tree is not.

Paths remain live and explicitly unwitnessed. Their location is an explicit route; source edits update
frontend inputs without manufacturing immutable remote evidence. Publication refuses modules containing
path dependencies. All adapters check a manifest's claimed subject against the requested one.

Exit: identical module-relative bytes give identical semantic artifacts through each supported route.
Relocation and transport changes can move provenance while preserving downstream semantic results.
Moved tags cannot change a locked read; unsupported archive entries and content drift are refused.
Every route reaches the same admission record, so no source kind acquires a quieter admission path than
the registry's.
Exercise remote I/O with a local test server/repository, including interruption and retries, without
requiring a public service. Network/subprocess work has bounds and publishes no partial admission.

### 6. implement compatibility reports and the publication gate

Compare canonical surfaces by subject and declaration/rule identity. Report additions, removals, changed
representations, constructors, interfaces, visibility, and dependency context. Distinguish body-only
changes from unchanged source, and harmless spelling changes from semantic changes. If contexts differ,
name the difference instead of attributing all movement to the edited module.

Replace the proposal's unconditional minor classification for additions with the opening record's
conservative policy. Added providers and imported names can break existing consumers; unchanged signatures
do not prove unchanged behavior. A consumer validation mode may compare selection/typechecking under the
two resolved closures, with its claim limited to those consumers. Publication enforces the declared
surface-version policy and refuses an insufficient bump, without describing it as a behavioral proof.

Exit: both compatibility probes run as regressions, including the `E-3012`/`E-1102` split that decides where
a break is detectable; a surface comparison across two dependency contexts reports no change to the edited
module's own declarations; representation/removal changes are breaking;
formatting and alias-only changes preserve semantic output; a body edit is reported with the policy's
stated limit. Publication uses the same differ exposed to the CLI, and a dependency-context change is
visible in both.

### 7. finish the CLI and explicit lock consumption

Put command logic in query/services so JSON and terminal output share one result. Add stable DTOs,
diagnostic codes, and snapshots for resolution explanations, lock drift, source admission, and surface
comparison. Commands have the following proposed effect contract; pin exact flags in slice 1.

| operation | behavior |
| --- | --- |
| `update` | refresh the declared universe, resolve, verify selected content, and atomically publish a lock; `--check` reports the proposed change without writing project files |
| targeted update | release only the selected subject's pins; keep others exact and explain when their constraints prevent the update; broader changes require an explicit broader update |
| `add` | validate and edit the selected module's manifest binding; leave resolution to `update`, avoiding a false claim of atomic replacement across two files |
| bare `diff` | compare current declarations/bindings with the lock and cached evidence, naming drift without silently refreshing the registry |
| surface diff form | compare two explicitly selected module versions/artifacts and their dependency contexts using slice 6 |
| check/explore/selection | load local sources and admitted cached dependencies without solving, network access, lock writes, or creating engine state; missing dependencies diagnose the required update |
| run/explain/graph | consume recorded selections, validate current requirements, and use the existing evaluation/state behavior; normal execution never updates dependency versions |
How much authority an operation has over its own inputs is **one named mode**, not a set of composable
flags. Three booleans describe eight states of which four are meaningful, `--frozen` is a composite alias
for two of the others, and none of the names says what it permits — the defect a named value fixes.

| mode | what it permits |
| --- | --- |
| `current` | may acquire a fresh candidate universe and change the answer; only `update` runs here |
| `pinned` | the lock is authoritative; recorded content may be reacquired at its recorded identity, and nothing may be reselected |
| `sealed` | no network at all; already-admitted local content only, and withdrawal knowledge is whatever the local index holds |
| `strict` | `sealed`, and every authority binding must be project-owned rather than user-level |

The modes are ordered by decreasing authority, so a project can declare its floor once and no command can
quietly exceed it. Refusals name the mode and what it forbade. `update --check` is not a mode: it is
`current` with the write withheld, which is why it stays a flag on one command.

Publication needs surfaces too: proposing a release, showing exactly what an entry would say, and
re-deriving an existing entry from its pinned revision so anyone can audit a registry without publishing
to it. Admission outcomes render as the typed values [the registry](registry.md) defines — `Stale`,
`Unreachable`, `Conflicted`, `Unchecked` — each with its own diagnostic code, because a single `refused`
code makes the difference between an attack and an unreachable mirror invisible.

Normal execution may reacquire missing content at recorded identities; it must not fetch a fresh candidate
universe. A newer registry snapshot does not by itself invalidate ordinary lock consumption. Only update
asks whether a fresh universe changes the answer. Missing/stale bindings, conflicting requirements, and
changed content produce different diagnostics. Keep `fmt` independent of resolution, with its current
root-only write scope.

Exit: a clean machine with a copied lock can acquire and run the same pure entry; an offline second run
hydrates; read-only commands create no database or lock. Revoking an admitted, locked release makes the
next run refuse with the revocation's issuer and the authorized override named, without changing the
recorded selection and without silently substituting another version. Failed updates preserve the old lock bytes,
concurrent writers cannot silently overwrite a changed input, and failures expose useful manifest/index
locations. New registry versions affect update, not ordinary replay.

### 8. migrate callers and close the milestone

Add the explicit workspace/module manifests for first-party domains and `example-domain` using the same
locator third parties use. Retire the implicit neighboring-file import resolver. A standalone `.pi`
document may remain useful for isolated syntax/pure examples, but imported modules require an explicit
manifest environment; there is no remaining fallback search. Migrate importing examples and tests with
source-bearing guidance for old callers.

Resolve `pith.lock`'s naming collision by amending 0043: module resolution owns `pith.lock`, and every
environment lock, including the default, is `<name>.pith.lock`. Update readers, writers, fixtures, docs,
and diagnostics together. Price subject/digest-basis changes explicitly against 0047/0048 and the
declaration mirrors; do not silently retain an obsolete digest-equivalence claim.

Run the complete milestone fixture from an isolated store/state root and temporary registry. It publishes
`example-domain` through the signed-entry path, compares local and registry elaborations, and
exercises both configuration refusals alongside the authority, rollback, and revocation refusals.
The companion pure project adds a transitive dependency, writes a lock, runs in a fresh process, updates
a dependency, reports the diff, and proves appropriate revalidation. Preserve multi-file dependency
diagnostics, alias invariance, and the ABI cutoff through this actual acquisition path.

Run `just ci`, including documentation, determinism, elaborator-digest, dependency/peerhood, lint, and
workspace tests. Record exact results and limitations in `measured.md`; promote records against their
evidence, update README/help/notebook status, and only then mark M-14 complete.

## closure and the next boundary

M-14 closes when authority and routing have executable refusals, every promised source kind reaches the
same admitted-source boundary, transitive resolution is deterministic and explainable, locks survive a
fresh process and engine-state deletion, compatibility reporting states its limits, CLI placeholders are
gone, and the actual example-domain publication witness passes. Registry security's day-one subset closes
with it: publication is two authorities, the trust root has a recovery fixture, the normalizer executes
nothing, admission is a recomputable record, and revocation outlives a lock. No registry/source/lock requirement is
silently moved to M-15 to make the milestone look complete.

Operating a public registry, global namespace governance, independent witness operation and checkpoint
gossip between machines, publication scanning and vulnerability feeds, forge-specific trusted-publishing
adapters, broad archive format support, multiple versions of one subject, an LSP, and remote execution are
separate work. Each deferral is honest only because it leaves a client-side check and a format field
behind; a checksum-only publication path with security fields reserved for later is the outcome
[the registry](registry.md)'s refusal list exists to prevent.
The authority record must name exactly what its local prototype proves before deferring service operation.

Host implementation binding and represented action-to-ActionSpec projection remain the next executable
boundary from 0065. Give that work an explicit opening slice in M-15: an ordinary `.pi` project produces
a real confined artifact, reuses it, and explains a rebuild. Module publication does not discharge that
obligation. The generic builder and its `Opaque` protocol follow that measured execution path.
