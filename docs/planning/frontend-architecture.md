---
schema: design-doc/v1
id: planning-frontend-architecture
title: the frontend architecture
summary: one boundary derived from content addressing, three graph nodes with an interface cutoff, no in-process incremental engine, and the language server that reads the HIR
kind: planning
status: draft
created: 2026-08-18
updated: 2026-08-21
tags:
  - planning
  - language
relations:
  informed_by:
    - research-language-frontend
    - research-tooling
    - research-diagnostic-spans
  depends_on:
    - planning-language-frontend
    - decision-0021-arena-graph-engine
    - decision-0022-sync-core-async-scheduler
    - decision-0053-parse-diagnostics-carry-their-source
    - decision-0057-the-rule-index
  supersedes: []
---

# the frontend architecture

this is round three of [the language frontend](language-frontend.md).

## one boundary, and it is derived

a frontend artifact may be a graph node if and only if every byte it derives from is reachable from a
`ContentId` that existed before the node's key was computed. otherwise it is frontend-local.

this is forced rather than chosen. `Engine`'s only byte entry is `put_blob(&mut self) -> ContentId`, and
`PureComputationKey` digests identity, revision, interface and encoded inputs — there is no key over "the
current text of file 7". and LSP puts the buffer on the other side of the same line: on `didOpen` the
document's truth is the client's and the server must not read it from disk.

the set of bytes an editor is authoritative over is exactly the set pith cannot key on.

so elaboration of a saved module is a pith computation, the editor's inner loop is not, and one elaborator
library serves both. that is the strongest available reading of U-3's "the same parsed and typed model as
evaluation": one elaborator called from two drivers, and additionally the same durable cache for every
module the user is not editing.

pith has no mutable input node, and that is the deeper fact behind it. salsa has
`set_file_text(file_id, text)`, a stable key whose value is replaced. pith admits bytes only through a
digest, so a keystroke does not invalidate a node, it creates one. relatedly, rust-analyzer's durability —
the mechanism that makes whole-world-from-source viable interactively — is a property of a mutable input
slot, so pith has no analogue and cannot easily acquire one. this is not a tuning problem; it is what
content addressing means.

## the graph tier

three rules, identical input lists, three distinct nominal outputs:

```
interface-of : (Source, ImportEnv) -> ModuleInterface
bodies-of    : (Source, ImportEnv) -> Bodies
index-of     : (Source, ImportEnv) -> Index
```

that the frontend needs a nominal type per rule to stay unambiguous is the fourth independent instance of
the pattern stele found for its three text renders and xylem records for `E-1102`. it is also the evidence
the frontend takes no privilege: it registers through its own extension trait, the convention
`BuildEngine::register_xylem` already uses and 0056 refused to promote into a kernel facility.

`Source` is `List<{path: Text, content: Blob}>`, path-sorted and canonical — byte-for-byte phloem's
`SourceTree`.

a module is a directory of `.pi` files, not one file, and that forces the shape: `PureStep::NeedBlob` takes
exactly one digest, `Resumption::One` returns exactly one value, `NeedAll` takes requests rather than blobs,
and no step reaches `ContentStore::get_tree` at all. so a *k*-file module is a *k*-yield frame with *k*
serialized store round trips and no way to declare them independent. putting the file list in the input
value is what makes the key cover it, so adding a file moves the key without a directory scan reaching the
graph — which also removes a nondeterministic-directory-listing hazard. the alternative, one file per
module, is cheaper and defensible, and it is rejected on the corpus: stele's `types.rs` alone holds ten
declarations and twenty-six structural types, and xylem as one file is about four hundred lines.

`ModuleInterface` is a shallow record `{ identity, tier, abi }` rather than an opaque blob, so a query
answers "is this module host or represented" without fetching anything. that is `.rmeta`'s lazy
random-access lesson reduced to the one distinction that matters: what something enumerates lives in the
value, what something decodes lives behind a digest. the tier being decided by the resolution artifact and
both tiers producing one `ModuleInterface` is what makes a migration unable to hide a module — nix's
`derivation` migrated out of the native tier and fell out of the catalogue both language servers consume;
here the catalogue is the declaration.

the key property is that `encode(ImportEnv)` reduces to a sorted list of `(binding, module, ABI digest)`
triples and nothing else — 32 bytes per import, no import's bodies and no import's doc text anywhere. so
editing a rule body in `xylem.pi` moves `bodies-of(xylem)`'s key, produces a byte-identical ABI digest,
leaves phloem's `ImportEnv` unchanged, leaves `bodies-of(phloem)`'s key unchanged, and leaves every cached
`phloem.package-build` computation reusable. that is the ijar and reference-assembly property, and it is
the change-pruning bazel's own `BzlLoadFunction` comment says it lacks for want of "an interesting equality
relation … able to ignore benign whitespace" and buck2 abandons with `EqualityBehavior::AlwaysUnequal`.

pith is the first system with the equality relation to get it exactly. the driver must sort the import list
before requesting or one module gets two keys, which is a proptest obligation on the same canonical-order
discipline records and sums already carry.

`Index`'s digest participates in no rule revision and is no dependency of `bodies-of`. it is a sibling.
otherwise a doc comment moves a `RuleRevision` and invalidates every cached compile.

the elaborator's own revision is derived, not hand-typed. this is the design's other soundness
requirement and it was nearly left open. `interface-of` is a rule; a rule's revision manifest today is
`u32le(BodyRevision) ‖ encode_interface(interface)`; all twenty-two rules in the corpus sit at
`BodyRevision(1)` and nobody has ever bumped one, while xylem's compile entry demonstrably changed body
shape. a stale `BodyRevision` on `interface-of` is not a slightly stale editor — it is a typechecker whose
results are reused across a semantic change, from a shared cache, on another machine, by a different
binary. `RuleRevision::of_manifest` is public, so the manifest becomes
`u32le(ELABORATOR_SEMANTIC_VERSION) ‖ encode_str(frontend version) ‖ IR body-encoding version ‖
encode_interface(interface)`. that is 0038's evaluator-ABI-version question arriving with its first
consumer. the weakest acceptable form is one constant plus a CI gate that fails when any file under the
elaborator crates changes without it moving — which is rustc's `eval_always`, an author-maintained honesty
annotation, and it should be labeled as one.

deep values must not ride the kernel codec. `MAX_NOMINAL_NESTING = 32` is checked on decode at every
nested constructor and not on encode, so a deep value encodes fine and then fails to decode. the kernel's
value codec therefore serves types, declarations, interfaces and shallow records, and every structure with
unbounded depth — the IR body, the interface surface, the index — is a frontend-owned byte format carried as
`Value::Blob`. three digest domains, version-gated and pinned at 1 pre-release. two consequences: every
inter-node payload is 32 bytes, so key derivation is microseconds rather than proportional to file size; and
the syntax tree is never a value and never digested.

a user error is data inside a `Complete` value; `Failed` means a frontend bug. the value is
`{ rules, incomplete: [{coordinate, diagnostics}], diagnostics }`; a rule whose body did not typecheck is
absent from `rules` and named in `incomplete`, and registration registers what is there. a module edited
into a broken state has its rules temporarily missing, which is the correct semantics, and its diagnostics
hydrate as an ordinary reusable result rather than living on a `Failed` attempt.

no engine entry point admits `NeedBlob` today without an executor. `evaluate_pure` rejects `NeedBlob`
and `NeedAction` with `E-1206`, and every other entry point requires a runtime, an action policy and an
executor. so `interface-of`, whose whole body is one `NeedBlob` per file, cannot be evaluated by
`evaluate_pure`, and `pith check` — a command that performs no external work — would have to construct a
tokio runtime and an executor to typecheck a file. two answers, and the proposal takes the second now and
names the first: a new `evaluate_with_content` entry point admitting `NeedBlob` and refusing `NeedAction` is
better and answers `open-questions.md`'s "should the synchronous pure step machine be unable to even name
effectful steps at the type level" in the direction of three step tiers; constructing the frontend engine
with a refusing executor and a deny-all policy is shippable today and needs no kernel change. the frontend
is the first consumer that wants `NeedBlob` without `NeedAction`, and it is therefore the named reader for
that open question, the way it is the named reader for `Unknown`.

## where incrementality lives

tier one is durable and cross-process: the ABI cutoff on `interface-of`, the only mechanism in the whole
prior-art survey that reliably pays for itself. it is also the one measurement that decides the tier, and if
it fails the graph tier collapses into a single host rule that the CLI and the server both call, with the
local layer untouched — a graceful failure and a consequence of the boundary rather than an addition to it.

tier two, in-process, is nothing, deliberately. no salsa, no query engine, no memo table beyond a map from
module to elaboration. on a keystroke, re-parse and re-elaborate the edited module. sorbet is the control
case and the strongest one: no incremental engine at all, around 100,000 lines per second per core, and one
of the fastest language servers in production. pith starts closer to sorbet's shape than a general-purpose
language does — no subtyping, no rows, no higher-kinded types, no type-level computation, no inference
pass, an interface of two fields, and selection as one map lookup.

this is the design's central latency bet, and it is measured. the spike [milestones](milestones.md) put in
front of M-10 ran it: two hundred modules generated from stele's declaration shapes — 35 declarations and
8 rules each, about 185 lines, 36,961 lines in all, up to three imports resolved per module — with a
throwaway lexer, recursive-descent parser and elaborator deriving per-declaration digests and the module
ABI digest as [the module surface](module-surface.md) specifies them, timing the keystroke path against an
import environment holding the other one hundred and ninety-nine modules. on an AMD Ryzen AI MAX+ PRO 395,
re-parse plus re-elaboration of the edited module is 105.6 µs at p50 and 117.3 µs at p99 over two thousand
samples, the same whether the edit holds the ABI digest or moves it; parsing alone is about a quarter of
the path. the whole world elaborates cold in 21.9 ms, about 1.7 million lines per second against sorbet's
hundred thousand, on hardware faster than the machines that figure was measured on. against the 50 ms
bracket the edited-module path holds a factor of about four hundred and twenty, and the cold-world pass —
what re-elaborating everything per keystroke would cost — fits inside the bracket too, which is headroom
the design does not need and should not spend. the bet holds.

two boundaries on the number. the spike elaborates declarations, signatures and shallow pure bodies, not
the represented bodies M-12's elaborator will check, and it interns nothing, so every nominal restates the
full path the way stele's `FileSet` does before a type pool amortizes it. the first boundary reads as
optimistic and the second as pessimistic, and neither moves a factor of four hundred. one correction stays
beside the measurement: an earlier draft of this section put forty thousand lines at sorbet's rate at
"about 0.4 ms per core", which is off by a thousand — forty thousand lines at a hundred thousand per
second is about 0.4 s, and the figure the argument wanted was the edited module's two hundred lines at
about 2 ms.

salsa is not merely unpersuasive here, it is refused by an accepted record.
[0021](../decisions/0021-arena-graph-engine.md) rejects it on three named grounds and is `accepted`. the
grounds are engine-specific — five identity types, capability propagation per edge, category-dependent cache
contracts, a macro-coupled extension surface — and a parser has none of them, so a record may argue that a
frontend-local query layer is a different question. what it may not do is treat the matter as open. and 0021
constrains the frontend in two ways that cost a paragraph and save a CI failure: `HashMap` and `HashSet`
are forbidden, enforced by an `xtask` check that walks every `.rs` under `crates/`, so interners,
name-to-module maps, the type pool, the reverse-reference index and the diagnostic accumulator are all
`IndexMap`; and nontrivial external crates sit behind pith-owned trait boundaries and never in public
signatures, so a syntax-tree type in a public elaborator signature may not be an upstream crate's type, and
`serde` is unavailable for the frontend's three byte formats.

tier three is the fast-path predicate, and it is exact. after re-elaborating the edited module, compare its
new ABI digest to the previous one; if equal, no reverse dependency needs re-elaboration. that is a proof
rather than a heuristic, because the ABI digest is exactly what a dependent's elaboration consumes. sorbet
approximates this with an "edits touching fifty or more files" threshold; rust-analyzer approximates the same
need with durability, which pith structurally cannot have. pith computes what both approximate, which is
why salsa's headline mechanism is not missed.

what is not there: no node per parse, per declaration or per expression. under the durable adapters one
graph node costs roughly 0.87 ms of serialized fsync and one blob admission roughly 0.89 ms, so a 50 ms
budget buys about fifty-five nodes before any work — and content addressing makes it worse than slow, because
a keystroke mints a key rather than invalidating one. an all-memory engine is microseconds per node and that
rebuttal is correct; it is still refused, because buying the inner loop that way costs two new store
adapters, one of them eleven trait methods under a generated conformance suite where a bug is silently wrong
reuse, plus a size-budget retention mechanism and a per-debounce hash whose cost scales with file size rather
than edit size. that is a great deal of new mechanism to make the graph serve a loop a map serves.

a retention consequence, and it is an amendment: 0027's aggressive root set is the reusable index, which
is per-key. each save mints a new key with exactly one attempt, so each save's attempt is the latest reusable
attempt *for its own key* and is a permanent root. two hundred modules at two hundred saves a day is roughly
six hundred new durable attempts a day — irrelevant in fsync, real in state and blob growth. 0027 needs a
retention axis scoped by rule identity or by tier, not only by key. a format-on-save editor doubles the
rate, and nobody has priced that interaction.

## construction

engine construction is from a program. a frontend engine holds host rules only — the three above plus
source and lock adapters — so its population is genuinely append-only. an evaluation engine is
constructed from its output, so a program change builds a new engine sharing the same content store and
state store rather than mutating a table. registration is a map insert per rule, so this is cheap, and the
precedent is uniform: rust-analyzer rebuilds its crate graph, gopls takes a new snapshot, swift rebuilds into
the module cache. never "mutate the table".

this answers 0057's unregistration question without inventing unregistration — 0057 names the collision by
name ("a language server reloading a domain, or the represented bodies of 0038 arriving from a file that
changed, has to say what happens to the bucket and to the ids in it, and that is a record, not a method") and
convergence on the two-engine construction is not a substitute for writing it. the cost is that nothing in
the type system prevents registering on a live evaluation engine, and `RuleId` fails silently as `None`
across instances, so a long-running daemon that gets this wrong produces a mystery rather than an error. that
is a documented invariant and a test, not a mechanism.

## the language server

rust-analyzer's two-engine split, with the pith engine playing the `cargo check` role and gopls's
disk-file/overlay distinction as the naming.

the server consumes the HIR, never the IR.

what is already built and unusual: `EngineQuery::select` answers "which rule serves this request, and is it
ambiguous"; `plan_action` answers "what would this contract be without running it"; `explain_invalidation`
answers "why is this stale". those are three editor features no other language server has, and they exist.

completion after `xylem.` is the interface-source chain: build `interface-of`'s key, call the reusable-attempt
read — which takes `&self`, needs no engine and no scheduler, and costs microseconds — decode, fetch; on a
miss, read the file from disk, parse, elaborate with an empty environment, take the interface. nothing is
written: no blob, no attempt row, no fsync, and the buffer's bytes never leave the process. it works
identically when a module is a pure rust host crate, because that path reads only `interface-of`.

completion of a *request* is idris's proof search at depth one — filter the import environment's rule
signatures for buckets the in-scope bindings can fill. no search, and no depth bound to rot.

four things the engine structurally cannot serve, and a record should say so rather than let them be
discovered:

unsaved buffer state, for the reason above. partial and invalid syntax, because the parser's invariant is
that parsing never fails and produces a tree plus errors, while the arena has only `Complete`, `Failed` and
`Cancelled` and cannot say "a tree with holes". names and positions for things pith identifies structurally
— a structural alias has no use-site spelling at all, structural types have no name to go to, and nothing
answers "which requests would select this rule", since the index is interface-to-rules. and rename that
preserves identity, because a declaration's identity is its coordinate.

rename is therefore offered, performed, and priced. renaming `xylem.Object` moves its declaration digest,
every interface naming it, every affected revision manifest, and every recorded computation under those
rules. the count is computable exactly, because the revision derivation is a pure function of encodable
data, so the edit is accompanied by "renaming `xylem.Object` moves 3 rule revisions and invalidates every
cached computation under them; this is a semantic change, not a refactor." no surveyed language server
computes that number, LSP has no channel for it, and a message is the honest maximum — refusing a legal edit
is worse.

a fifth item, and it is 0053's own open question arriving with its reader. hydrated diagnostics keep
severity, code, span and message and drop the source, so a durable diagnostic has neither a URI nor a range.
frontend diagnostics are values carrying their own source identity — a record holding the source blob's
digest plus offsets, per label — and the render boundary fetches each blob and builds the diagnostic with a
source file per label. this fixes three things at once: cross-file notes work in the terminal as well as the
editor, which is where they are currently inexpressible since `Note` has no file and `SourceId` is read by
nothing; file identity becomes content identity, which survives a process where a table slot does not, so
`SourceId` is deleted rather than revived and `Span` stays eight bytes; and hydrated frontend diagnostics
re-render with their snippet. this is an amendment to 0053's reasoning, not a contradiction of it: 0053
rejected attaching text at the render boundary on the premise that by then the path and the text are gone
unless something carried them, and 0053's rule — attach when the producer holds the text — is right. for
this producer the text is never gone, because it is content-addressed. hydrated *engine* diagnostics still
point nowhere and this does not fix that.

evaluation diagnostics get spans with no kernel change. the elaborator emits a side table mapping
`(body digest, control point)` to `(module, span)`, never digested and never in the IR. the interpreter is
frontend-owned host code and holds it, so when a represented body's guard fails the interpreter — not the
engine — attaches the source and returns the error, and the CLI renders `stele.pi:41:9` with quoted bytes.
that keeps 0053's promise that "engine evaluation diagnostics carry `Span::none()` today and will until the
surface language gives an evaluation something to point at", keeps the IR span-free, and grows no
control-point field on the diagnostic type. it is also a contradiction the notebook currently contains and
nobody listed: U-3 wants editors on the same model as evaluation, 0038 strips spans from the only thing the
kernel evaluates, and no record proposed the mechanism that reconciles them.

warnings need an `Ok` path. `PithResult<T> = Result<T, DiagnosticSink>` carries no sink on the `Ok` arm,
so a successful parse cannot return warnings. the syntax and elaborator crates return their own
value-plus-diagnostics pairs internally and lower to `PithResult` only at the boundary. an editor with no
warnings is not an editor.

interning is forced. 0047's deep embedding means a nominal carries its whole declaration body, so
stele's `FileSet` restates `nominal → list → record → sum → record` at every occurrence, and a
type-at-offset table storing a type per span would be megabytes for one module — go's measured 300x arriving
in the frontend. a per-module type pool makes stele's about thirty entries. and `is_type` allocates a
`String` per nominal check through `coordinate.spelling()`, so coordinates intern to integers and comparison
is integer comparison, which is sorbet's exact trick.

one honest regression: find-references finds `.pi` references, not rust use sites, and phloem reaches
xylem's declared types, value constructors, field-name constants and request builders at twenty-plus places.
that is worse than running rust-analyzer alone on today's tree, and it lasts as long as the host tier, which
every surveyed system says is forever. whether a combined view is buildable is unasked.
