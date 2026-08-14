---
schema: design-doc/v1
id: decision-0034-discovered-header-dependencies
title: header dependencies are discovered by an action and resolved at plan
summary: run a -MM depfile pass as its own sandboxed action over a declared header universe, parse the captured depfile in a pure rule, and feed the discovered paths back as a request input that the compile resolves to content identities when it plans
kind: decision
status: proposed
created: 2026-06-08
updated: 2026-06-08
tags:
  - dependencies
  - incrementality
  - actions
  - build
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0007-tracked-dynamic-dependencies
    - decision-0015-interface-rule-selection
    - decision-0026-generic-typed-calculus
    - decision-0030-toolchain-closure-as-declared-input
    - decision-0031-action-cache-identity
    - decision-0033-consumer-of-action-reuse
  supersedes: []
---

# header dependencies are discovered by an action and resolved at plan

> exercises the license [0007](0007-tracked-dynamic-dependencies.md) grants for discovered dependencies inside the mechanism it prescribes: every discovery passes through the graph as a tracked, cached action. settles the item M-3's evidence section carried as "header dependencies are declared by hand in the fixture; a `-MMD` depfile pass is the real answer and is follow-up work."

## context

xylem's compile action held the shared header at construction. The two-source fixture declared it by hand, which proved the plumbing and nothing about discovery: one header for all sources, chosen by the caller, invisible to the request. a build library that cannot find out what a source includes is a demo.

the design space is constrained from three sides. an action's declared inputs are fixed before it runs, and the contract digest — half of the action key 0031 commits to — is built from them, so whatever a compiler reports after running cannot retroactively enter the contract of the run that produced it. 0007 licenses discovery but only as tracked requests through the graph with provenance, never as ambient filesystem tracing. and 0033, just accepted, revalidates a consumer by calling `plan()` again and states plainly in its unresolved section that this rests on `plan()` depending on nothing but its inputs and the rule's own state: a planner that read a depfile off the filesystem would make revalidation answer differently on two identical runs. any design that puts post-hoc bytes where the planner can see them breaks 0033 rather than amending it.

there is also a measured fact to build on: 0030's landlock layer makes declared-first safe to start from, because a wrong declaration fails loudly inside the sandbox instead of silently reading a stale header from the host. the fixture now proves that direction too — a source whose include the universe does not offer fails the discovery pass with the preprocessor's own "no such file" diagnostic, and nothing outside the declared set can be read instead.

## proposed decision

discovery is a second action, and the discovered set reaches the compile as a request input.

a `HeaderDiscoveryAction` plans a `-MM -MF deps.d source.c` invocation. its declared inputs are the source and a **header universe**: the `(path, content identity)` pairs a build offers to `#include`, assembled before the run on the same terms as `Toolchain::discover` — host configuration the caller declares, not something evaluation discovers. the universe is staged whole, the preprocessor reads what the source asks for under landlock confinement to exactly what was staged, and the captured depfile is imported into the store and returned as a nominal `xylem.Depfile` value.

the compile entry — the pure rule a build requests — runs discovery, then suspends on the existing `NeedBlob` step to read the depfile it captured, parses it, and requests the compile with the discovered paths as a third request input, a `List<Text>`. the compile action's `plan()` resolves each path against the universe it was registered with and stages the resolved files as the contract's declared inputs.

three properties follow from this shape, and each is load-bearing.

the discovered set never influences the contract of the run that discovered it. discovery's own key covers its planned contract, which stages the universe; the compile's key covers its planned contract, which stages the resolved files the depfile named. the feedback edge is a request between two cached actions, which is the ordinary shape of a pith graph, and 0007's provenance requirement is met because the edge is a recorded dependency like any other.

`plan()` stays a function of its inputs and the rule's own state. the universe and the toolchain are rule state, fixed at registration; the paths arrive as a request input; nothing reads the filesystem or a leftover depfile. revalidation under 0033 re-plans the recorded request and derives the same key, on this run and on any identical one. the parse is pure text processing over bytes the engine already owns, running on the step machine like any other pure body.

the contract still names content, not names. a depfile says `answer.h`; the planned compile says `answer.h` at the content identity the universe maps it to. so a header edit produces new action keys for discovery (the universe it stages changed) and for the compile (the resolved input changed) with no rule revision involved, and a stale object is not served across an engine boundary unless the whole unchanged root hydrates — which is 0024's one-level trust, recorded below with the measurement.

### why the universe, rather than staging candidate headers per source

the alternative that avoids the universe is a first pass that walks the filesystem for headers and stages everything it finds. that is ambient discovery, the thing 0007 names as invalid: it records accidental access as dependency, and the file set becomes part of the build's meaning without being part of any declaration. the universe is the declared boundary — the build says which headers exist, the source says which it includes, and the kernel enforces the difference by refusing the include that steps outside it.

## alternatives considered

### a post-hoc dependency edge recorded after the compile runs

run the compile, read the depfile it wrote, attach the discovered files to the attempt as dependencies the contract never declared.

rejected. the contract is the unit the engine validates, digests, and confines; an edge outside it is unenforced by construction. landlock would have had to permit the headers for the compile to succeed, so the permission would live in the executor's layout rather than the contract, and 0030's kernel-enforced declaration becomes a convention again. it also gives the action key a contract that does not determine the action's inputs, weakening exactly the property 0031 built the key for.

### re-plan the compile with the discovered set inside the same evaluation

after discovery returns, plan a compile whose declared inputs are the resolved headers, without an intervening request.

rejected as a false economy: this is what the entry frame's second `NeedAction` already does, expressed as a request. doing it implicitly would put the discovered bytes somewhere `plan()` can see, which is the 0033 violation this record exists to avoid; doing it as a request is the mechanism.

### fold discovery into the compile with -MMD as a side output

one action writes the object and the depfile; the contract declares the union of the known inputs plus the depfile output; the next run reads the recorded depfile to widen the declared set.

this is the shape a make-style build uses, and it was the working assumption in M-3's notes. rejected for this increment on the sequencing problem it shares with every post-hoc scheme: the first build of a source runs against a declared set that does not yet include the headers (it cannot — nobody has read the source), so the first compile is confined without them and fails. making the first run succeed means either permitting undeclared reads, which gives up the landlock property, or a separate discovery pass, which is this decision. caching the depfile as a side output and re-planning from it later remains available as a follow-up that saves the second process; it must still route the recorded depfile through the graph as this record does, because a planner reading it back from disk is the 0033 break.

### static inference of includes before the run

a library could scan sources for `#include` directives and declare the union without running the preprocessor.

0007 already answers this: inference is a useful producer of declarations and cannot be the enforcement boundary, because inference can be incomplete while hermetic execution fails loudly on a miss. conditional includes (`#ifdef`) make scanning the preprocessor's job; the fixture's conditional case is exactly where a scanner and `-MM` disagree. the universe here is the declared half of that split and the discovery action is the loud half.

## consequences

`CompileAction` stops holding a header. it holds the universe and resolves request-supplied paths; the fixture no longer declares the shared header by hand — it assembles a universe containing a header no source includes and the build works anyway, which is the incrementality claim (the unused header is never staged into a compile) turned into a fixture.

the compile action's interface gains the discovered set as a third input, and the compile entry's public interface does not change: a caller still names a toolchain and a source. discovery and parsing are below the entry, so the two-source build rule and every assertion over it carried forward unchanged except the action counts, which now count discovery too (a cold two-source build is five actions; touching one source re-runs three).

the discovered set is the first value in the repository that needed a list, and the `List` constructor 0026 names in its closed set is now landed for it: `Value::List`, `Type::List(Box<Type>)`, canonical tags, and a decode depth bound shared with nominal nesting, moving the record-encoding version to 3. the slice is documented in 0026's landed-ahead section. `is_type` treats a list as inhabiting `List<T>` when every element does, and an empty list as inhabiting every `List<T>` — `value_type` alone cannot say that, so request-input checking now uses `is_type` rather than comparing `value_type`, which agrees with it everywhere else.

`Request::validate_inputs` now uses `is_type` for the same reason, which is a kernel change in service of a library need; it is the check that section always meant to make, and the empty list is what made the difference visible.

### measured

`crates/xylem/tests/two_source_build.rs`, over the nix-store gcc 15.2.0:

- a cold two-source build against a two-header universe runs five actions (two discoveries, two compiles, one link) and produces a working executable.
- touching `a.c` re-runs three: `a`'s discovery, `a`'s compile, and the link. `b`'s discovery and compile are served from the reusable action index.
- an edit to the shared header, delivered through a changed universe in a fresh engine over the same durable state, re-runs all five — `b`'s compile entry has an unchanged pure key and is served only after the walk 0033 built re-plans its recorded requests against the universe this run registered — and the executable that comes out answers the touched header (exit 87 against the original 81).
- a source including a header the universe does not offer fails the build with the preprocessor's own "no such file or directory" naming that header, having been unable to read anything outside the declared universe. that is 0030's enforcement carrying the discovery claim.

## unresolved

the header universe is rule state, so a universe change between two engines is invisible to a root whose request inputs did not change: the walk trusts an indexed attempt that was reusable when it was published (0024's model, one level deep), and 0033's unresolved section already records that entry whose own dependencies drifted is stale "in the same way for pure and action edges alike". the fixture measures the case where something else forces the root to re-run; the case where nothing does is the trust 0023 places in rule revisions, extended to universe contents, and it stays unenforced for as long as rule bodies are arbitrary rust. closing it means either walking indexed attempts recursively or carrying the universe's content identities into the entry's pure key, and which of those is right is not settled here.

the discovery pass runs the preprocessor a second time per changed source (the compile runs it again inside `cc1`). the `-MMD` side-output shape above would fold the two processes into one for the steady state and is the natural follow-up; it inherits this record's constraint that the recorded depfile reaches the next plan through the graph.

the universe is staged whole into every discovery, so a build with a thousand headers stages a thousand files per source even when a source includes three. the landlock rule count and the staging cost both scale with the universe, and the right fix (per-directory universes, or a compile-commands-style precomputed set) belongs to the build-description layer this milestone does not build.

make-escaped spaces in prerequisite names are not unescaped; a header path containing a space fails to resolve against the universe and the plan refuses. the refusal is loud, and the fix is parser work that waits for a toolchain that emits such a thing.
