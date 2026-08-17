---
schema: design-doc/v1
id: decisions-index
title: decisions
summary: chronological record of accepted and proposed architectural choices
kind: decision
status: active
created: 2026-03-11
updated: 2026-08-18
tags:
  - decisions
relations:
  informed_by: []
  depends_on:
    - foundation-problem
  supersedes: []
---

# decisions

decision records preserve the alternatives and the reason for choosing among them. accepted records describe the current direction. proposed records contain a preferred direction that still needs research or a prototype.

when an accepted decision changes, a new record supersedes it. the old record stays in the repository.

## accepted

- [0001: use a generic semantic kernel](0001-generic-kernel.md)
- [0002: declarations denote values and constraints](0002-declarative-semantics.md)
- [0004: first-party without privilege](0004-first-party-without-privilege.md)
- [0008: research design lineages](0008-lineage-research.md)
- [0009: keep first-party domains as peers](0009-peer-first-party-domains.md)
- [0011: separate documentation by role](0011-document-structure.md)
- [0021: a hand-built arena graph with explicit change propagation, not a salsa query DB](0021-arena-graph-engine.md)
- [0028: a first-party sandboxed local executor using landlock and seccomp](0028-sandboxed-local-executor.md)
- [0033: a consumer of an action revalidates by re-planning it](0033-consumer-of-action-reuse.md)
- [0041: the written lock is a text projection of the lock value, and writing it is a caller effect](0041-the-written-lock.md)

## proposed

- [0003: model effects and capabilities explicitly](0003-explicit-effects.md)
- [0005: separate identity types](0005-separate-identities.md)
- [0006: target Linux first without putting Linux in the kernel](0006-linux-first.md)
- [0007: allow tracked dynamic dependencies](0007-tracked-dynamic-dependencies.md)
- [0010: use a typed, pure, terminating declaration language](0010-typed-pure-language.md)
- [0012: revision-pinned plans](0012-revision-pinned-plans.md)
- [0013: managed-object identity](0013-managed-object-identity.md)
- [0014: separate the reproducibility properties](0014-reproducibility-properties.md)
- [0015: select rules by interface match and refuse ambiguity](0015-interface-rule-selection.md)
- [0016: implement the kernel in rust, graph by arena and index](0016-implementation-language.md)
- [0017: structural types by default, nominal by declaration](0017-structural-with-nominal.md) (superseded by [0026](0026-generic-typed-calculus.md))
- [0018: total pure evaluation by construction, with cycle detection and a backstop limit](0018-termination-and-recursion.md)
- [0019: five type-level effect categories, with nondeterminism as a tracked dependency](0019-effect-categories-and-nondeterminism.md)
- [0020: reuse Nix infrastructure as adapters, not as the substrate](0020-nix-as-adapter-not-substrate.md)
- [0022: a synchronous pure evaluator core with an async scheduler at the effect boundary](0022-sync-core-async-scheduler.md)
- [0023: separate stable rule identity from cache-invalidating rule revision](0023-rule-and-cache-identity.md)
- [0024: persist content in a filesystem cas and engine state in sqlite](0024-persistent-engine-state.md)
- [0025: store engine state as normalized relations, not as canonical record blobs](0025-relational-engine-state.md)
- [0026: a generic structural type calculus, with nominal identity, generic uncertainty, and no predicate types](0026-generic-typed-calculus.md)
- [0027: frame retention and garbage collection as roots, two stores, and composable policy axes](0027-retention-and-gc.md)
- [0029: independence is declared by the rule body, not inferred by the scheduler](0029-declared-independence.md)
- [0030: a toolchain enters an action as a declared closure of host paths](0030-toolchain-closure-as-declared-input.md)
- [0031: an action is identified by its request and admitted by its execution facts](0031-action-cache-identity.md)
- [0032: one action is one tool invocation, and a foreign build system is one opaque boundary](0032-action-granularity.md)
- [0034: header dependencies are discovered by an action and resolved at plan](0034-discovered-header-dependencies.md)
- [0035: a link is over a list of objects](0035-link-over-a-list-of-objects.md)
- [0036: a program the graph produced enters an action as content](0036-produced-program-as-content.md)
- [0037: a contract declares whether an exit status is a failure or a result](0037-exit-status-as-a-declared-outcome.md)
- [0038: rule bodies are data in one kernel-facing core ir, with host rules as a permanent declared tier](0038-represented-rule-bodies.md)
- [0039: separate package identity from version and realization identity](0039-package-identity.md)
- [0040: constraints are declared values with domain models, and resolution is a host-rule computation in the graph](0040-declared-constraints-and-resolution.md)
- [0042: binary reuse is an admitted substitution over a lock binding, decided by policy over measured evidence](0042-binary-reuse-as-admitted-substitution.md)
- [0043: the development environment is a value over the lock, and materializing and entering it are caller effects](0043-the-development-environment.md)
- [0044: the first source adapter reads a registry as a caller-side effect, and the witness for a remote binding is a transparency log checked locally](0044-the-first-source-adapter.md)
- [0045: a locked source becomes a built artifact through a closed build procedure, and a realization is the attempt the engine already has](0045-a-locked-source-becomes-a-built-artifact.md)
- [0046: an index line carries the requirement, and a dependency's artifact is a library the graph builds](0046-an-index-line-carries-the-requirement.md)
- [0047: a declaration table in the core, with type identity by coordinate and revision by digest](0047-the-declaration-table.md)
- [0048: every version number stays at 1 until the first release, and pre-release incompatibility is a rebuild](0048-pre-release-version-pinning.md)
- [0049: a pure edge revalidates against the revision its rule is registered at](0049-pure-edge-revalidation.md)
- [0050: a cycle is a live computation key, not a walk over frames](0050-cycle-detection-over-the-computation-key.md)

note: 0013 amends 0005 to add a fifth identity type. 0005 stands; the amendment is recorded in 0013.

note: 0022 refines 0019 by fixing where each effect category executes (synchronous step machine for `Pure`, async scheduler for the other four). 0019 stands; the refinement is recorded in 0022.

note: 0025 refines 0024 by fixing how an adapter represents the records 0024 defines. 0024's choice of storage substrates stands; the representation within them is recorded in 0025.

note: 0026 supersedes 0017 (its structural-default and nominal-by-declaration mechanism becomes one section of the larger calculus) and amends 0010 (settling the calculus questions among 0010's unresolved list). 0017 stays in the repository with a pointer; 0010 stands.

note: 0027 complements 0024 by framing the retention and GC problem 0024 left open. it does not implement GC; it defines the design space (roots, policy axes, cross-store ordering) a later workload-evidence record lands in.

note: 0028 amends 0016 by recording the sandboxing approach 0016 left in "unresolved." 0016 stands; its sanctioned `unsafe`-at-ffi-boundary discipline is what 0028's two `sys_*` modules implement.

note: 0029 refines 0022 by answering where the scheduler's concurrency comes from. 0022 stands; its synchronous core is unchanged, and 0029 records that the width a concurrent scheduler needs is declared by rule bodies rather than inferred.

note: 0031 completes the action half of 0023, which built a computation key for pure applications and left action results uncached. 0023 stands; its separation of stable identity from cache-invalidating revision is what the action key reuses, and 0031 adds only what an effectful computation needs on top of it.

note: 0021 moved from proposed to accepted once the five things its "prototype evidence" section named as unsettled all existed: durable rule-revision identity (0023), action cache identity (0031), persistent graph and cache storage (0024, 0025), invalidation after changed durable inputs, and cache explanations. its own unresolved items remain, which is what `accepted` allows — the direction is chosen, not every question closed.

note: 0032 introduces no mechanism. it states the granularity at which external work enters the graph, which the glossary, the rules-and-graph design doc, requirement U-5, 0019, and 0020 each imply separately and none states. 0019 and 0020 stand unchanged; 0032 is the record a build library can be started from.

note: 0033 completes the consumer half of 0031, which cached an action and kept the pure computation above it out of the index. 0031 stands; its key and its admission test are unchanged, and 0033 adds what a pure attempt has to record and re-derive before it may hold an action edge and still be reused. the first build library is what turned 0031's note into a measurement. it moved to accepted once the walk was built and the M-3 fixture asserted both halves: a second build reusing its root, and a fresh engine over the same durable state hydrating it.

note: 0030 amends 0028 by recording how a toolchain enters an action contract as a declared closure of host paths rather than a single executable blob. 0028 stands; the executable-as-blob model its "unresolved" section named as wrong is resolved here, and reading a nix store path's closure is recorded as the first prototype of the local content-store adapter over a Nix store that 0020 named.

note: 0034 exercises the license 0007 grants for discovered dependencies inside the mechanism 0007 prescribes: discovery runs as a tracked, cached action over a declared header universe, and the discovered set reaches the compile as a request input the planner resolves. 0007 stands; its "static inference" alternative's split — inference declares, hermetic execution fails loudly on a miss — is what the universe and the landlock layer respectively implement.

note: 0035 closes the fixed-arity-two item the link rule's doc comment carried since M-3 began, over the `List` constructor 0034 landed. 0026 stands; the element type keeps nominal identity inside the list, which is what preserves 0015's unambiguous selection now that the link input is no longer positional.

note: 0036 amends 0030 by covering the case 0030's argument does not reach. 0030 stands: a toolchain is not one file, so it enters a contract as a host path and a declared closure. a build product is one file the engine owns, and 0005's separation of content identity from external identity is what makes that a typed sum rather than a path whose meaning depends on its first character.

note: 0037 extends the contract 0003 makes visible to cover how an action ends. it exists because 0032 bars the wrapper that would otherwise answer the question: an action is one invocation of one tool, so what a nonzero exit means has to be declared rather than arranged around.

note: 0038 names the thing the design overview and 0010 call the "typed semantic representation" and that nothing had settled: one elaborated, canonicalizable core ir in which a rule body is data — its yield points are the `PureStep` protocol made explicit, and its suspension is a re-enterable (body, control point, environment) state rather than a host closure. host rules stay as a declared tier with 0023's conservative revisions; represented rules keep 0023's identity at the declaration site (module identity and name) and derive their revisions from a digest of the body. it is the mechanism 0033's unresolved section points at: a represented `plan()` excludes ambient state structurally, and declared state has only spellings the key or the revision covers, so the honesty 0033 trusts becomes enforceable. it sits behind the same gate as surface syntax, the 0026 calculus landing, and not behind surface syntax itself.

note: 0039 opens M-4 by giving 0005's semantic-identity slot its first domain construction: a package is an author-declared name in a domain, stable across version bumps, platform and toolchain changes, and domain-surviving source moves, broken only by rename. a package version adds the coordinates constraints and locks range over; a realization is identified by the computation and content identity the kernel already has, so the package library adds no identity machinery and stays a peer consumer of the build library (0009). a lock entry binds coordinates to source content identity the way flake.lock, go.sum, and Cargo.lock each do, and binary reuse is an admitted substitution over that binding rather than a 0031 cache hit. it is the record that measures the need for records and declared sums in `pith-core`, the way 0015 measured `Nominal` and 0034 measured `List`.

note: 0040 is the record 0039 deferred to: constraints, ranges, preferences, the resolution algorithm's placement, and the explanation model. constraint models are domain-declared values with a shared protocol rather than one generic representation — the CUDF separation with the format refused, so M-6's placement and toolchain domains declare their own models instead of translating into package vocabulary. features are coordinates (0039's variant-dimensions question), ranges are a closed constructor set over the domain's declared ordering, and preferences are declared lexicographic orderings that refuse when they underdetermine, reconciling 0015's refusal to rank with a resolver that must choose. resolution runs during rule evaluation as an ordinary request whose body sits in 0038's host-rule tier — a CDCL search is not structurally recursive, so 0018 bars it from the represented tier — which puts the computation key, incremental invalidation, and provenance behind the result for free. explanations are two layers: the engine's existing invalidation account, plus a solver-carried derivation held to a proof standard that separates "no solution exists" from "the budget ran out," which NP-completeness (Mancinelli, Cox) makes permanent. `Map` is refused a second time: the keyed lookups live inside the host-rule solver, and the value spelling is a sorted list.

note: 0042 takes the line 0039 opened ("binary reuse is an admitted substitution rather than a 0031 cache hit") and the witness shape 0041 predicted for it. a binary is offered against a lock binding and admitted by four legs — binding match, environment match, measured digest match, and policy authorization — with the policy leg shipping at M-4 as local admission of named origins, the honest degraded form of the attestation leg that arrives with key infrastructure. the substitution is never routed through the reusable index, whose admission test asks about a recorded past a foreign binary does not have; the lock records source only, on 0039's ground that realizations recompute per environment; a rejected offer is an explained miss that builds from source and is not remembered, because a negative entry would be state no input names (0038's third class); and the reuse preference 0040 named is refused, because a selection that depended on offers would move the lock when a mirror gained a binary, with no input diff naming the cause. the research note beside it records the disagreement the precedents never settled — optimization or trust boundary — and the record's answer: the perimeter is named in one place rather than inherited from a cache.

note: 0043 closes M-4 by taking the three handoffs it inherited: 0041's lock placement and count, 0042's substitution-record persistence, and 0032's foreign-toolchain seam. an environment is a value over the lock — one resolution's lock held unchanged, plus the realization coordinates the declaration names and the substitution records the environment served — which answers the research note's three-way disagreement by consuming 0041's lock rather than forking a Spack-shaped one or becoming a venv-shaped directory. "reproducible" is scoped to 0014's first two properties: the selection's determinism under recorded inputs and the rendering's determinism, with bit-for-bit reproducibility of the packages left to the build instructions and the rebuild. computing is pure on 0041's terms and entering ships not at all, the print-dev-env position; confinement is claimed nowhere, because a person's shell is not a sandboxed action and saying so plainly is cheaper than letting a reader infer it.

note: 0044 takes the question three records deferred — 0039's threat model for M-4's actual sources, 0041's prediction that the entry's shape picks the log, 0042's naming of the first remote source adapter as what settles the leg — and answers it with that adapter: a registry read, a fetch, and a witness check, each a caller-side effect in the position 0041 put the lock's write, producing values the engine consumes as declared inputs. the threat model names five adversaries and the position is detection after a binding exists: a lying mirror is caught by the measurement, a republisher by the log's witnessed line, a network attacker by the same digest check whatever the transport does, and the malicious publisher and the compromised host are refused as claims pith cannot make. the witness is the transparency log — go's sum database down to the leaf spelling, the lock's binding lines as the log's leaves — verified locally as three legs: a checkpoint pinned by configuration on 0042's terms, an inclusion proof checked by recomputation, and the witnessed digest against the binding's. TUF's threshold shape is rejected on 0042's ground that there is nothing to distribute keys to; attestations are absorbed into 0042's authorization leg rather than witnessed here; and the git refusal lifts at the fetch because git's object model authenticates content intrinsically, so the revision is the witness and the ref survives only as provenance. the fetch as a fixed-output action is named as the future home and deferred until an executor can admit network under a declared output digest. the engine is unchanged, and the universe a registry read returns enters the computation key like any declared input, so a registry that moves moves the universe digest and the lock's diff names it.

note: 0045 takes the one item M-4's statement still owed after 0044 — "no package was built end-to-end from a lock entry into an artifact through xylem" — and makes 0039's load-bearing claim a measurement: a realization is the attempt the engine already holds, the evaluation's artifact identity and the computation that produced it, and the package library constructs nothing. the fetched archive unpacks as a parse rather than a tool — a tar is a deterministic encoding, so 0032's action question does not arise and 0028's allowlist is not widened — with the import caller-side beside the fetch, on the ground that a pure rule cannot publish store content. a package's build is data from a closed procedure the library owns, the stdenv and Guix position against Debian's and Homebrew's script-per-package: a fetched source cannot carry host code the engine could run, and the script position waits on 0038's represented bodies. the procedure runs as one pure rule requesting xylem's compile and link entries, so phloem plans no action, 0009's direction holds over the resolved manifest, and the build reuses on 0031 and 0033's machinery alone — `Reused` on the second build, `Hydrated` in a fresh engine — with the misnamed `Realization` enum corrected to `Serving` for saying how a binding was served. 0042's substitution claim is measured against a real build at last, 0043's environment names coordinates under which realizations now exist, and one dependency edge is deferred to the first index format that carries requirements.

note: 0046 takes the hand-off 0045 recorded as the next round's first item and starts with the ground itself: the index line grows `requires` clauses, so a candidate says what it depends on and the solver's transitive half — written in 0040, never before driven by a real index — runs over a registry answer, one resolve request selecting both packages with the failure derivation naming the requiring candidate. the position is cargo's and alpine's against PyPI's sidecar, read from the primary documents in the new index-formats research note: requirements live where the resolver reads them, so resolution is one read and zero fetches, and the disagreement rule — the index authoritative, an overlapping description refused on mismatch with both spellings named, Debian's precedent over cargo's silence — is stated before the surface exists, with the witness saying nothing because requirements are index data, not bindings. a dependency's artifact is a library, objects plus offered headers, produced by a second package-build interface and refused as both an artifact sum (it widens every consumer) and an `ar` archive (the first non-compiler tool, spending a confinement story to transport what values already carry and collapsing the per-object edges 0031 and 0033 exist for). the dependent's build names its dependencies as (tree, build) pairs, so the edge is the graph's to order, key, and invalidate — and the header half is where the round touches xylem for the first time in six records: the compile entry gains a declared provided-headers input, argued against per-engine universe composition (the fixture-fabrication trap 0045 named in advance, and a fact invisible to every computation key), with 0034 extended rather than amended and 0009's direction intact. the measured asymmetry is the claim the fixture exists for: republishing only the dependency moves the dependent's artifact while the dependent's own compile is served — three actions where five ran — and republishing a package nothing uses moves the universe digest and no artifact. measured on linux in the same VM devshell 0045's gated runs used, with the darwin host's own limits recorded there.

note: 0047 amends 0026 twice. it builds the declaration site 0026 kept deferring — the site whose absence left `Type::Nominal` carrying a bare name, so any value claiming a nominal name inhabited any interface declaring it whatever its representation held — and it audits the constructor set 0026 closed, removing `Map`, `Option`, `Result` and the five effect categories, the last of which were 0019's ir types read as value types. 0026 stands; its constructor list is corrected the way its own closure rule requires, by a record. the amendment is a net shrink with the declaration table added, and the five uncertainty constructors stay in the set unbuilt, each gated on the subsystem that would read it, on the ground that landing one without its reader is what produced the `Type::Nominal` history in the first place.

note: 0048 generalizes a rule the tree already applied to one constant and amends the two records that plan version movement against it. `RECORD_ENCODING_VERSION` reached 5 and was returned to 1 in a9f0b8b; 0026 narrates those four bumps in the present tense and was never revised, and 0047 planned two more. 0048 states one rule for every version number — pinned at 1 until the first tag, with pre-release incompatibility answered by discarding and rebuilding — and fixes the post-release rule per class of artifact rather than per constant: derived caches may always discard, committed user data must migrate, and digest domains stay under 0047's Dhall rule. 0024 and 0025 stand; the "before release" they defer to now names a condition that can be checked. it also moves `publish = false` to the workspace so the pre-release claim the pinning depends on is enforced by cargo rather than by convention.

note: 0049 completes the pure half of 0033, which revalidates an action edge by re-selecting and re-planning it and left a pure-pure edge trusting its recorded key. 0033 stands and its mechanism is unchanged. the gap it leaves was a K-9 violation reachable in the ordinary case: a revised rule mints a new computation key, so the old key is orphaned rather than superseded, its recorded attempt stays the latest reusable one under it, and the consumer hydrates a result derived from a rule body the engine no longer has. it was reachable across the library boundary the central claim rests on, because xylem derives every revision from one shared literal while phloem derives its own from its own manifests, so a xylem bump left phloem's package builds hydrating superseded xylem results. the fix needs no record-shape change: the recorded key already carries the dependency's rule identity and revision, and the sqlite adapter already stores both as columns. transitive validity, a requirement edit re-scoping K-9 to quantify over the rule set, and an executable from-scratch-consistency harness are all left open there.

note: 0050 replaces the cycle predicate's structural scan with a set lookup over the computation digest the engine already derives per request. 0018 stands and this is the mechanism it leans on: cycle detection is the real bound on pure recursion, so it runs on every request, and it was O(depth) deep comparisons per request — quadratic in a build's dependency chain, because `Need` pushes onto the requesting chain rather than opening a new one. the equivalence is argued in both directions rather than assumed, and the case where a digest could match while the old scan did not is excluded by 0015's refusal of ambiguity rather than by a new invariant. it also lands the repository's first benchmark, which is what makes the claim a measurement: 850 ms to 60 ms at sixteen thousand frames, with the two depth-free shapes unchanged as the control. the guard that should have caught the `HashSet` this record wanted is recorded there as covering `HashMap` only.
