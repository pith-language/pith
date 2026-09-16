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

a module is distributed when it is reproducible outside its author's checkout: published, resolved,
its selection and evidence recorded, that exact content acquired in a clean consumer, and the same
program elaborated there. this document holds the contracts that make that true, and the dependency
order that builds them. it covers all four source kinds and the CLI, not only the first registry fixture.

it is the remainder of M-14 after [workspaces](workspaces.md); the milestone stays open until the
obligations below have witnesses.

this is an implementation plan, not an accepted decision. recommended choices are stated so work can
start without rediscovering the alternatives, but the opening records must settle their contracts before
dependent encodings land. [the targeted research](../../research/module-distribution.md) separates external
evidence from deductions about pith's existing code.

[registry integration](registry-integration.md) states how far the registry is a part of pith rather than
a service beside it: one language, one engine and resolver protocol, one admission mechanism, one store,
one artifact, and consumer-written policy. its integration tests belong to the slices below, and the seams
it lists are the cheapest wrong commits in this sequence.

[the module registry](registry.md) holds the registry's own model, on
[the incident and specification evidence](../../research/registry-supply-chain-security.md) gathered for it:
a signed, append-only, metadata-only index whose every cached field is derived from the revision it names.
supply-chain admission is part of this milestone rather than a later hardening pass, and it is small
because three structural choices (source modules, self-authenticating revision pins, and domain-bound
routing) remove most of what a registry would otherwise have to defend. nothing about it is measured yet.

## starting point

the working copy contains the local-workspace slice: manifest grammar, declared subjects, path
dependencies, multiple source files, module-local imports, query integration, and a CLI fixture. slice 0
below has since closed it on evidence: full `just ci`, the acceptance table audited row by row, the two
uncovered rows given witnesses, and the frontend-graph gap that audit found now filled. 0067 is accepted
and the local-workspace plan is reconciled; the evidence is in
[measured](../measured.md#m-14-the-module-system--the-local-workspace-slice).

slice 1's records and grammar are in the working copy: [0069](../../decisions/0069-module-authority-is-consumer-configuration.md)
and [0070](../../decisions/0070-module-acquisition-resolution-and-replay.md) are written as proposed, the
lexer and manifest grammar carry ranges, registry bindings, and the remaining source kinds, and the
refusal codes they allocate exist. slice 2 is implemented against them, on the evidence in
[measured](../measured.md#m-14-the-module-system--the-acquisition-boundary-slice); both records stay
proposed until their own measurements run. do not infer completion from the presence of test names in
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

two of the three probes have been run, they moved the contracts above rather than confirming them, and
[0068](../../decisions/0068-published-surfaces-are-context-bound.md) records the result with its limits.
they live in `crates/pith-loader/tests/probes.rs` and `crates/pith-cli/tests/end_to_end.rs`.

**a published surface is context-bound, and so is a published ABI.** with a consumer's source bytes
unchanged, a public addition to its dependency moves the dependency's ABI, the consumer's ABI, and the
consumer's *interface surface*; what moved in the surface is exactly the dependency's ABI digest,
established by redacting it and finding the two encodings equal. a dependency body edit moves none of them.
so a registry field holding one ABI or one surface per released version means "under the context this
release recorded", never "under every resolution its requirements admit", and `pith diff` must subtract the
imported-ABI region before attributing any surface difference to the edited module.

**an added rule breaks consumers, but not where the earlier objection said.** within one module, two rules
providing one interface are refused at elaboration with `E-3012`, so a publisher cannot build that break.
across modules, the closure elaborates cleanly (`check` passes) and the consumer fails only at evaluation
with `E-1102`. the break is therefore a property of a consumer's whole closure, not of the edited module;
the publisher's own `check` cannot detect it; and no comparison of two versions of one module can prove an
addition safe. conservative classification is forced.

the third probe is not yet runnable: withdrawing an admitted, locked selection and replaying the lock needs
registry machinery that does not exist, so it stays an obligation of slice 3. it belongs beside the other
two because it decides a format field: take an admitted, locked selection, withdraw it in the index, and
replay the lock. the selection must still be the recorded one and the run must still refuse; that fixes the
separation between what a lock freezes and what it does not before any lock bytes exist outside this
repository.

the dependency-context recommendation permits source re-elaboration under a consumer's selected closure.
the published surface is checked under its recorded publication closure; reuse in another closure needs
matching imported ABIs and elaborator revision. a different resolved context is not source tampering.
candidates include the advertised artifact and context in their canonical metadata. a lock records the
selected source binding; resolved frontend artifacts carry the actual context. do not make normal local
body edits require republishing a registry ABI or rewriting immutable dependency pins.

## a forge is an implemented interface, not a table

[the module system](system.md) already settled that forge shorthands are configured sugar over the general
git route, that the host comes from configuration rather than the compiler, and that adding `codeberg:` is
therefore not a language change. this sharpens *what* configuration means; a table of base URLs is the
weaker form of the idea.

a forge is a **declared interface implemented in `.pi`**, with typed options. `github`, `gitlab`, and
`forgejo` are first-party implementations shipped with the distribution as ordinary peers, and a
self-hosted instance instantiates the same implementation with its own options rather than being handled
by a special case. this is 0056's peerhood test applied to routing: the shipped forges come through the
door a third party's would, and if any of them needs a private hook the extension model is missing
something.

two constraints make it work.

**a forge implementation is pure.** it computes locators; it does not fetch. given a reference and its
typed options, it returns where a revision may be found and how a symbolic reference resolves, as values.
the acquisition adapter consumes those as declared inputs and performs the effect, on the caller side,
exactly as 0044 requires. no forge implementation can run during evaluation, and none can be a channel
for ambient network access.

**the general git route needs no forge.** bootstrap would otherwise be circular: a forge implementation is
a module, and modules are acquired through routes. because a plain revision and locator are a complete
route on their own, a third-party forge implementation can be acquired without itself being needed, and
the first-party ones ship in the checkout. any forge shorthand must remain expressible in the general form.

the degenerate case unifies the two answers this design had. a forge with no special behavior is only a
base URL, and its implementation is one line, so `forge acme = git "https://git.acme.internal"` and a
full provider with typed options are the same mechanism at two points. typed options are the reason to
prefer the interface: a provider's authentication style, API flavor, and archive capability become named
fields with declared types rather than stringly configuration a reader has to guess at.

what a forge may never be is dependency-supplied. like routing and admission policy, forge declarations
are consumer configuration; a dependency's manifest cannot introduce, replace, or extend one.

the locator fix in [the registry](registry.md) makes this safer than the shorthand originally could be.
because a locator is a hint outside the identity and content is checked against the normalized tree digest,
a forge's archive endpoint and a clone of the same revision produce the same measurement, so a fast path
is interchangeable rather than a second lock shape. nix's `github:` wart, where the shorthand and the git
URL fetch and lock differently, is not reachable from here.

## implementation slices

the sequence below is the dependency order. each slice can be several focused commits; the checkpoints
are measured outcomes, not a requirement to make a single large change. candidate/lock codecs wait for
slice 1. the differ can be implemented after that decision while acquisition work continues, but its
publication gate waits for the round trip.

slice 3 will likely split: the round trip, the signed-entry publication path, and the key-set and
append-only client are three reviewable pieces. split them by boundary, not by deferring the verification
half: a round trip that admits content on the registry's word first, and grows authorization after, is
the retrofit this ordering avoids.

### 0. close local workspaces on evidence

done. the audit found the gap this step predicted: `tests/edits.rs` compared two direct elaborations, and
the frontend graph tier had no caller outside its own test file, so nothing measured a reusable lookup
over a resolved workspace. `Workspace::project_onto_frontend` is the missing route (source sets published
by content identity, import environments naming each dependency's published surface), and
`tests/workspace_graph.rs` measures the cutoff through it, with a representation-edit control and a
relocation case. runtime revalidation stays the separate CLI witness it already was. two acceptance rows
that had no test gained one: module-local alias scoping with an undeclared transitive import refused, and
the read-only guarantee over all four commands rather than `graph select` alone.

`just ci` passes at 1083 tests across 112 suites. the outcomes are in `measured.md`, linked from M-14;
0067 is accepted and the local-workspace plan records the correction. this closed a slice, not M-14.

what it deliberately did not do: route `pith-query` through the projection. no person-facing command
reuses a frontend computation yet. slice 2's requirement that registry loading feed the same path as local
loading now has a path to feed, which is why the projection was built first.

### 1. record authority, distribution identity, and replay semantics

write focused records for authority/routing and module acquisition/resolution/locks. keep compatibility
as its own record if the two probes require enough argument to obscure either concern. extend the
existing lexer and manifest AST with ranges, registry bindings, and the missing source clauses; keep
source-file imports unchanged. all examples become parser fixtures before subsequent code relies on them.

settle the table above, including numeric-equivalent version spellings, source conflicts through a
diamond, publication of path dependencies (refused), and manifest/index disagreement (refused).
the authority record covers the pinned root key, domain key sets, signed entries, and the consumer-verified
append-only check; take its argument and its threat model from [the registry](registry.md) rather than
restating them. settle there that an index entry is a derived cache of the manifest at its pinned revision,
so a disagreement is a refusal rather than an authority rule, and that a locator is a hint outside the
identity. record the rollback-counter carve-out against 0048 in the same place, before an encoder exists to
inherit the wrong rule.
declare how local module paths and a registry snapshot are selected without upward search. if a workspace
member is selected from another directory, name the workspace root explicitly rather than rediscovering it.

define both meanings currently attached to `pith diff`: workspace/lock drift and comparison of two module
surfaces. preserve bare `diff` for workspace drift; give surface comparison an explicit operand form or
subcommand and pin the help contract.

exit: accepted-for-implementation contracts, three executable probes, grammar fixtures, and a refusal table
with source-bearing diagnostics. the refusal table includes the authority and admission cases, each named
at its own boundary rather than folded into a later generic check. records remain proposed until their
full implementation measurements.

### 2. establish reusable primitives and an acquisition boundary

implemented, on the evidence in
[measured](../measured.md#m-14-the-module-system--the-acquisition-boundary-slice). the paragraph below
is the contract as written before the work; the notes after it record what landed and what stayed with
later slices.

keep `pith-loader::Workspace`'s resolved subjects, source sets, and per-module environments as the input to
elaboration. separate acquiring a module from assigning its aliases and compiling it. registry loading
must feed the same path as local loading, including diagnostic file identity and frontend graph inputs.

introduce a module-domain implementation for constraints, candidates, resolution answers, and locks.
the compiler/query driver links the protocol and module frontend support; it must not link phloem,
xylem, or stele as a shortcut. register the module resolver explicitly on the same engine API available
to peers; its bootstrap declarations cannot require resolving their own source first.

audit small reusable pieces in phloem: `identity::NumericSegments`, the range operations in `constraint`,
`witness` inclusion verification, `archive` parsing, and atomic publication. extract only domain-neutral
mechanics that have two concrete callers. preserve phloem's declaration names and digest behavior behind
its adapters; its candidate, lock, index, solver explanations, and preference values stay domain-specific.
crate names and grouping follow the dependency graph, not a plan to move the whole package library.

unify admission rather than writing a second one. `crates/phloem/src/substitution/` already holds
0042's `Admission`, `admit`, and `Refusal` for binary offers, and a module source's admission is the same
decision over different evidence. one refusal type with a clause per claim keeps T-5's separate claims
separate while sharing the machinery.

exit: a local source still produces identical semantic artifacts through the new boundary; phloem's
conformance fixtures retain their identities; a dependency-closure guard proves frontend code contains
no first-party package/build/system domain. shared mechanisms are exercised through both callers. the
module resolver registers through the engine API peers use, and the solver performs no I/O: removing
network access changes only whether a universe could be acquired, never a solved answer.

what landed. acquisition is a `ModuleStore` (locate, manifest, sources over a store's own location
type) with the local filesystem as one implementation and a registry-shaped `Route` that carries the
subject a registry serves by; `Workspace::resolve` runs any store through the same graph resolution,
admission, and source acquisition `Workspace::load` uses, and a module's provenance is a rendered
location for diagnostics, never a semantic key. a module source's admission runs through the same
machinery a binary substitution admits under: `pith-constraint` holds the interval algebra over an
abstract ordering and the `Refusal<Clause>` envelope, phloem's `Range` and `admit` delegate to it
behind adapters, and the module clauses (subject, version against a written range) refuse at
acquisition under 0070's `E-3063`. the module resolver is a `modules.resolve` pure rule over
constraints, universe, preference, and budget, with typed segment versions, canonical-form identity,
a derived rule revision, and an own deterministic backtracking search whose four outcomes match the
protocol; it registers through `RegisterModuleResolver` on the engine's public call, and the
dependency-closure guard holds every kernel crate's linked dependencies to no first-party domain.

what stayed with later slices. the module lock document and its text projection land together in
slice 3, which is also when the registry client (and with it the remaining admission clauses over
content identity, witness evidence, and policy) replaces the fixture store the tests use. the audit's
other candidates (`NumericSegments`, witness inclusion, archive parsing, atomic publication) each
still have one concrete caller, so none was extracted; the range algebra and the admission machinery
were the two with two. no person-facing command consumes the resolver or the projection yet; slice 7
routes them.

the signed-index slice ran the audit again and found two more with two concrete callers: the
line-oriented tokenizer phloem's lock text had grown and the registry's line format needed, now
`pith-diag`'s `text` module with phloem delegating to it; and the hexadecimal codecs that digests,
keys, and signatures all spell, now `pith-ids`' `encode_hex` and `decode_hex_into` with
`ContentDigest`, phloem's digest reader, and the registry's key spellings sharing one
implementation. the range-token codec stayed apart deliberately: phloem's bounds carry opaque
scheme-spelled versions parsed by the domain, the registry's carry typed segments parsed by the
grammar, and one codec would force a single payload semantics on both.

### 3. complete an exact-version registry and lock round trip

the first routing boundary is implemented. root domain bindings are checked before member or dependency
acquisition; duplicate domains name both clauses, unconfigured domains and registry names refuse without
fallback, and adapters receive a `RegistryRoute` carrying the consumer-selected locator and root key.
its private fields keep unchecked registry clauses out of that adapter route. dependency-supplied authority
is diagnosed before walking that dependency's own requirements. `tests/routing.rs` records adapter calls
for these refusals; `tests/acquisition.rs` consumes the configured route in the existing equivalence fixture.
explicit user configuration, project precedence, the project-owned authority check, and whole-closure
route agreement now have witnesses in
[measured](../measured.md#m-14-consumer-configuration-and-route-agreement). full operation modes, signed
entries, and locks remain open.

the signed-index half has since landed, on the evidence in
[measured](../measured.md#m-14-the-module-registry--the-signed-index-slice): the line format and its
ed25519 signatures, publication that derives every cached field from the manifest it releases and
refuses path dependencies, the client's four checks with rotation and rollback fixtures, the
directory host, and the store whose acquisition pipeline refuses a wrong cache, a wrong tree, an
unreachable index, and a withdrawn release each at its own boundary, with the round trip that
elaborates the same bytes identically from a local directory and through the signed index. what
remains of this slice is the lock document with its text projection, the withdrawal-replay probe
that needs it, and the admission-value calculus; the store serves exact selections until slice 4
wires the version universe through `modules.resolve`.


start with a temporary directory registry and exact selections. publish `example-domain` as an ordinary
manifest module. generate index metadata from the manifest, store its source tree and advertised surface,
and exercise the registry's explicit publishing grant. register no special case for that domain.

publication uploads nothing. `publish` derives an entry from the manifest at a pinned revision, measures
the normalized tree at its subpath, signs the result, and proposes it; admission appends the registry's
time. implement the client's four checks here (the pinned root key over domain key sets, key-set
generations, per-entry signatures, and the consumer's own per-subject append-only record) together with
key rotation and generation-rollback fixtures. retrofitting recovery into a deployed trust root is the
change this ordering exists to avoid.

use one acquisition pipeline: locate, read bounded content, verify transport integrity, normalize the
module tree, measure its identity, validate subject/version/requirements, verify applicable witness
evidence, then expose the admitted source set. never turn a reference or the registry's claimed digest
into measured evidence without reading bytes. close 0044's named checkpoint gaps in this pipeline
(signed checkpoints, consistency against a persisted last-seen checkpoint, and a configured witness policy
whose quorum requirement is stated) rather than shipping a second boundary on the pinned-checkpoint
footing 0044 called a degradation.

normalization executes nothing. no install or publication hook, no native host code, no executable git
filter, no submodule hook, and no editor or agent configuration in a dependency tree may run or take
effect. this is M-14's strongest control, and it holds only while defended by a bounded
file-kind allowlist and an explicit refusal for everything else.

acquisition produces an admission record, not a boolean: content identity, evidence consulted, policy in
force, time context, and reason. it is recomputed rather than stored as a trusted bit, so a later
revocation or an expired snapshot changes the answer without changing the bytes.

implement the module lock value and its text projection together. exact-version resolution is already
a small instance of the module resolver protocol; it may initially have one candidate per subject.
writing the lock remains a caller effect. failed acquisition, validation, or resolution leaves the prior
lock intact. preserve the evidence needed to reacquire content after deleting the engine database;
snapshot identities must have locators or retained data, not only unrecoverable hashes.

exit: the exact same module loaded locally and through the registry has equal declarations, ABI,
interface surface, and represented IR under the same dependency context. a fresh consumer uses the lock
without resolving a new version. a mirror mismatch, rewritten binding, invalid proof, and unauthorized
publication each fail at their own boundary. both domain-configuration refusals run before any network
attempt. an entry signed by an unenrolled key, an entry whose cached fields disagree with the manifest at
its pinned revision, a key set whose generation moved backwards, and a previously seen entry that is now
altered are each refused distinctly: wrong cache, rollback, fork, and an unreachable index are four
outcomes, not one. add a pure executable companion fixture; `example-domain`'s host action need not become executable
to prove the milestone's IR-equivalence claim.

### 4. implement transitive resolution over a scoped universe

acquire a finite, consistent metadata snapshot outside the engine. its scope is the closure of subjects
reachable through eligible candidate requirements, including alternatives that could change the answer,
not merely the versions eventually selected. bound acquisition separately from deterministic solver work;
I/O failure and an absent candidate are different outcomes. a git commit can provide the first registry
snapshot boundary; a local publisher must expose an equivalent stable snapshot or detect a concurrent edit.

implement the module's host resolver with 0040's four inputs and explicit solved, unsatisfiable,
underdetermined, and exhausted outcomes. start with deterministic bounded backtracking at fixture scale;
use exhaustive small-universe enumeration as an independent correctness oracle. do not introduce a larger
solver dependency without a measured need. the phloem implementation is evidence for the protocol,
not a module model to rename wholesale. a search limit is never evidence of unsatisfiability.

resolution time is a declared input, not a wall-clock read inside the solver. `update` takes a fixed
cutoff, records it, and does not drift as time passes. a cooldown is then a consumer policy comparing that
cutoff against the registry's admission time, a rule the consumer writes, not a mechanism the registry
operates. withdrawal is consulted here as data, so a withdrawn candidate is never selected in the first
place.

solve one version per subject across workspace requirements. trace every added constraint back to its
manifest/index clause. dependency-added constraints must also be checked against already chosen subjects.
keep the scheme, preference, budget, candidate provenance, advertised surface context, and resolver revision
in the computation's declared inputs or rule revision as appropriate. lock serialization follows the
successful answer and acquired evidence, with 0041's deliberate budget omission retained.

exit: a release a consumer's cooldown policy excludes is not selected, and the fixed cutoff appears in
the lock;
a diamond shares one version; a case requiring backtracking succeeds; incompatible ranges report
the actual chain; equal-order competing versions refuse unless canonicalization already makes them one
candidate; exhaustion is distinct. input permutations produce the same answer and lock. an unrelated
registry subject leaves the scoped universe unchanged; a relevant candidate or requirement edit moves it.
the resolver reuses and hydrates through the engine, and an input change explains why its answer moved.

### 5. complete registry, Git, archive, and live-path adapters

extend the admitted-source contract through all four source kinds promised by M-14, including archives:
not everything a person depends on lives in a repository, and vendored or generated distributions are the
case that keeps them. implement the forge interface above here, with `github`, `gitlab`, and `forgejo` as
its first three implementations and a conformance fixture that runs all three plus a locally declared
instance through one code path. directory registries are
the test host; git can distribute the same index snapshot, with remote transport behind the adapter.
git module dependencies resolve a tag/reference at update time, record the full commit and module subpath,
and measure the selected tree. locked reads never resolve the tag again. forge shorthand expands to the
general git route without changing fetch or lock semantics.

an explicit git or archive route names one module, not an index. acquire its manifest outside the solver
and present it as a singleton candidate with its declared requirements; check any written version range
against that manifest version. do not scan tags to invent a version universe. its transitive registry
dependencies then use the ordinary metadata closure. this is the necessary acquisition cost of sources
without a separate index, not permission for the pure solver to perform I/O.

archive dependencies check the archive digest and then the normalized tree, supporting a documented
bounded initial format rather than silently accepting unknown members. define behavior for symlinks,
special files, duplicate paths, traversal, submodules, and git LFS pointers. these are the normalizer's
security boundary as much as its format boundary: a symlink escape, a traversing path, and an executable
git filter are attacks, and each is refused by name. an explicit refusal is valid
for unimplemented source-tree features; a plausible but incomplete tree is not.

paths remain live and explicitly unwitnessed. their location is an explicit route; source edits update
frontend inputs without manufacturing immutable remote evidence. publication refuses modules containing
path dependencies. all adapters check a manifest's claimed subject against the requested one.

exit: identical module-relative bytes give identical semantic artifacts through each supported route.
relocation and transport changes can move provenance while preserving downstream semantic results.
moved tags cannot change a locked read; unsupported archive entries and content drift are refused.
every route reaches the same admission record, so no source kind acquires a quieter admission path than
the registry's.
exercise remote I/O with a local test server/repository, including interruption and retries, without
requiring a public service. network/subprocess work has bounds and publishes no partial admission.

### 6. implement compatibility reports and the publication gate

compare canonical surfaces by subject and declaration/rule identity. report additions, removals, changed
representations, constructors, interfaces, visibility, and dependency context. distinguish body-only
changes from unchanged source, and harmless spelling changes from semantic changes. if contexts differ,
name the difference instead of attributing all movement to the edited module.

replace the proposal's unconditional minor classification for additions with the opening record's
conservative policy. added providers and imported names can break existing consumers; unchanged signatures
do not prove unchanged behavior. a consumer validation mode may compare selection/typechecking under the
two resolved closures, with its claim limited to those consumers. publication enforces the declared
surface-version policy and refuses an insufficient bump, without describing it as a behavioral proof.

exit: both compatibility probes run as regressions, including the `E-3012`/`E-1102` split that decides where
a break is detectable; a surface comparison across two dependency contexts reports no change to the edited
module's own declarations; representation/removal changes are breaking;
formatting and alias-only changes preserve semantic output; a body edit is reported with the policy's
stated limit. publication uses the same differ exposed to the CLI, and a dependency-context change is
visible in both.

### 7. finish the CLI and explicit lock consumption

put command logic in query/services so JSON and terminal output share one result. add stable DTOs,
diagnostic codes, and snapshots for resolution explanations, lock drift, source admission, and surface
comparison. commands have the following proposed effect contract; pin exact flags in slice 1.

| operation | behavior |
| --- | --- |
| `update` | refresh the declared universe, resolve, verify selected content, and atomically publish a lock; `--check` reports the proposed change without writing project files |
| targeted update | release only the selected subject's pins; keep others exact and explain when their constraints prevent the update; broader changes require an explicit broader update |
| `add` | validate and edit the selected module's manifest binding; leave resolution to `update`, avoiding a false claim of atomic replacement across two files |
| bare `diff` | compare current declarations/bindings with the lock and cached evidence, naming drift without silently refreshing the registry |
| surface diff form | compare two explicitly selected module versions/artifacts and their dependency contexts using slice 6 |
| check/explore/selection | load local sources and admitted cached dependencies without solving, network access, lock writes, or creating engine state; missing dependencies diagnose the required update |
| run/explain/graph | consume recorded selections, validate current requirements, and use the existing evaluation/state behavior; normal execution never updates dependency versions |
how much authority an operation has over its own inputs is **one named mode**, not a set of composable
flags. three booleans describe eight states of which four are meaningful, `--frozen` is a composite alias
for two of the others, and none of the names says what it permits, which is the defect a named value fixes.

| mode | what it permits |
| --- | --- |
| `current` | may acquire a fresh candidate universe and change the answer; only `update` runs here |
| `pinned` | the lock is authoritative; recorded content may be reacquired at its recorded identity, and nothing may be reselected |
| `sealed` | no network at all; already-admitted local content only, and withdrawal knowledge is whatever the local index holds |
| `strict` | `sealed`, and every authority binding must be project-owned rather than user-level |

the modes are ordered by decreasing authority, so a project can declare its floor once and no command can
quietly exceed it. refusals name the mode and what it forbade. `update --check` is not a mode: it is
`current` with the write withheld, which is why it stays a flag on one command.

publication needs surfaces too: proposing a release, showing exactly what an entry would say, and
re-deriving an existing entry from its pinned revision so anyone can audit a registry without publishing
to it. admission outcomes render as the typed values [the registry](registry.md) defines
(`Stale`, `Unreachable`, `Conflicted`, `Unchecked`), each with its own diagnostic code, because a single `refused`
code makes the difference between an attack and an unreachable mirror invisible.

normal execution may reacquire missing content at recorded identities; it must not fetch a fresh candidate
universe. a newer registry snapshot does not by itself invalidate ordinary lock consumption. only update
asks whether a fresh universe changes the answer. missing/stale bindings, conflicting requirements, and
changed content produce different diagnostics. keep `fmt` independent of resolution, with its current
root-only write scope.

exit: a clean machine with a copied lock can acquire and run the same pure entry; an offline second run
hydrates; read-only commands create no database or lock. revoking an admitted, locked release makes the
next run refuse with the revocation's issuer and the authorized override named, without changing the
recorded selection and without silently substituting another version. failed updates preserve the old lock bytes,
concurrent writers cannot silently overwrite a changed input, and failures expose useful manifest/index
locations. new registry versions affect update, not ordinary replay.

### 8. migrate callers and close the milestone

add the explicit workspace/module manifests for first-party domains and `example-domain` using the same
locator third parties use. retire the implicit neighboring-file import resolver. a standalone `.pi`
document may remain useful for isolated syntax/pure examples, but imported modules require an explicit
manifest environment; there is no remaining fallback search. migrate importing examples and tests with
source-bearing guidance for old callers.

resolve `pith.lock`'s naming collision by amending 0043: module resolution owns `pith.lock`, and every
environment lock, including the default, is `<name>.pith.lock`. update readers, writers, fixtures, docs,
and diagnostics together. price subject/digest-basis changes explicitly against 0047/0048 and the
declaration mirrors; do not silently retain an obsolete digest-equivalence claim.

run the complete milestone fixture from an isolated store/state root and temporary registry. it publishes
`example-domain` through the signed-entry path, compares local and registry elaborations, and
exercises both configuration refusals alongside the authority, rollback, and revocation refusals.
the companion pure project adds a transitive dependency, writes a lock, runs in a fresh process, updates
a dependency, reports the diff, and proves appropriate revalidation. preserve multi-file dependency
diagnostics, alias invariance, and the ABI cutoff through this actual acquisition path.

run `just ci`, including documentation, determinism, elaborator-digest, dependency/peerhood, lint, and
workspace tests. record exact results and limitations in `measured.md`; promote records against their
evidence, update README/help/notebook status, and only then mark M-14 complete.

## closure and the next boundary

M-14 closes when authority and routing have executable refusals, every promised source kind reaches the
same admitted-source boundary, transitive resolution is deterministic and explainable, locks survive a
fresh process and engine-state deletion, compatibility reporting states its limits, CLI placeholders are
gone, and the actual example-domain publication witness passes. registry security's day-one subset closes
with it: publication is two authorities, the trust root has a recovery fixture, the normalizer executes
nothing, admission is a recomputable record, and revocation outlives a lock. no registry/source/lock requirement is
silently moved to M-15 to make the milestone look complete.

operating a public registry, global namespace governance, independent witness operation and checkpoint
gossip between machines, publication scanning and vulnerability feeds, forge-specific trusted-publishing
adapters, broad archive format support, multiple versions of one subject, an LSP, and remote execution are
separate work. each deferral leaves a client-side check and a format field behind; a checksum-only
publication path with security fields reserved for later is the outcome
[the registry](registry.md)'s refusal list exists to prevent.
the authority record must name exactly what its local prototype proves before deferring service operation.

host implementation binding, the next executable boundary from 0065, is now M-16's, through
[0071](../../decisions/0071-host-bodies-bind-through-host-adapters.md) and the component records after
it. the unfinished work in slices 3 to 8 is parked until M-16 is complete; see [the milestones](../milestones.md).
