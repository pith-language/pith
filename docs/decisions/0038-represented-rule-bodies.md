---
schema: design-doc/v1
id: decision-0038-represented-rule-bodies
title: rule bodies are data in one kernel-facing core ir, with host rules as a permanent declared tier
summary: the typed semantic representation is a single closed, elaborated, canonicalizable ir over the 0026 calculus, whose yield points are the PureStep protocol made explicit; suspension is a re-enterable machine state, never a host closure and never a persisted artifact
kind: decision
status: proposed
created: 2026-06-17
updated: 2026-08-14
tags:
  - language
  - rules
  - ir
  - identity
relations:
  informed_by:
    - research-configuration
    - research-nix
    - research-build-systems
    - research-tooling
  depends_on:
    - decision-0010-typed-pure-language
    - decision-0015-interface-rule-selection
    - decision-0018-termination-and-recursion
    - decision-0022-sync-core-async-scheduler
    - decision-0023-rule-and-cache-identity
    - decision-0026-generic-typed-calculus
    - decision-0031-action-cache-identity
    - decision-0033-consumer-of-action-reuse
  supersedes: []
---

# rule bodies are data in one kernel-facing core ir, with host rules as a permanent declared tier

## context

the design overview says the kernel "should consume a typed semantic representation rather than depend on the source syntax," and 0010 says the language "compiles into the kernel's typed semantic representation. source syntax is not the engine API." both statements name something that does not exist. what the kernel consumes today is rust. `PureStep` is a step protocol — `Need`, `NeedAll`, `NeedBlob`, `NeedAction`, `Complete` — and a rule body is a host closure, `PureRule::start` returning a `PureRuleFrame`, that yields into that protocol and is resumed with a `Resumption`. every rule ever written, which is all of xylem, is such a closure. 0026 settled the type calculus the values speak; it did not settle the form of a rule body, and its unresolved section says so by omission: surface syntax is gated on the calculus landing, but even a landed calculus gives you types and values, not a body.

the gap has three prices being paid now. 0023's revision manifest for rust rules is the digest of the providing crate plus an author-chosen constant, and cache correctness rests on an author remembering to bump `xylem-v2`. 0033's revalidation walk relies on `plan()` depending on nothing but its inputs and the rule's own state, and its unresolved section records that nothing enforces this; the trust extends one paragraph beyond what the engine can check. and the tooling research line wants editors and queries to consume the same semantics the engine does, which is impossible while the semantics live in compiled rust.

the timing argument is M-4. the package and environment libraries will add another library's worth of rules as host closures, and each one hardens the assumption that the kernel's public API is a rust trait. a record argued now is argued against one real library; after M-5 it is a transcription of whatever three libraries happened to do.

the precedent landscape is unusually convergent. Bazel splits Starlark, the authoring surface, from Skyframe's `SkyKey`/`SkyValue` graph, and a SkyFunction that requests a missing dependency returns null and is re-invoked from the top when the dependency lands; Buck2 runs Starlark over DICE, which caches completed values under keys with versions and re-executes deterministic computes on invalidation. Nix persists the fully-evaluated derivation (the ATerm `Derive(...)` record) and never persists the expression language at all. Dhall is the one total language with a persisted form, and its binary standard is a canonical encoding of the elaborated expression, alpha- and beta-normalized before hashing so the digest is invariant to refactoring. GHC is the pattern 0026 already cites: a large surface desugars into Core, roughly nine `Expr` constructors, and everything downstream consumes only that. neither Skyframe, DICE, salsa, nor Nix persists a suspended computation; resumption is either restart (à la Carte's monadic tasks, Skyframe's null return) or a compiler-generated state machine, which is Rust `async fn`: the compiler defunctionalizes the coroutine into an enum, one state per suspension point, with a `resume` function that interprets it. Reynolds named that transformation in 1972: replace a closure with a first-order data structure plus an interpreter for it.

so the precedents say two things at once. the layering (restricted surface over a small keyed value graph, completed values cached under keys) is settled everywhere. and the specific thing pith's protocol already has, a re-enterable body with explicit yield points, is exactly what restart-based systems re-derive the hard way, because a host closure cannot be resumed mid-body, only re-called. pith's closures happen to be resumable because `PureRuleFrame::step` is a coroutine by construction. making the body data keeps that and adds what restart systems cannot have: an inspectable, digestable, checkable body.

## proposed decision

### the representation is one elaborated core ir

there is one kernel-facing representation, not a surface tree plus a separate kernel core. frontends (a surface language, generated definitions, editor tooling) elaborate into it; the kernel never sees a surface form. this is the GHC Core position and the Dhall binary position: Dhall's standard does not define a second, smaller language for persistence, it defines a canonical encoding of the one language, alpha-normalized before hashing.

the ir is an expression language over the 0026 type calculus, closed under the same discipline: a constructor set fixed by record, changed only by a record that amends this one. it is elaborated, meaning typechecked, name-resolved, and with imports resolved to digests of imported declarations, so the ir is self-contained: a body plus its declaration digests fully determines what it means. spans, labels, and comments do not survive into it. it is canonicalizable by construction, structural and alpha-normalized (binder names do not participate in the digest), with no type-level computation, which is the property 0026 already established for the type side and extends here to the expression side. 0023's requirement that computation keys digest "canonical typed inputs" already presupposes this discipline; the body rides the same one.

it is not a normal form. a rule body is not a value; normalizing it is evaluating it, which is the engine's job at run time, driven by the graph. what persists is the elaborated body, the way Dhall's standard encodes lambdas it does not normalize away.

### a rule body is a defunctionalized state machine in the ir

the hard part is suspension. a pith body yields `Need` and is resumed with a value; in the closure world the suspended state is the rust frame's captured locals and program counter, invisible to the engine. in the ir, the yield points are explicit constructs (a request expression whose evaluation produces a `Request`, with a binder for the resumption value), and the suspended state of an application is a triple: the body's digest, the control point inside it, and the value environment of the resumption binders in scope. that triple is data. re-entry is the evaluator resuming interpretation at the control point with the environment, which issues the same steps the closure would have. this is Reynolds defunctionalization applied to the frame: the `CompilePhase` enum in xylem's `CompileEntryFrame`, with one variant per yield and the captured `toolchain_value` and `source` as fields, is already a hand-written version of exactly this shape. the ir makes the compiler do it instead of every rule author.

`NeedAll`'s independence declaration and `NeedBlob`/`NeedAction` are ir constructs at the same level. the four-way step protocol is the ir's effect vocabulary at the kernel boundary; 0019's categories arrive in the ir as the types of what the steps request, which they already are in the protocol (`Request<Pure>`, `Request<Action>`).

suspension state is not persisted across processes. no surveyed engine persists suspended computations, and pith's durable story does not need it: 0024 persists completed attempts and recorded requests, and 0033 revalidates by re-selecting and re-planning. the represented body makes that walk cheaper and more honest (the re-plan is an evaluation of body data under the kernel's own evaluator) but it does not add a new class of durable artifact. an in-flight suspension dies with the run, exactly as a cancelled attempt does today.

### host rules are a permanent declared tier

rust `PureRule` implementations do not go away. they are the analog of native builtins in every comparable engine, and they are how the kernel's own bootstrap libraries and adapters compose rules over machinery that has no ir spelling yet. a host rule registers the same `Interface` and is selected by 0015 on the same terms; the tier is not a tiebreaker, and a represented rule and a host rule matching one interface is an `E-1102` ambiguity like any other. what the tier changes is revision derivation and checkability: a host rule keeps 0023's conservative manifest (provider digest plus author-declared data) because the kernel cannot see its body, and a host rule's `plan()` honesty stays a convention, the trust 0033 already records. the tier is declared in the rule's registration record, visible to queries, so provenance can say which kind of body produced a result, the same visibility discipline `Opaque` applies to effect categories (0019), applied to rule authorship.

### identity stays at the declaration site; revision is derived from the body

0023 splits rule naming into a stable `RuleIdentity` and a cache-invalidating `RuleRevision`, and a representation that derives one of them has to say where the other comes from. identity is not derived from the body. a digest of the body moves with every body change, and identity exists precisely to survive compatible refactors, so deriving both from one digest collapses the split 0023 built.

a represented rule's identity stays where 0023 put it: the module identity and the declaration name, the stable coordinate of the declaration site, carried in the module's declaration table. a compatible refactor (the body changes, the coordinate does not) keeps the identity and moves the revision, which is exactly the split 0023 draws. an interface change is a different semantic declaration and requires a new declaration name, on 0023's own terms. the module identity model across repositories and released versions remains 0023's open question, unchanged by this record.

the revision, by contrast, is derived, and 0023 already promises this: "future pith-language rules derive their revision manifests from canonical typed semantic ir, the semantic revisions of imported modules, and the evaluator abi version." this record commits to the concrete form. a represented rule's `RuleRevision` is a domain-separated digest over the canonical encoding of its elaborated body, the digests of the imported declarations it names, and the evaluator version. formatting, binder names, and spans do not participate, so the only thing that moves the revision is a change to the elaborated body or to something it names. the failure mode 0023 already accepts — false invalidation, when a digest changes without the meaning changing — is bounded by alpha-normalization and by the absence of type-level computation; a change that survives both of those is close enough to semantic that invalidating on it is the conservative behavior the revision contract wants.

0031's action key and 0033's walk are unchanged in shape. the action key already commits to the planned contract's digest rather than the rule revision alone, and 0033 already re-plans at revalidation. what changes is epistemic, along two axes that need separating.

ambient state is excluded structurally. a represented planner evaluates under the kernel's pure evaluator, which has no filesystem, no clock, and no capability but those the steps declare, so 0033's ambient clause is enforced the same way 0022's pure core is structurally pure.

the rule's own declared state is not excluded, and should not be. 0033 permits a planner to depend on its inputs and the rule's own state, and xylem's rules carry exactly that: `SourcesBuild` holds its `toolchain_value`, `CompileAction` holds the header universe. what the representation removes is the third class, the one the header case measured, state that neither the request nor the revision names. a represented body's declared state has two spellings: a request input, which the computation key covers, or part of the body itself, which the derived revision covers. state drifting with no key or revision moving has no spelling left. the claim is therefore narrower than "`plan()` is a pure function of its inputs," and it is still the strongest argument for this representation that 0033 could not make on its own.

### totality becomes a check on data

0018 proposes totality by construction: no general recursion, repetition through structural recursion and the graph. for a rust closure this is a claim about code the engine cannot read. for a represented body it is a property of the ir: if the constructor set contains no general recursion and its iteration constructs are structural folds, a body cannot express non-termination, and the elaborator enforces this by rejection at the boundary, the same place typechecking happens. the cycle-detection hook and the backstop on impure paths are unchanged and stay with 0018; what this record adds is that for represented rules the "by construction" clause is checkable rather than asserted.

### serialization and versioning

the body ir is a new grammar beside `Value` and `Type`, and it gets its own encoding domain and version gate rather than riding `RECORD_ENCODING_VERSION`. two reasons. bodies and values change at different rates, and the existing gate has moved twice for value constructors that no prior record could emit (0026's own account of versions 2 and 3); a body-encoding change that bumps a shared gate would move every durable record's version for a reason that touches only rule registration. and 0023 requires a new digest domain when evaluator semantics change: the body ir version is that domain, participating in the revision digest. the encoding itself follows the canonical codec's existing discipline, length-prefixed, depth-bounded, tag-numbered in its own namespace, with digests domain-separated the way `pith:action-computation:v1` is. whether a type-only serializer change ever needs to split from the value version stays the open question 0026 left; this record only declines to add a third grammar to that coupling.

## alternatives considered

### the kernel consumes surface syntax directly

no elaboration step; each frontend's tree is the API, as Bazel consumes Starlark ASTs.

rejected on canonicalization and selection. 0023 digests canonical forms and 0015 matches canonical interfaces; two frontends spelling one interface differently would produce distinct digests and phantom ambiguities unless the kernel canonicalizes anyway, at which point the canonicalized thing is the ir and the surface trees are inputs to it. Starlark gets away with it because there is one Starlark and one implementation; pith's stated premise is multiple frontends. GHC desugars for the same reason.

### restart-only bodies

à la Carte's monadic task and Skyframe's SkyFunction: a body needing a dependency stops and is re-run from its start when the dependency lands, so no re-entry state is needed at all.

this is the strongest alternative, because it is what every production engine actually does, and it is simpler: no control points, no environments, just re-invocation. rejected on a specific interaction: pith bodies interleave graph requests with local computation over materialized values. xylem's compile entry requests a discovery action, then parses the returned depfile bytes, then builds the compile request from the parse. under restart, every dependency landing re-runs the depfile parse, and a body doing expensive pure work between yields pays for it per dependency. with re-entry, the work happens once. the honest residue is that pith already has restart where it is the right shape: 0033's revalidation re-plans from the request rather than resuming anything, and that stays. restart and re-entry are not rivals here; restart serves cross-run revalidation where the suspension is gone, re-entry serves in-run suspension where the body is live.

### persisted suspensions

serialize the suspended triple (body digest, control point, environment) into durable state, and resume across processes, gaining skip-ahead hydration for mid-flight rules.

rejected. no surveyed engine does this: Skyframe, DICE, and salsa all restart or re-execute, and salsa's durable-incrementality work persists completed values only. that absence is evidence about difficulty rather than authority, so the load-bearing reason is internal: a persisted suspension is a cache entry whose key must cover everything the environment values were derived from, which is exactly the recorded-request-plus-revalidation machinery 0033 already runs. persistence would duplicate that machinery for the fraction of rules that were mid-flight when a process died. rust async's `Pin` problem — a suspended state machine that cannot safely move because its states are self-referential — is the canonical warning about how hostile suspended state is to relocation, and a value-environment triple avoids it only by paying the copying discipline the whole time.

### bytecode instead of an ast

compile the body ir to a linear bytecode, the way go.starlark.net compiles to bytecode, and digest the bytecode.

rejected on the digest contract. bytecode shape is a compiler version artifact; two elaborator releases producing equivalent bodies would need identical codegen to keep revisions stable, which couples cache invalidation to optimizer work. Dhall digests the elaborated expression, not any compiled form, for the same reason. bytecode remains available later as an in-process execution strategy behind the digest, with no record required.

### free-monad instruction streams as the body form

represent a body as `Complete(v) | Bind(step, continuation)`, the freer-monad shape (Kiselyov and Ishi).

absorbed, not adopted. the instruction half is exactly the step protocol and survives as the ir's yield constructs. the continuation half is a function in every mainstream effect library; Kiselyov and Ishi's own `Impure` carries `b -> Freer f a`, and a function is the thing being replaced. defunctionalizing that continuation is precisely the state-machine form chosen above, so this alternative is the ir's evaluation semantics wearing a host-language closure at the recursion point.

### host closures permanently, the representation deferred

the kernel's API stays rust; the overview's phrase is amended out; surface syntax, when it comes, generates rust or interprets to the protocol directly.

the defer option, and it deserves its strongest form: xylem's rules are short, the revision discipline has not yet cost a wrong build, and 0026's calculus is not built, so the ir's type side does not exist yet either. rejected on the growth argument and on two standing commitments. M-4 and M-5 will triple the closure population, and 0033's unresolved section already names the trust problem as a standing cost of closures; deferring the representation defers the mechanism that makes the ambient clause structural and the drift class unrepresentable, while accumulating more bodies to migrate. 0010 and the overview both state the representation as the design; deferral is not a neutral default here, it is a reversal of two records, which is superseding work and should be argued as such if wanted.

## consequences

the kernel gains a second body tier: an interpreter that evaluates ir bodies on the same step machine, registering them beside `PureRule` implementations. `PureRuleFrame` remains the protocol both tiers drive, so the scheduler, the reuse machinery, and 0022's sync core are unchanged: the representation replaces the body behind the frame, not the frame.

a represented rule registers its declaration coordinate (module identity and name) and its elaborated body with its declaration digests. the engine digests the canonical encoding into the revision; the coordinate stays the identity. the revision author disappears for this tier, which is the point. host rules keep 0023's conservative manifest, and provenance records the tier of every rule application.

0033's walk gains enforcement for represented rules: the ambient assumption is structural rather than trusted, and the drift class 0033 measured — state moving with no key or revision following — has no spelling. 0034's discipline — no planner reads a depfile off the filesystem — stops being a convention the compiler cannot check.

tooling gains a queryable body. the values, selection, provenance, and plans the tooling research line wants exposed become readable from the ir rather than approximated from rust, on the same terms as every other derived view.

xylem's rules become the first migration. `ActionRequestFrame` and `CompileEntryFrame` are already defunctionalized state machines in miniature, so the translation is mechanical, and it is the prototype evidence this record needs before it can move to accepted: at minimum, one xylem rule represented in the ir, evaluated by the interpreter through the existing engine, with its revision derived from its body digest.

the cost is the constructor set and the elaborator. the ir cannot be smaller than the step protocol plus the pure expression language it needs between yields, and the expression language is typed in the 0026 calculus, most of which is not built. this record therefore sits behind the same gate as surface syntax, the 0026 calculus landing in `pith-core`. it does not sit behind surface syntax itself: a frontend can elaborate programmatically, and the first frontend is the rust registration API wrapping hand-built ir, which is what migration runs through.

## unresolved

the constructor set of the expression language between yields is not enumerated here. it needs the 0026 calculus constructors that exist and a decision on which total constructs from 0018's open list (folds are the baseline; fixed points, generators, bounded search are open there) become ir constructs rather than graph patterns. that enumeration is the first design task this record gates, and it belongs with the calculus landing.

the evaluator abi version scheme, what counts as a change to evaluator semantics that must move the body digest domain versus a change that leaves all digests fixed, needs the same treatment 0023 gave computation-key domains.

whether `NeedAll`'s batch-with-declared-independence (0029) is the only fan-out construct the ir needs, or whether independent requests in sequence need a distinct construct so a body is not forced to order what it does not mean to order, is open with 0029.

`Observation`, `Mutation`, and `Opaque` have no step protocol yet (0022's unresolved section), so the ir's yield constructs cover `Pure` and `Action` only. the categories' arrival will extend the constructor set by amendment, as 0019's closure rule requires.

the exact canonical form for binders, de Bruijn indices in the encoding or name-based with an alpha-normalization pass before digesting, is an encoding-detail question for the first serializer, with Dhall's alpha-normalization-before-hashing as the precedent either way.
