---
schema: design-doc/v1
id: decision-0026-generic-typed-calculus
title: a generic structural type calculus, with nominal identity, generic uncertainty, and no predicate types
summary: one closed structural calculus (records, declared sums, parametric generics, effect types, uncertainty types) with nominal identity as a declaration attribute; refinements stay out of the type language and live as pure validation rules
kind: decision
status: proposed
created: 2026-05-13
updated: 2026-08-14
tags:
  - types
  - language
  - calculus
relations:
  informed_by:
    - research-configuration
    - research-nix
  depends_on:
    - decision-0010-typed-pure-language
    - decision-0017-structural-with-nominal
    - decision-0018-termination-and-recursion
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0023-rule-and-cache-identity
    - foundation-principles
  supersedes:
    - decision-0017-structural-with-nominal
---

# a generic structural type calculus, with nominal identity, generic uncertainty, and no predicate types

> supersedes [0017: structural types by default, nominal by declaration](0017-structural-with-nominal.md), whose structural-default and nominal-by-declaration mechanism becomes one section of the calculus below. 0017 stays in the repository; its proposed direction is replaced by this record. amends [0010: use a typed, pure, terminating declaration language](0010-typed-pure-language.md), whose unresolved list names "nominal versus structural typing, termination checking, row polymorphism, refinement performance, schema evolution, and module compatibility"; this record settles the calculus questions among those and leaves termination (0018), schema evolution, and module compatibility to their own records.

## context

decision 0010 chose a strongly typed, pure, terminating declaration language and left the exact type calculus open. decision 0017 settled one axis of that question, structural typing by default with nominal identity by declaration, without naming the rest. the values-and-types design doc lists the required features: records, variants, generics, interfaces, opaque types, and validation that refines an uncertain value into a stronger type. the current implementation in `pith-core` carries six scalar types and a nominal type that holds only a name string. the calculus the design calls for has not been built.

four load-bearing commitments already constrain the calculus without it being written down.

decision 0023 makes type canonicalization load-bearing. the pure-computation key is a versioned, domain-separated digest over "canonical typed inputs." a type that cannot be canonicalized cannot be digested, and a type that cannot be digested cannot participate in computation identity. whatever the calculus is, it has a canonical form the engine can serialize and hash.

decision 0015 makes types participate in dispatch. rule selection matches a request against declared interfaces and refuses ambiguity. "match" is currently equality on the `Interface` struct. a calculus that turned match into unification, as row variables in interface types would, would make the refuse-ambiguity rule substantially harder and more expensive to check.

decision 0019 already puts types in the kernel IR. the five effect categories are distinct IR types because "the scheduler, cache, capability checker, and policy engine read the category structurally rather than by convention." the calculus has to represent them, and the reasoning that put categories in the types applies beyond categories.

the principles commit the calculus to model uncertainty structurally. "the types should say this directly. `Unknown`, `Unreachable`, `Stale`, `Conflicted`, and `Unchecked` are more useful than a value that only looks trustworthy" (principles:56). that phrasing is almost verbatim the gradual-typing position on `?` / `Dyn`.

so the question is what is in the calculus and what stays out. four decisions already assume there is one.

## proposed decision

the kernel has one structural type calculus. it is closed: the constructor set is fixed by this decision record, and a new constructor requires a record that amends or supersedes this one, on the model of 0019's closure rule for effect categories.

the calculus is structural by default. nominal identity is a declaration attribute on the structural core, with no parallel system beside it. this carries 0017's mechanism forward unchanged. the new work is naming everything else in the constructor set and everything explicitly kept out.

### the constructor set

the calculus has these constructors.

scalar types are `Unit`, `Bool`, `Int` (arbitrary precision), `Text`, `Bytes`, and `Blob` (content-addressed identity). bounds on `Int` are a library concern expressed via generic uncertainty or declared nominal wrappers, with no proliferation of fixed-width scalar types.

records are products of named fields. a record is closed: its field set is fixed by its type. there are no positional tuples; a tuple is a record whose field names happen to be numeric, and the engine treats it as one.

declared sums are a nominal type with a fixed set of constructors, each optionally carrying a typed payload. pattern matching is exhaustive. this is the only sum mechanism.

parametric type constructors cover `List<T>`, `Map<K, V>`, `Option<T>`, `Result<T, E>`, and the effect categories `Pure<A>`, `Action<A>`, `Observation<A>`, `Mutation<A>`, `Opaque<A>`, all of kind `* -> *` or `* -> * -> *`. type application is reified: `List<Int>` and `List<Text>` are distinct types, distinct cache keys, and distinct interface participants.

generic quantification is rank 1 (prenex `forall`): a rule may be polymorphic over its type parameters. higher-rank and higher-order quantification are out of scope for the surface language.

nominal declarations carry a name, an optional set of type parameters, and a structural representation. `nominal MachineId = Text` is not interchangeable with `Text` even though its representation matches. this is where the five identity types (0005, 0013) land: managed-object identity, content identity, external identity, and the rest are attributes of nominal declarations, with no separate registries.

effect types are the five effect categories from 0019 as type constructors of kind `* -> *`. they are in the calculus because the engine already dispatches on them structurally. this is settled by 0019; the calculus represents it.

generic uncertainty types are `Unknown`, `Unchecked<T>`, `Stale<T>`, `Conflicted<T>`, and `Unreachable` as primitive type constructors of the calculus. the engine dispatches on them structurally: the cache invalidates on `Stale`, the planner surfaces `Conflicted`, the scheduler treats `Unreachable` distinctly. they pass the same inclusion test 0019 used for effect categories.

nominal declarations are the only place a type gets identity. everything else is structural.

### what stays out of the type language

five things are deliberately absent from the calculus. each is a real position in the design space, each was weighed against the constraint that 0023 and 0015 put on canonicalization and dispatch, and each is rejected below with its reasoning on the record.

no row polymorphism. records are closed. a function that wants "any record with a `name` field" cannot be typed in the calculus; it must declare the full record type it accepts, or the caller must pass a value of a nominal type the function names. composition across record shapes happens at the value level through an explicit, provenance-carrying merge operator (see below), with no row variables at the type level.

the decisive reason is canonicalization. no content-addressed build system has digested a row-polymorphic type. PureScript, the cleanest contemporary row-typed language, does not canonicalize its `Type` AST at all and invents a separate `RowList` type-level representation when it needs stable ordering. putting row variables into interface types would also turn 0015's equality-based match into unification, making the refuse-ambiguity rule O(rules × unification) per selection and making ambiguity diagnostics substantially harder. the calculus stays closed so cache keys and selection stay cheap and exact.

this does not give up the composition power rows were meant to provide. it moves it to the value level, where conflict resolution belongs.

no predicate refinement types. the type language does not contain `{ x : Int | x > 0 }`. predicates over values live in pure validation rules, with no home in types. the calculus models the difference between an unvalidated and a validated value structurally via `Unchecked<T>` and `T`; what the validation checked is a provenance fact, with no type-fact counterpart.

the decisive reason is content addressing. if refinements were types, then either the digest includes the predicate (and changing a signature invalidates every downstream artifact even when the value is unchanged, and predicate equivalence is undecidable so canonicalization breaks) or the digest erases it (and refinements are unobservable to the cache, so they buy nothing at the identity layer). this tension is not addressed in the published literature because no one has built a refinement-typed content-addressed system; it follows directly from the two requirements together. Dhall and Nickel, the two closest shipping systems, both keep predicates out of the type language. Dhall does so entirely; Nickel via contracts that are values.

no polymorphic (extensible) variants. the only sum mechanism is declared nominal sums. there is no "any variant containing `Stopped`" type.

the decisive reason is ambiguity. polymorphic variant tags live in a flat namespace and compose by silent unification; the `` `Running `` collision (service state versus build-step state) is the textbook failure. the structural composition across boundaries that polymorphic variants would provide is already provided by structural records, which compose by field name where a missing field is locally obvious. declared sums are what every comparable build and configuration system ships, and the OCaml community, including the designer of polymorphic variants, recommends declared variants for almost all code.

no higher-order polymorphism in the surface. the calculus has type constructors (`* -> *`, `* -> * -> *`) because generics and the effect categories require them. it has no abstraction over type constructors (`(* -> *) -> *`): there is no `forall f. ...` and no `Functor`-style type class.

the decisive reason is the cost-benefit curve. higher-kinded abstraction is what Haskell and Scala ship and what OCaml, Rust, Swift, and F# all declined, on inference-predictability grounds. no build, package, or deployment domain needs it. none of Nix, Bazel, Buck2, Dhall, Nickel, or CUE has it, and none has hit a wall because of its absence. the Yallop encoding proves that if a genuine need for higher-kinded abstraction ever appears, it can be added as a library technique without a language change. the IR carries ordinary type constructors because 0019's effect categories are `* -> *` constructors; the surface does not expose abstraction over them.

no positional tuples. records are named-field only. a tuple is a record whose field names are numeric, with no separate calculus construct. one mechanism for products.

### composition: merge operators, not magic

0017 and the values-and-types design doc both reject "a global magic merge that decides conflicts from import order" (principles:50). closing records does not weaken this rejection; it sharpens where the merge lives.

composition of record-shaped values is a value-level operation that carries provenance and names what it does. it has no hidden operator with implicit conflict resolution. a merge takes two records and produces either a merged record or a `Conflicted<T>` that the engine surfaces; it does not silently pick a winner from import order. priorities, overrides, and conflict policy are declared at the merge site and recorded in provenance.

Nickel is the existence proof that an explicit, typed, explainable merge operator coexists with structural records and gradual typing without needing row polymorphism. the calculus here follows that split: structural records for type-level composition, merge operators for value-level composition.

### generic uncertainty as kernel primitives

baking the five uncertainty types into the calculus is the natural completion of 0019, with no new axis introduced. 0019 fixed how the engine dispatches on categories of work; this decision fixes how it dispatches on states of results. the test is the same: does any engine subsystem (scheduler, cache, capability checker, policy engine, provenance) read this state structurally?

`Stale<T>`: the cache invalidates on it; the scheduler treats it as a freshness fact. 0019 already says "staleness is a graph fact."

`Conflicted<T>`: the engine refuses to treat a conflicted value as a resolved one; plans and provenance surface it.

`Unreachable`: the scheduler treats an unreachable observation differently from a present one.

`Unknown`: the gradual `?`. the consistency relation and the refinement of `Unknown` into a precise type are type-system operations, with no library-convention fallback. this is the Siek-and-Taha / AGT result that gradual types must be primitive because their semantics are definitional.

`Unchecked<T>`: the type system prevents an unchecked value from flowing where a checked one is required, without an explicit validation step. validation is a pure rule `Unchecked<T> -> Result<T, ValidationFailure>`.

the set is closed under the same discipline as 0019: a sixth uncertainty type requires a decision record that amends or supersedes this one. this guards the "keep the kernel small" principle. the set is bounded by argument, with no accretion.

the case against baking uncertainty in is Rust's `Option` / `Result`, which are library types and work structurally. that case does not transfer here, for one precise reason. the Rust compiler does not schedule, cache, or dispatch differently on `None` versus `Err`. those are value-level facts the program handles. in pith, the engine itself (not the program) dispatches on staleness, conflict, and unreachability, because those are facts about the incremental graph, the cache, and provenance. that is the difference between uncertainty a program handles, for which a library is enough, and uncertainty an engine schedules around, which must be structural.

### canonicalization

types canonicalize by structural normalization. closed records sort their fields; type application is structural; nominal identity is a stable digest of the declaration (the same construction 0023 uses for rule identity, applied to type declarations); effect and uncertainty constructors are nullary tags. there is no type-level computation (comptime is deferred to a future surface-language decision), so structural equality is type equality and the digest machinery extends naturally.

this is the property that makes the calculus compatible with 0023 and 0015. because the calculus is closed and free of type-level computation, canonicalization is a serializer, with no normalization over arbitrary computations. the versioned, domain-separated digest in 0023 applies to the canonical form directly.

## alternatives considered

### full row polymorphism (open rows in types and value-level extension)

Leijen-style extensible records with scoped labels. records carry a row variable; functions accept "any record with field `x`"; value-level extension adds and removes fields.

the most expressive option and the one the configuration literature points toward. OCaml uses rows for objects and polymorphic variants; PureScript makes records a `Row Type -> Type`; Roc is betting the language on structural records with good error messages.

the cost is canonicalization. no row-polymorphic language digests its types. PureScript's `Type` AST is not even hash-consed, and where stable ordering is needed (serialization) PureScript invents a separate `RowList` type-level representation. that is direct evidence that the row type itself is not canonicalizable without a transformation step, and that the transformation is non-trivial. in pith, where 0023 puts the type into the computation key's digest, open rows in interface or cache positions would require alpha-equivalence over row variables, canonical field sorting, and a versioned encoding for all of it. the algorithm exists; the migration story and the testing burden do not come for free.

elm's removal of value-level record extension is a separate documented data point. elm had full Leijen-style extension and deletion and removed both in 0.16, citing near-zero usage over two years, code-quality collapse in the cases that did use it, and a measurable optimization cost. elm kept extensible record types and dropped extensible record values. the lesson the calculus here takes is the same: the type-level extensibility is what matters for composition; the value-level extension is a power nobody needs and a cost everybody pays.

row polymorphism is not rejected forever. if configuration libraries chafe at exact-shape matching on closed records, the cleanest expansion is to admit row variables in local inference only and require interface and cache positions to be closed (ρ = empty). that keeps canonicalization tractable where it is load-bearing while recovering the width-accepting function signatures config libraries want. this is reserved as a future amendment, with no adoption now.

### refinement types in the type language

either full SMT-backed refinements (F*, LiquidHaskell) or the decidable Liquid Types fragment.

rejected on the content-addressing interaction, which is decisive and independent of the refinement flavor chosen. F* and LiquidHaskell also document the secondary costs: 3-to-5 lines of proof per line of code at F*'s scale, unpredictable SMT compile times, version brittleness across solver releases, a global qualifier vocabulary at the Liquid Types layer that is a poor fit for non-expert config authors. none of those is acceptable in a build kernel where cache lookup is on the hot path. the content-addressing tension makes the case regardless of those.

### structural records plus a separate nominal system (0017's literal reading)

two type systems, one for structural data and one for identity-bearing types. explicitly considered and rejected by 0017 itself, on the principle of one mechanism per concern. this record carries that rejection forward unchanged: there is one calculus, and nominal identity is a declaration attribute within it.

### declared sums plus polymorphic variants

OCaml's full sum story: declared variants for closed cases, polymorphic variants for open extension.

rejected on ambiguity. the duality between extensible records and extensible variants is formally exact but practically asymmetric: record fields are read by name where a missing field is locally obvious, while variant tags live in a flat namespace and collide silently. the `` `Running `` collision is the canonical example and pith's "reject ambiguous composition" principle has nothing to say to it under polymorphic variants that it can say structurally. the cross-boundary composition polymorphic variants would buy is already provided by structural records.

### higher-kinded polymorphism in the surface

full System Fω: abstraction over type constructors, `Functor` and `Monad` as type classes, effect handlers as a library.

rejected on the cost-benefit curve, with no objection on principle. the GHC/Core split, an Fω IR behind a far simpler surface, is the pattern pith follows in miniature: the IR carries ordinary type constructors because the effect categories are `* -> *`; the surface does not expose abstraction over them. if a future library genuinely needs HKT-like abstraction, the Yallop `app`-brand encoding adds it as a library technique without a calculus change, which is the path OCaml's ecosystem took.

## consequences

the `Type` enum in `pith-core` grows from six scalars and a nominal placeholder to a closed constructor set matching the list above. `Nominal` gains its representation and type parameters; the current `Nominal { name: Box<str> }` is a placeholder that this record replaces. the canonical-type serializer that 0023's digest already requires becomes the authority for type equality; there is no separate normalization pass because the calculus has no type-level computation.

rule selection (0015) stays exact-equality on canonical interfaces. cache identity (0023) digests canonical types directly. both are cheaper than they would be under any alternative that admitted row variables or type-level computation.

library authors gain a complete calculus for domain types: records for configuration, declared sums for state machines and result types, generics for containers, nominal declarations for identity-bearing types, and a structural uncertainty vocabulary the engine understands. they lose row-polymorphic function signatures (a function must name the record it accepts) and refinement types in signatures (a predicate belongs in a validation rule, with no home in a type). both losses are deliberate and recoverable: rows via a future amendment restricted to local inference, refinements via external tools that read pith types without being in the kernel.

the merge operator becomes the value-level composition tool. it is library-visible, typechecked, and provenance-carrying; it is not the global magic merge the principles reject. designing its exact signature and conflict policy is part of the values-and-types design work this record enables, with no claim on the calculus itself.

the uncertainty types and the effect categories compose. an `Observation` of a managed object that has become stale carries its `Stale<T>` state structurally; the scheduler reads it; the cache invalidates on it; provenance records it. no library convention is needed for the engine to do the right thing, and no library can accidentally do the wrong thing because the type says what the value is.

## landed ahead of the calculus

the `Value::Nominal { name, representation: Box<Value> }` constructor landed before the rest of the constructor set above. the rest of the calculus — records, declared sums, parametric generics, effect and uncertainty constructors — is not built; only the one constructor that unblocks rule dispatch (0015) is. without an inhabiting value, `Type::Nominal` was declared, canonically encoded, and digested into computation keys (0023) but never matched: `value_type` returned `false` for every value against it, so any rule declaring a nominal output failed `ResultTypeMismatch` and any request declaring a nominal input failed `RequestInputsMismatch`. every content-producing rule collapsed to `(...) -> Blob`, and any two of them collided as `E-1102` ambiguity.

the slice that landed carries the 0026 semantics in miniature: a nominal value matches its own name only, and is not interchangeable with its representation. `value_type` returns `Type::Nominal { name }`; `is_type` accepts the value against `Type::Nominal { name }` when the names agree and rejects the representation's own type. `Type::Nominal` still carries only a name, not its declared representation, because the declaration site that would carry one does not exist yet; checking the name is what is checkable today. when the declaration lands, the representation type it carries becomes the second thing `is_type` verifies.

the canonical codec's reserved `TAG_NOMINAL = 6`, which the comment in `value_codec.rs` held "`Type`-only," now carries a value payload: the name, then the recursively-encoded representation. `RECORD_ENCODING_VERSION` (0024) moves to 2; the byte-level `ENCODING_VERSION` stays at 1 because no existing byte sequence changed meaning — no prior build could emit a nominal value. a pre-release database recorded under version 1 is moved aside and rebuilt, which the existing test at `durable_engine_state::an_incompatible_database_is_moved_aside_and_rebuilt` already asserts.

`List` is the second constructor to land (0034 needed a discovered-header set with no cardinality limit, and the `List<T>` this record names is the list the calculus already reserves). `Value::List(Box<[Value]>)` and `Type::List(Box<Type>)` carry it; `TAG_LIST = 7` encodes a type's element type and a value's length-prefixed elements under the shared numbering. `List` makes both grammars recursive, so type decoding gains the depth bound value decoding already had, and `RECORD_ENCODING_VERSION` moves to 3 on the same moved-aside-and-rebuilt terms as the move to 2. type application is reified the way this record says: `List<Text>` and `List<Nominal>` are distinct types and distinct interface participants, which is what keeps a link over `List<Object>` from colliding with anything else. `is_type` types a list by its elements and accepts an empty list against every `List<T>`, a fact `value_type` cannot express (it must pick an element type when there is no element to ask); request-input checking therefore compares with `is_type` rather than `value_type`, and the two agree everywhere else.

`Record` is the third constructor to land (0039 needed a package description and a lock entry, and neither is honestly spellable in scalars plus `Nominal` plus `List`). `Value::Record` and `Type::Record` carry a sorted slice of named fields; construction sorts and rejects duplicate names, `TAG_RECORD = 8` writes the count then each length-prefixed name and payload in both grammars, and the decoder accepts strictly ascending name order only, so the canonical form this record's canonicalization section specifies is the only one on the wire. `RECORD_ENCODING_VERSION` moves to 4: tag 8 is new and no existing byte sequence changes meaning, but the retained-value grammar the version gates has grown, so the gate moves on the same moved-aside-and-rebuilt terms. what does not land: the merge operator (the value-level composition this record's own section reserves for design alongside the first configuration library) and row variables, which stay rejected. `is_type` matches a record against a record type field by field — same names, each payload inhabiting the declared field type — with no width or depth subtyping, and `value_type` answers for a record field by field. a record introduces no asymmetry of its own, but it inherits any its fields carry: a record with an empty-list field types as `{f: List<Unit>}` while inhabiting `{f: List<Int>}` just the same, and a record with a sum field inherits the singleton problem. the guarantee that holds everywhere is weaker than agreement: `is_type` accepts every value against its own `value_type`, and records keep that reflexivity without adding a third exception to the two `List` and `Sum` already found.

declared sums are the fourth constructor to land (0039's source binding is a fixed set of constructors carrying different payloads — a registry archive, a git revision, a local path — and the tag-field spelling re-creates the flat-namespace ambiguity this record rejected polymorphic variants for, at a smaller scale). `Type::Sum` carries the sum's name and its constructor set, constructors sorted by name at construction; `Value::Sum` carries the sum's name the way `Value::Nominal` carries its own — the declaration site that would resolve the name does not exist yet — plus the selected constructor and its optional payload. `TAG_SUM = 9` writes the name then, for a type, the sorted constructors with a presence byte and payload type each, and for a value, the constructor name, a presence byte, and the payload. `RECORD_ENCODING_VERSION` moves to 5 on the same terms as the move to 4. the asymmetry `List` found arrives here from the other direction: a sum value cannot recover its sibling constructors, so `value_type` names only the singleton sum holding the constructor the value selected, while `is_type` accepts every declared sum of that name containing that constructor with a matching payload. request-input checking already compares with `is_type`, so nothing gates on the singleton. what does not land: pattern matching, which is a rule-body concern (0038), and the declaration site itself, which keeps the deferral this record's unresolved section already records — landing it was not required to type a sum value, because the value carries its sum's name the way a nominal value does.

## unresolved

the exact surface syntax for declaring nominal types, records, and sums is open. the mechanism is settled by this record and by 0017; the surface is not, and belongs to the future module-and-syntax work alongside schema evolution and module compatibility (both still open from 0010).

the merge operator's signature, priority system, and conflict-to-`Conflicted<T>` promotion rule need design alongside the first configuration library prototype. Nickel's merge with `default` / `force` / `optional` priorities is the strongest precedent; whether pith's merge carries the same priorities or a different set is a library design question.

how `Unchecked<T>` is produced at system boundaries (the exact shape of the validation rule's failure type, and how blame is attributed in the Findler-Felleisen style when a validator rejects) needs design alongside the first build library that consumes untrusted input.

whether the canonical-type serializer needs a version separate from the semantic-encoding version in 0024, or rides the same gate, is a serialization-detail question for the first persistent prototype that stores types.

schema evolution for nominal types across library versions, the question 0017 explicitly left open, is unchanged by this record and still belongs to the module-system work.
