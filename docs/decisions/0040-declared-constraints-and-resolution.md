---
schema: design-doc/v1
id: decision-0040-declared-constraints-and-resolution
title: constraints are declared values with domain models, and resolution is a host-rule computation in the graph
summary: constraint sets, ranges, and preferences are values in the 0026 calculus, declared per domain over the ordering 0039 already made the domain's; resolution is an ordinary request whose solver body sits in 0038's host-rule tier, so computation keys, invalidation, and provenance apply to it unchanged; solver explanations travel as values beside the engine's own
kind: decision
status: proposed
created: 2026-06-29
updated: 2026-07-27
tags:
  - packages
  - constraints
  - resolution
  - identity
relations:
  informed_by:
    - research-dependency-resolution
  depends_on:
    - decision-0009-peer-first-party-domains
    - decision-0015-interface-rule-selection
    - decision-0018-termination-and-recursion
    - decision-0021-arena-graph-engine
    - decision-0024-persistent-engine-state
    - decision-0025-relational-engine-state
    - decision-0026-generic-typed-calculus
    - decision-0032-action-granularity
    - decision-0033-consumer-of-action-reuse
    - decision-0038-represented-rule-bodies
    - decision-0039-package-identity
  supersedes: []
---

# constraints are declared values with domain models, and resolution is a host-rule computation in the graph

> the record 0039 named: "constraints, version-range semantics, preferences versus hard requirements, the resolution algorithm, and the explanation model are their own record against the dependency-resolution research." it also carries 0039's two deferred questions, variant dimensions and `Map`, and the open question under first-party domains about when resolution runs relative to rule evaluation.

## context

0039 settled what a constraint ranges over, a package version, and left everything else here. the research record already separates the data model from the solvers, and the open questions list asks the bigger version of that: one generic constraint representation with multiple solvers, or several domain-specific models — a question whose section also names toolchains and machine placement, so whatever is decided binds M-6 as much as M-4.

the precedents disagree about more than implementation. they disagree about what the problem is.

PubGrub (Dart's solver, and uv's) treats version solving as CDCL over terms that are version ranges rather than booleans, and its derivation graph is the strongest failure-explanation mechanism anyone has shipped: "because foo <1.1.0 depends on a ^1.0.0 ..." generated from the proof that no solution exists. it is silent, almost embarrassingly so, about choosing among valid solutions — its decision heuristic (latest version of the most-constrained package) is described in its own documentation as having "likely room for improvement." Molinillo (Bundler, CocoaPods) is a backtracking state machine whose conflict objects list requirements and whose errors stop there; it finds solutions but explains poorly. libsolv (ZYpp, dnf, and conda's libmamba) goes the other way: dependencies, weak dependencies, and preferences all become SAT rules with weights and priority levels, and its decision lists explain every install and erase — but the explanation is an account of a search, not a proof, and the weights are policy baked into the solver. the MISC competitions, on the MANCOOSI project's CUDF format, are the strongest precedent for what this record most needs: CUDF "only defines what a solution is," any solver can compete over the same instance, solutions are ranked by a criterion written as data (aspcud's criteria vocabulary: removed, changed, notuptodate, lexicographically combined), and the competitions judged solvers complete first, powerful second, efficient third — an explicit ordering of values. Spack's concretizer solves versions, compilers, variants, and virtuals together in answer set programming with optimization statements, which is the closest precedent to the joint problem pith actually has, and its cost is visible in the same paper: optimization is policy, and the policy lives inside the solver. 0install is the caution against all of them: its author first used a pseudo-boolean cost function over a SAT encoding, found that most combinations had similar costs and that ties produced nondeterministic selections, and replaced global optimization with component-at-a-time satisfiability queries — pick the best root that appears in any solution, then the best of the next, recursively — buying determinism by giving up global optimality. and Mancinelli et al., with Russ Cox's reduction beside them, bound the whole territory: dependency resolution is NP-complete, so every promise anyone makes here is a promise about search, not about computation.

two internal facts shape the answer. 0038 gives pith a host-rule tier for exactly the case where the kernel cannot see a body, and 0018 makes pith's totality claim structural rather than assumed — which is precisely why a CDCL search cannot be an ordinary represented body today, since conflict-driven search with learnt clauses is not structural recursion over any argument. and 0021's incremental graph with 0033's revalidation is machinery no surveyed package manager has: a resolver placed inside it inherits caching, invalidation, and provenance for free, which is an argument none of the precedents could make because none of them live in an incremental graph.

## proposed decision

### constraint models are domain-declared; the shared thing is a protocol, not a representation

there is no kernel constraint type. a constraint set is a value in the 0026 calculus, declared by the domain that ranges over it, on the same terms 0039 declared the version scheme: the package domain declares constraints over package coordinates, a placement domain declares constraints over machines, and neither translates into the other's vocabulary.

what is shared, and deliberately small, is the protocol every domain's model satisfies. a solver request names three values: the constraint set, the candidate universe with the provenance of every candidate, and the preference order. a solver answer names two: the choice, as one candidate per constrained subject, and the explanation, as a value. the candidates are 0025-queryable evidence rather than ambient repository state, which is what makes an answer reproducible under the same universe — the property the research record already requires of locks.

this is the CUDF separation with the format refused. CUDF proves the problem statement and the solver can be pulled apart, and MISC proves several solvers can compete over one statement, but CUDF is package-shaped: installed states, upgrades, obsoletes. placement and toolchain selection are not package-shaped, and forcing them through a package constraint format is the translation the research record already refuses. the shared layer is therefore one level up from any format: the requirement that a domain's constraints be values, its candidates carry evidence, and its solver answers carry explanations.

### a package version's coordinates are a record; features are coordinates

a constraint ranges over coordinates, and coordinates are a record: the version, in the spelling and comparison the domain declares, plus the variant settings the domain declares. this answers 0039's handed-over question in the direction purl qualifiers and conda's used-variable hashes point, and the argument is 0039's own timing argument applied one level down. "openssl with shared" is speakable before anything is built, and a constraint over a feature is a constraint over coordinates if and only if features are coordinates; if features stayed request inputs, a description could prescribe them but a constraint could not require them, and M-4's own statement — feature selection — would have no subject to select over. the realization keeps deriving variants the way it already derives platform and toolchain facts, as request inputs; what changes is that the constraint speaks before the realization exists.

### ranges are a closed constructor set over the domain's ordering

a version range is not a grammar in the domain's syntax. it is a closed set of constructors over the ordering the domain already declares — any, exactly, at least, at most, between — with satisfaction, intersection, and negation defined against that ordering. PEP 440 and semver bundle three things into one grammar: how a version is spelled, how two versions order, and which sets of versions a range expression names. 0039 already split the first two out (the spelling and comparison are the domain's `VersionScheme`), and this record splits the third: the range algebra is shared because it depends only on the ordering, not on the spelling, which is also why PubGrub's term algebra (union, intersection, difference over range-sets) transfers to any domain that declares an ordering at all. a domain whose ordering makes "between" meaningless simply does not construct it; the constructor set is closed the way 0026's is, and a new constructor is an amendment to this record.

### hard constraints decide validity; preferences are declared orderings, and underdetermination refuses

a hard constraint is one a solution violates only by not being a solution: intersection over the constraint set defines what valid means, on the model CUDF's semantics already use. a preference never appears in that intersection. it is a third input value: a lexicographic list of orderings over valid solutions.

the reconciliation with 0015 is that a preference must be grounded in a declared ordering, never invented. 0015 refuses to rank ambiguous rule providers because the candidates are arbitrary code by different authors and no ordering among them exists to appeal to. versions of one package are not that case: the domain declares their ordering (0039's `VersionScheme`), so "newest" is domain-declared fact rather than engine policy, the way a registry's publish order is fact. a preference list therefore names orderings the domain declares — newest under the version scheme, fewest changes against the previous lock, the reuse preference over prebuilt binaries — and when the list underdetermines the choice, no declared fact distinguishes the remaining candidates, and the resolver refuses on 0015's terms rather than letting a search order pick invisibly. 0install's tie-nondeterminism is the measured cost of the alternative. the difference from aspcud is deliberate: its criteria vocabulary ranks any solution, and here an underdetermined ranking is an error, because a lock written from it would record a choice nothing explains.

### resolution is a computation in the graph, and its body is a host rule

resolution runs during rule evaluation, as an ordinary request against a declared interface, selected by 0015 like any other. it is not a pre-pass before evaluation and not a separate fixed point outside the graph.

the placement argument has two halves. the first is what the graph gives: the solver's inputs are values — the constraint set, the candidate universe, the preference list, the environment facts a toolchain constraint ranges over — so they participate in the computation key, a changed candidate universe or constraint invalidates the cached resolution through the machinery that already exists, and the lock a resolution writes is provenance like any other result. a pre-pass cannot have this: its result would be a value the engine never keyed, and a changed input would leave the engine serving a stale resolution it cannot see the reason for, which is exactly the failure 0033's revalidation exists to prevent. the second half is what totality allows: a CDCL or ASP search is not structurally recursive, so it cannot be a represented body under 0018, and it does not have to be. 0038's host-rule tier is the declared home for machinery with no ir spelling, and a bounded search over a finite candidate universe is the canonical member: total in the practical sense 0018's backstop gives (a finite universe, a generous limit, a diagnostic naming the runaway), with the honesty that NP-completeness forces and the next section spells out. a represented bounded-search construct stays on 0018's open list, and a second domain needing to explain its solver's interior is the evidence that would promote it.

the determinism contract is the load-bearing clause. a solver body declares that its answer is a pure function of its three inputs, which makes the result cacheable under its key, reproducible across processes on 0024's terms, and explainable by input diff alone. within that contract the search may backtrack, learn, and order itself however it likes — the engine never sees the interior, only the request and the answer, which is the same epistemic position it holds toward every host rule.

0032's seam is answered by the same placement. a project that resolves through cargo inside an `Opaque` and a project that resolves through phloem produce different provenance tiers, and the choice between them is visible in the graph rather than blurred, because resolution-through-phloem is a rule application with recorded attempts and resolution-through-cargo is an `Opaque` boundary with none.

### explanations are two layers, one engine

the engine's existing explanation machinery explains resolutions the way it explains every other computation: an invalidation explanation names the input that moved, which here is a constraint, a candidate, a preference, or an environment fact. nothing new is built for that half.

the solver's own explanation is a value in the answer, carried beside the choice rather than reconstructed from the search. for a failure it is a derivation over the constraint set — PubGrub's derivation graph is the shape, an unsat core in the SAT lineage, a decision list in libsolv's — and it is held to a proof standard: it distinguishes "no solution exists, and here is the derivation" from "the search budget was exhausted," which NP-completeness makes a permanent distinction rather than an implementation deficiency. for a success it is the decision trail: which candidates were considered for each subject and which declared ordering chose the winner, the account 0install's component-at-a-time selection makes naturally short. the two layers meet at one seam: an invalidation explanation points at the input that moved, and the solver explanation inside the previous answer says why that input produced what it produced.

this is the same machinery and different machinery, said precisely. the carrier, the structured diagnostic and the provenance record, is what the engine already has; the content, a derivation over constraints, is a domain value the engine treats as opaque data. the engine does not learn to read derivations, for the same reason 0024 keeps the engine from learning about packages.

## alternatives considered

### one generic constraint representation with multiple solvers

CUDF as a kernel type: every domain — packages, toolchains, placement — states its problem in one format, and solvers compete over it, the MISC arrangement.

the strongest external precedent and the open question's own first phrasing. rejected on domain shape and on 0026's discipline. placement and toolchain constraints are not package constraints wearing different names, and translating them into one format costs exactly what the research record says it costs, a domain bent into another domain's vocabulary. the generic representation also has to live somewhere, and a kernel constraint type is a constructor 0026's closure rule has no room for and no measuring domain behind. the separation that matters — statement apart from solver, criteria as data — survives at the protocol level instead.

### resolution as a pre-pass or an opaque call outside the graph

the conventional arrangement: the package library runs a solver before evaluation begins, hands the engine a resolved set, and the engine builds from it.

rejected on incrementality. the engine can neither key the result nor invalidate it, so a changed candidate universe or constraint needs a manual cache discipline that duplicates 0023 and 0033 badly, and the invalidation explanations users actually need — why did my lock move — have no machinery to come from. the costs of the in-graph choice are real and paid above: 0018 bars the solver from the represented tier, and the search's interior is invisible to the engine. the exchange is a body the engine cannot read for a result it fully owns, which is the trade 0038 already made for every host rule.

### preferences as solver policy

weights and priority levels inside the solver, the libsolv and Spack arrangement, with criteria fixed where the user cannot see them.

rejected on 0015's ground. a preference the caller cannot read is a score, and scores rot; when the policy inside the solver changes, locks move with no input diff explaining why. aspcud's contribution is kept — criteria as data — but the data is an input value under the caller's hand, not a setting of the search.

### global optimization over valid solutions

one cost function ranking every valid solution, minimized by the solver, the strongest form of the aspcud and conda position.

rejected on 0install's evidence and on explainability. near-tied costs made selections nondeterministic in practice, and a globally optimal solution is explained only by exhibiting the arithmetic, which tells a user nothing about their packages. the lexicographic declared orderings chosen here are weaker — no global optimum is promised — and that is the point: NP-completeness already took optimality off the table, and a preference list that names its grounds explains every choice it makes.

### resolve features after version selection

features as request inputs realized per build, with the constraint model ranging over bare versions.

rejected on the timing argument in the coordinates section: feature constraints are speakable before realization, and M-4's feature selection has no subject unless the coordinates carry the features. the cost is accepted knowingly: coordinates grow a record field, and a domain with no features carries an empty one.

## consequences

phloem declares its constraint types as values over the constructors that already exist — records, sums, the range constructors as a declared sum — and `pith-core` gains nothing. no new type constructor lands from this record, which is the first M-4 record to need none.

the first solver is a host rule in phloem, registered against a declared interface, with the determinism contract as part of its declaration. its algorithm is not chosen here; the unresolved section names the candidates and the evidence that would pick one.

a lock written by a resolution records the preference list and the digest of the candidate universe it resolved against, alongside the entries 0039 fixed. selection is reproducible under the same universe by construction, and a lock that moves records which input moved, through the ordinary invalidation explanation.

M-6 is bound only by the protocol. placement and toolchain domains declare their own constraint models and their own solver bodies, or share one through an interface, on 0009's peer terms; nothing in the package model is imposed on them.

resolution explanations enter the diagnostic surface as domain values. the renderer that turns a derivation into prose is library tooling, the way PubGrub's reporter is Dart's rather than the algorithm's.

## unresolved

the algorithm for the first solver is open. a PubGrub-shaped backtracking search over range terms is the leading candidate: its explanations are the best shipped, its single-version-per-package assumption matches a lock that pins one version per package, and its silence on preferences is harmless here because preferences are inputs it consumes. an ASP or SAT translation in the Spack and libsolv line is the stronger fit for the joint space this record's own coordinates create — versions, variants, and toolchains solved together — and the cost is the opacity of its explanations and the weight of the dependency. the evidence that would settle it is a candidate universe of real pith-package scale, which does not exist yet.

whether several versions of one package may coexist in one realization is open. PubGrub assumes they may not, Cargo's feature unification assumes it harder, and pith's realization identity (0039) does not care, since two realizations of one package version are already distinct computations. the constraint model here does not forbid coexistence; whether the package domain's preferences should tolerate it is a policy question for the first solver to measure.

the exact value shape of a derivation is open, held to the proof standard above but not enumerated. PubGrub's incompatibility graph is the starting sketch, and the renderer requirement — a prose account a user can act on — is the design pressure on it.

the reuse preference over prebuilt binaries is named as a preference here and not designed. it orders valid solutions by which realizations have admitted substitutes under 0039's admission policy, and how it composes with the newest-version preference is a policy ordering the binary-reuse work in this milestone has to argue.

`Map` is refused again. the keyed lookups this record creates — a constraint set indexed by package identity, a candidate universe indexed by coordinates — live inside a host-rule solver, which has the host's own containers, and in the value spelling they are canonically sorted lists, whose sortedness is what the digest and the diff need. no pith value computation performs a lookup that needs keys, and the constructor stays unbuilt until one does, on the same terms 0039 left it.
