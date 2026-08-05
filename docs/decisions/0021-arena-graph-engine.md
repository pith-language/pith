---
schema: design-doc/v1
id: decision-0021-arena-graph-engine
title: a hand-built arena graph with explicit change propagation, not a salsa query DB
summary: the engine is a hand-built arena graph over per-instance branded arenas with explicit dependency edges and a change-propagation pass, rather than the salsa query-DB framework
kind: decision
status: proposed
created: 2026-04-27
updated: 2026-06-01
tags:
  - implementation
  - engine
  - kernel
  - incrementality
relations:
  informed_by:
    - research-build-systems
    - research-tooling
  depends_on:
    - decision-0016-implementation-language
    - decision-0005-separate-identities
    - decision-0013-managed-object-identity
    - decision-0015-interface-rule-selection
    - decision-0019-effect-categories-and-nondeterminism
    - foundation-principles
  supersedes: []
---

# a hand-built arena graph with explicit change propagation, not a salsa query DB

## context

decision 0016 settles that the kernel is implemented in rust and that the graph is modeled as an arena of nodes indexed by integer handles. it does not settle the engine built on top of that representation: whether the engine is a hand-built dependency graph with an explicit change-propagation pass, or whether it adopts a query-based incremental framework such as salsa.

the question is load-bearing. the kernel's central invariant (requirement K-9) is that incremental execution is equivalent to an empty-cache evaluation under the same declared inputs. the engine representation determines whether that invariant is something the engine can reason about and verify, or something the engine inherits from a framework whose assumptions do not match pith's model.

the rules-and-graph design doc already describes the engine in arena-and-edge terms: dependencies are recorded as a rule evaluates; they may depend on earlier results; requests are part of the semantic interface; the engine needs parallel requests, cancellation, persistent caching, equality-based change pruning, and invalidation queries. the design test (a new domain implementable without a core patch) and the no-first-party-privilege requirement (U-10) both assume an engine whose extension surface is ordinary typed values, not framework-specific macros.

## proposed decision

### hand-built arena graph

the engine is a hand-built dependency graph over per-instance arenas. nodes are rule applications (request plus selected rule plus inputs). edges are dependency records: a request that one rule made of another, an observation of external state at a revision, a declared capability use. the graph stores provenance, capability propagation, and the five identity types (0005, 0013) as first-class typed edges and node data, not as framework opacities.

a change-propagation pass walks the graph from changed inputs, invalidates consumers by equality-based comparison, and re-evaluates only what the dependency edges show is affected. the pass is the engine's own code. it is not delegated to a query framework's invalidation algorithm.

### per-instance arenas with branded indices

each engine instance owns its arenas. indices are `Id<Brand>` newtypes with two parts: a type-level brand for the arena category and a private owner token for the arena instance. a `TypeId` and a `ValueId` are different types and cannot be substituted at compile time. only an arena can construct an id, and lookup rejects an id carrying another arena's owner token. cross-engine isolation is therefore checked at the arena boundary without making the engine's public API lifetime-generative.

### adapters behind pith-owned traits

nontrivial external crates sit behind pith-owned trait boundaries, never in public signatures. the arena layer wraps `la_arena`-style `IndexVec`; source spans wrap `text-size` and `line-index`; hashing wraps `blake3`; the async runtime wraps `tokio` behind a `Runtime` trait. if a crate is replaced or reimplemented, only the adapter module changes. `serde` is the one stated exception: it appears in `pith-output` (the DTO crate) because serialization is that crate's job and the DTO is already the replaceable boundary.

### byte-based utf-8-correct spans

spans are byte-offset-based and utf-8-correct. `Span` is a pith-owned newtype over `u32` offsets; conversion to and from `text_size::TextRange` and `line_index::LineIndex` lives in one internal module. non-ascii source is handled correctly without leaking the upstream crate into diagnostic types.

### deterministic maps

`HashMap` is banned from any code path that affects observed output (diagnostics, cached values, serialized records, ordering user-visible to tools). `IndexMap` and `IndexSet` (insertion-ordered, no `Ord` requirement) are the default; `BTreeMap` is used only where sorted-by-key iteration is specifically wanted. this is enforced by a CI grep against `HashMap` use in non-internal modules and by code-review norm. determinism is a semantic property (K-9, 0014); making it structural prevents an entire class of silent breakage.

## why not salsa

salsa is a generic framework for on-demand, incrementalized computation, extracted from rustc's query system and used by rust-analyzer. it is the strongest off-the-shelf option and was considered seriously.

its assumptions do not match pith's model in three places where the mismatch is load-bearing rather than cosmetic.

first, pith's graph carries five identity types (0005, 0013), capability propagation across composition, provenance linking every identity kind, and the requirement that empty-cache equivalence be provable. salsa's query DB abstracts the dependency graph behind query functions and an invalidation algorithm. the structured data pith needs to record per-edge — which identity kind this edge is, which capability it exercised, which revision it pins — has no natural home in the query-DB model and ends up stored in side tables the framework does not know about, which is the same failure mode as a category field on one `Effect` type (rejected in 0019): load-bearing structure hidden behind a framework abstraction.

second, the five effect categories (0019) have genuinely different scheduling, caching, and authority contracts. an `Observation` is never cached across revisions; a `Mutation` is not cached as a result at all; an `Opaque` is a fixed-output boundary. salsa's invalidation is keyed on input equality. modeling the category-dependent cache and staleness contracts inside salsa's invalidation means re-implementing the engine's policy inside a framework whose own invalidation runs underneath, with the two competing. a hand-built graph lets the propagation pass dispatch on the category structurally, which is what 0019 requires.

third, the no-first-party-privilege invariant (U-10) and the design test (a new domain without a core patch) assume the engine's extension surface is ordinary typed values. salsa's query model is expressed through attribute macros (`#[salsa::tracked]`, `#[salsa::input]`) that couple the IR to the framework. migrating a salsa-based engine to a different mechanism later, or proving that an external library can replace the engine's query layer, is harder than starting from a graph the project owns.

## alternatives considered

### salsa query DB

adopt salsa for on-demand incremental queries.

the strongest off-the-shelf option: less code, battle-tested in rust-analyzer, with durable inputs and verified-correct invalidation. rejected because the structured per-edge data pith needs, the category-dependent cache contracts, and the ordinary-typed-values extension surface do not fit the query-DB model without storing engine data in side tables and re-implementing policy under the framework's invalidation. salsa's ideas (durable inputs, verified-correct invalidation, cycle handling) are borrowed; its macro-coupled query machinery is not adopted.

### rustc's own arena-graph system

model the engine on rustc's hand-built query system with arena-allocated interned ids.

the closest precedent and the strongest evidence the pattern works at scale. rejected as a literal model because rustc's query system is specialized to batch compilation and carries compiler-specific assumptions. the arena-and-index representation (0016) and the brand-token discipline are borrowed; the query layer is pith's own, shaped by the effect categories and the identity model rather than by rustc's compilation passes.

### defer, prototype with salsa and revisit

prototype M-1 on salsa, mark it provisional, revisit if the structured-data mismatch appears.

violates the notebook's discipline. decision 0015 and 0019 are resolved before the prototype because prototyping on an undefined base cannot test the question. the engine representation is the same kind of gate: building the prototype on a framework whose assumptions mismatch pith's model means the prototype cannot honestly test capability propagation, category-dependent caching, or provenance across identity types. rejected for the same reason 0015 and 0019 were not deferred.

### a fully custom graph with no framework ideas

hand-build the graph and ignore salsa and rustc entirely.

rejected. the durable-inputs, verified-correct-invalidation, and cycle-as-structured-diagnostic ideas are correct and worth absorbing. the decision is to own the engine, not to reinvent the field.

## consequences

the engine is pith-owned code. capability propagation, category-dependent caching, provenance across five identity types, and empty-cache equivalence are all things the engine can reason about and verify because they are in its own graph, not behind a framework abstraction.

the cost is real. a hand-built change-propagation pass is more code than adopting salsa, and it carries its own correctness burden. contributors need to understand the arena-and-edge model, category brands, owner checks, and the propagation pass. this is the standard cost of owning a core mechanism rather than delegating it.

the arena-owner discipline makes test code go through arena constructors to mint ids, which is occasionally felt as friction. this is a feature: it is what makes the cross-arena and cross-category invariants real. a `test_arena()` helper keeps the friction off the common test path.

the adapter discipline (every nontrivial external crate behind a pith-owned trait) means public signatures name pith types, never foreign types. swapping a crate or reimplementing an adapter is a local change. the cost is one indirection per external concern, which is the price of the replaceability the project wants.

`serde` appears in `pith-output` and nowhere else in public types. `tokio` appears only inside the scheduler module (decision 0022). the leaf-crate property of `pith-output` and `pith-diag` is structural, enforced by the dependency direction.

## prototype evidence

the in-memory prototype interns completed pure applications by selected rule, typed interface, and input values. diagnostic labels and spans do not participate in reuse. distinct roots share completed pure dependencies.

action contracts are validated before receiving a versioned, domain-separated digest over their complete canonical representation. this digest identifies the declared contract, not a reusable computation: it does not include the selected rule's completion semantics, the resolved execution platform, or policy decisions. action computations therefore remain non-reusable, and that decision propagates to pure parents.

action computations record their canonical declared capability requirements. completed pure computations derive the canonical union of their dependency requirements, and the query interface exposes that effective set. this proves propagation through the implemented `Pure` and `Action` composition path; the other effect categories and per-use capability edges remain open.

the graph records a structured reuse decision instead of a boolean, so queries can distinguish reusable pure computations from actions whose caching is disabled and parents with non-reusable dependencies. this proves exact pure reuse inside one engine instance. it does not settle action cache identity, persistent pure-computation identity, durable cache storage, invalidation after changed durable inputs, or cache explanations, so the decision remains proposed.

## unresolved

the exact change-propagation algorithm — eager invalidation, lazy re-derived-on-demand, or a hybrid — needs a prototype. the empty-cache-equivalence invariant (K-9) constrains the choice but does not determine it.

how the graph persists between engine instances and between daemon versions is open. durable inputs (the salsa idea worth borrowing) suggest one shape; the brand-token discipline makes cross-instance id portability a question that needs an explicit answer.

how a remote-cache adapter (0020 names nix binary caches as one) speaks the engine's invalidation language is open. the content-identity layer is shared; the dependency-edge layer is not, and a remote cache that does not understand the edge structure can only claim content equality, not computation equivalence.

whether the CI grep against `HashMap` is sufficient or a custom `dylint` rule is needed is empirical. start with the grep; escalate only if `HashMap` slips into output-affecting code.
