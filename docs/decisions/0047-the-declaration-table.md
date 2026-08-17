---
schema: design-doc/v1
id: decision-0047-the-declaration-table
title: a declaration table in the core, with type identity by coordinate and revision by digest
summary: nominal types, declared sums, and structural aliases are declared once per module under a module identity; Type::Nominal carries its declaration so is_type verifies the representation, and a rule's revision derives from the declarations its interface names
kind: decision
status: proposed
created: 2026-07-22
updated: 2026-08-14
tags:
  - types
  - language
  - identity
  - declarations
relations:
  informed_by:
    - research-declarations
  depends_on:
    - decision-0017-structural-with-nominal
    - decision-0018-termination-and-recursion
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0023-rule-and-cache-identity
    - decision-0026-generic-typed-calculus
    - decision-0038-represented-rule-bodies
  amends:
    - decision-0026-generic-typed-calculus
---

# a declaration table in the core, with type identity by coordinate and revision by digest

> amends [0026: the generic structural type calculus](0026-generic-typed-calculus.md) twice: it builds the declaration site 0026 kept deferring ("the declaration site that would carry one does not exist yet"), and it audits the constructor set 0026 closed, removing the constructors that fail 0026's own inclusion test. 0026 stands; its constructor list is corrected the way its closure rule requires — by a record.

## context

three records point at a thing that does not exist. 0026 says it three times in its own account of what landed ahead of the calculus: "`Type::Nominal` still carries only a name, not its declared representation, because the declaration site that would carry one does not exist yet," and its unresolved section leaves "the exact surface syntax for declaring nominal types, records, and sums" to future work. 0023's unresolved section accepts that "the prototype accepts an explicit module identity at its rust registration boundary" while the real model stays open. 0038 is the most direct: a represented rule's identity "stays where 0023 put it: the module identity and the declaration name, the stable coordinate of the declaration site, carried in the module's declaration table." the table is named. it is not built.

the gap is not inert. `Type::Nominal { name }` matches any value that carries the same string, so `Value::Nominal { name: "xylem.Object", representation: ... }` inhabits xylem's link interface from any crate in the workspace whatever its representation holds — a `Text` masquerading as content identity passes every check the kernel runs. the same bare name is why a rule's revision cannot be derived: nothing connects the `Object` in an interface to a declaration whose changing representation should move a compile's cache key. xylem's `types.rs` is the hand-built version of what is missing — six nominal types as string constants, one per content role — and `rule_revision` in `rules/mod.rs` is the standing failure the prompt for this work names: it hashes `b"xylem-v3"`, so one author edit moves every xylem rule at once, and no edit at all moves any rule when a representation changes underneath it.

the precedents disagree about what a declaration's identity even is, and the disagreement is the substance. the research note beside this record reads four of them from their primary documents: protobuf and cap'n proto keep author-chosen ordinals stable while content changes, at the price of permanent reserved-number discipline; WIT keeps registry-qualified names with a semver contract; Dhall hashes the normalized expression and learned, in the 1.17.0 basis change, that a digest's basis is itself versioned surface; Unison makes the content hash the identity and the name metadata. this record picks a position among them and says why.

## proposed decision

### what a declaration is

a declaration is one entry in a declaration table held under a module identity. three kinds of thing are declarable. a nominal declaration carries a name and a structural representation type; `xylem.CSource` over `Blob` is one. a declared sum carries a name and a fixed set of constructors, each optionally carrying a typed payload; `phloem.Source` is one. a structural alias carries a name and a target type it abbreviates; referencing one yields the target, expanded.

a declaration's identity is its coordinate: the module identity plus the declared name. this is 0023's identity half applied to types, and 0038's coordinate made concrete. the coordinate is stable across representation changes, doc-comment edits, and reordering, the way a rule's identity is stable across a compatible refactor. a module declaring one name twice is refused at the table; two modules declaring the same short name are two declarations with two digests, not a collision — the table's key is the pair, not the name.

the module identity is accepted at the rust registration boundary, as 0023's unresolved section already accepts for rules: xylem declares under `"xylem"`, phloem under `"phloem"`, matching the module identities their rule identities already carry. the cross-repository compatibility model for module identities stays 0023's and 0038's open question, unchanged by this record.

### why not unison's position

unison gives a definition the hash of its elaborated content as its identity and demotes the name to metadata. the research note takes it seriously because it is the strongest published answer to exactly this question, and because pith already agrees with it where behavior is the meaning: content identity for values, and, when 0038's represented tier lands, digests for rule bodies.

it is the wrong answer for nominal declarations, for the reason 0026 makes load-bearing: two nominal types over `Text` are distinct. `MachineId = Text` and `ClientId = Text` carry identical content — the same representation, no constructors, nothing else in the declaration to differ — and their distinction is the entire point of nominal identity: it records which distinction the author meant, and an author's intention is not a function of the declaration's bytes. content identity cannot express a distinction the content does not carry; a coordinate can, because the author chose it. unison's model also couples identity to the elaborator's internal representation, which is the coupling Dhall's 1.17.0 break shows the price of; a coordinate couples identity to nothing but the author's naming act.

so declarations follow rules, not values: identity by coordinate, revision by digest. the two-halves shape is 0023's, applied at the type layer.

### what a declaration's digest covers

a declaration's digest is a domain-separated hash over its coordinate, its kind, and the canonical encoding of its body — the representation type for a nominal, the sorted constructor set for a sum, the target for an alias. self-reference inside the body (see recursion below) encodes as a dedicated cut tag, not as a nested digest.

three things must not move it, on Dhall's ground that a digest must not move for what no reader can observe: a doc-comment change, declaration order in the table, and formatting. the table's registration order is not content and does not participate; two tables holding the same declarations in different orders derive the same digest per declaration. one thing must move it: a change to what the declaration says — the representation type, the constructor set, a payload's type. these claims are tested against the real population, not invented fixtures: xylem's six nominal declarations and phloem's five declared sums and nominal set are built from tables in this round. the sums are `phloem.Source`, `phloem.Origin`, `phloem.Range`, `phloem.Preference`, and `phloem.Resolution`, all constructed through the one `sum_type` helper in `codec.rs`, which is the seam the migration goes through.

### the type carries its declaration; the value keeps its name

`Type::Nominal` carries its declaration, not a bare name. `Type::Sum` likewise: a declared sum's use-site type is the reference to its declaration, so the constructor set lives in the table and once in the type, not twice. this is what closes the representation hole: `is_type` verifies, in order, that the value names the declaration's coordinate and that its representation inhabits the declared representation type — the second check 0026 promised "when the declaration lands." `Value::Nominal { name: "xylem.Object", representation: Text(...) }` against the declared-over-`Blob` type is refused at the same gate that already checked the name.

values keep carrying names rather than declarations, deliberately. a value is data; any crate can construct one that claims a name, and the type side — the declaration the expected type carries — is the side that can check. the residual after this round is exact: a fabricated value with a correct-shaped representation is indistinguishable from a genuine one, which is inherent to values being data and is the same residual every capability-free type system has.

carrying the declaration in the type rather than resolving through an ambient table is a real choice, not convenience: `is_type` stays a total function of its two arguments with no table parameter, and a decoded type — the durable-record case — retains full checking power, because the encoding carries the body. the cost is bytes: a nominal reference encodes its module, name, kind, and body rather than a digest. pre-release, bytes are cheaper than an ambient resolver would be in correctness risk.

`value_type` cannot recover a declaration — a value's name is a string, not a table lookup — so it synthesizes the best-effort declaration its own content supports: the module split off the coordinate string, and the representation's best-effort type as the declared representation. for fully determined representations the synthesized declaration is the declared one; the empty-list and singleton-sum asymmetries 0026 documents survive unchanged, and reflexivity — every value inhabits its own `value_type` — still holds.

### recursion is a property of the encoding, not a taste

nominal declarations and declared sums may be recursive. the recursive occurrence inside a declaration's body is spelled as a cut — an occurrence of the declaration being declared, encoded as a dedicated tag — which is what the calculus's own construction predicts: nominal identity is a coordinate rather than a structure, so a recursive occurrence encodes as a coordinate instead of expanding forever. `xylem.Tree = Node(List<Tree>) | Leaf(Int)` is a finite canonical form because the inner `Tree` is the cut, and a value of it type-checks in time proportional to the value, not the type.

structural aliases may not be recursive. an alias has no coordinate spelling — referencing one yields its target — so a recursive alias has no finite canonical form: expansion is its only semantics and the expansion does not terminate. the table refuses one at registration with a diagnostic that names the cycle, and mutual alias cycles are unconstructible rather than merely refused, because a body may only reference already-registered declarations plus itself. this is the same line the calculus already draws between nominal and structural, stated as a property of the canonical encoding.

one consequence is accepted and recorded: mutual recursion between declared sums is unconstructible under ordered registration, since each sum's body would need the other's digest. direct self-recursion covers the build-domain shapes (trees, linked structures); forward references belong to the module-system work if a measured need appears.

### Int is arbitrary precision, in the round that computes with it

> the argument below stands and its landing moved. arbitrary precision is deferred to the record that lands arithmetic, on the grounds given under serialization: it has no consumer here, and its incidental surface is larger than the declaration table's. `Value::Int(i64)` stands until then.

0026 says `Int` is arbitrary precision; `Value::Int(i64)` implemented 64-bit. the record picks arbitrary precision, and the codec matches when it lands: a sign and a length-prefixed magnitude.

the argument that decides it is totality, 0018's ground. `Int` closed under addition and multiplication makes arithmetic total, and 0018's claim that the pure fragment terminates by construction then covers arithmetic wholesale — the empty-cache equivalence holds because no input can leave the finitary operations, not because someone inspected them. a fixed width makes arithmetic partial — overflow is a run-time failure 0018 would have to admit as a backstop on pure code, which its own text refuses ("a limit ... is never the primary defense against non-termination in pure code"). bounds on integers remain what 0026 says they are: a library concern via validation, not a scalar proliferation.

no arithmetic lands in this round — the kernel computes nothing over `Int` yet — so the totality argument is prospective, and that is the reason the representation waits for it. the round that lands arithmetic lands the representation, the round-trip beyond the 64-bit range, and the stable digest together, where the totality claim can be measured rather than stated.

### a rule's revision derives from the declarations its interface names

a rule whose interface names declarations derives its revision manifest from them: the canonical interface encoding, whose nominal and sum participants now carry their declarations' bodies, plus the sorted digests of every declaration that encoding reaches. changing `CSource`'s representation moves the compile rule's revision with no author edit; changing an unrelated declaration does not; changing the interface's own shape moves it through the encoding half. `rule_revision`'s constant — the current `b"xylem-v3"` — loses its job, which is the reason this round exists.

the granularity claim is honest about what it does not cover: a host-tier rule body can change while its interface and every declaration it names stay fixed, and a derived manifest will not move. that is 0023's recorded conservatism for the host tier (its manifest was to include a provider digest; xylem never had one, only the constant), now narrowed from "one edit moves every rule" to "an interface-level change is automatic, a body-level change is not." the body half arrives with 0038's represented tier, whose revisions derive from canonical ir. false invalidation remains acceptable and reuse across a semantic change remains unacceptable, in 0023's words, and this derivation moves the line strictly toward precision.

### the constructor-set audit

0026 closes the constructor set and supplies its own admission test: "does any engine subsystem (scheduler, cache, capability checker, policy engine, provenance) read this state structurally?" applied honestly, with three outcomes rather than two.

leaves the set: `Map<K, V>`, `Option<T>`, `Result<T, E>` as calculus primitives. no engine subsystem reads any of them structurally — the scheduler, cache, and policy engine dispatch on interfaces and effect categories, none of which is a keyed lookup, a maybe, or an either. `Option` and `Result` are spellable as declared sums the moment a library wants them, which makes primitives redundant twice over; `Map`'s keyed-lookup uses are host-tier (0040's solver holds them internally and spells the value as a sorted list, which was already a second refusal). if a subsystem ever reads a keyed container structurally, it returns by amendment, on the same terms it leaves.

leaves the set: the five effect categories as value-type constructors. this is the over-read the audit finds in 0026. 0019 puts the categories in the kernel ir, and they are there: `Request<K>`, `Rule<K>`, and the two rule arenas are the ir positions, typed by the phantom parameter, and 0022 fixes where each executes. no `Value` has ever had type `Action<Object>` — the step machine resumes bodies with plain values, and effectful facts ride provenance and diagnostics, not value shapes. "the calculus represents it" (0026) was reading 0019's ir types as value types. the categories stay exactly where 0019 and 0022 put them and lose nothing; the value-type constructor list stops claiming them.

stays in the set, unbuilt: the five uncertainty constructors, `Unknown`, `Unchecked<T>`, `Stale<T>`, `Conflicted<T>`, `Unreachable`. they pass 0026's test as designed but not as measured — the subsystems that would read them structurally (the cache's staleness surface, the merge operator's conflict promotion, the observation tier's unreachability, the surface's gradual consistency) are themselves unbuilt, and 0033's revalidation already shows staleness living as a graph fact on durable records. each stays gated on the subsystem that reads it: `Stale` and `Conflicted` on the merge operator and invalidation surfaces, `Unchecked` on the first validation boundary, `Unknown` on the surface's gradual typing, `Unreachable` on the observation tier. landing one without its reader would repeat the `Type::Nominal` history — a declared, encoded, digested constructor nothing inhabits.

built now: the declaration table itself with its three kinds, and the recursive cut. nothing else — `Int` moves to the arithmetic round for the reason given above. 0026's amendment is a net shrink of the primitive set, which is what an honest application of its own test yields.

### serialization does not move

> amended by [0048](0048-pre-release-version-pinning.md), which pins every version in the tree at 1 until the first tag. the paragraph below planned two bumps; neither happens. what the round still does is discard the pre-release database, which was always the mechanism — 0048 makes it the only one.

`ENCODING_VERSION` stays at 1 while the grammar it gates changes: `Nominal` and `Sum` grow to carry module, name, kind, and body, and the recursion cut takes a tag. `RECORD_ENCODING_VERSION` stays at 1 for the same reason, because the retained-value grammar grew with no released reader to misread it. the pre-release database is moved aside and rebuilt, which the existing test asserts, and that discard is what a version mismatch would otherwise have announced.

the `Int` shape is no longer part of this round. arbitrary precision has no consumer — this record says so itself, "no arithmetic lands in this round" — and it carries the largest incidental surface of anything here: an arbitrary-precision dependency in a kernel with few, or a hand-rolled magnitude under `arithmetic_side_effects = "deny"`, a variable-width canonical encoding, and digest stability for every `Int`-bearing value. it belongs to the record that lands arithmetic, and `Value::Int(i64)` stands until then. the totality argument this record makes for arbitrary precision is unaffected by the delay and moves there with it.

the digest domains stay at `v1`. nothing released has ever persisted a digest over the old manifest shapes, so there is no state to protect and nothing to migrate; bumping the domains would be ceremony over an empty warehouse. the Dhall 1.17.0 lesson is carried as the rule for when there is a release: from then on, a change to what participates in any digest is a domain bump or an explicit migration, never a silent basis change — which is 0023's rule, restated with the precedent attached.

## alternatives considered

### unison's content-addressed declarations

identity is the digest of the elaborated declaration; names are metadata.

strongest published answer to the question, and pith agrees with it everywhere behavior is the meaning. rejected for the type layer on the distinctness requirement: `MachineId = Text` and `ClientId = Text` must be two types with identical content, which content identity cannot express. also couples type identity to the elaborator's representation, the coupling whose price Dhall's basis break records.

### bare-name interning

keep `Type::Nominal { name }` and intern names in a global registry that maps a name to its representation.

the current state plus a lookup. rejected on two grounds: a global name registry is a flat namespace in exactly the shape 0026 rejects for variant tags — cross-module collisions become someone's problem with no coordinate to settle them — and it still leaves revision derivation with nothing per-declaration to digest that a name change does not also move.

### wit-style registry-qualified identity with versions

coordinates as `namespace:package/name@version`, with semver governing compatible change.

the right shape for published ecosystems, and the reason it is not adopted is scope: pith has no registry, no publisher, and no released versions, and adopting the semver contract now would be paying for an authority that does not exist. the coordinate this record builds — module identity plus name — is the prefix of that model that can be honored today; the version half belongs to the module-system record that 0023 and 0038 both defer to.

### ambient table resolution instead of carrying declarations

types carry bare coordinates; `is_type` resolves the declaration through a table parameter.

the GHC shape — types carry names, the environment resolves. rejected for this round because it moves the table into every checking signature (`is_type`, `validate_inputs`, every codec test), and a decoded type would check against whatever table the reader happens to hold — the durable-record path would re-open, at a distance, the hole this round closes. carrying the body in the type costs bytes and buys totality; that trade is right while nothing is released, and the module-system work can revisit it when names have a compatibility model.

## consequences

the representation hole closes at the type side: a value naming `xylem.Object` with a non-`Blob` representation no longer inhabits any interface declaring it, at request validation and result checking alike, and the first test of that is the one that demonstrates the hole in the committed tree before the change.

every nominal and sum construction site migrates from string literals and inline constructor sets to table references: xylem's six nominal types and six interfaces, phloem's five declared sums and nominal set. the migration is smaller than the raw construction-site count suggests: of roughly 102 `Type`/`Value` `Nominal`/`Sum` sites in non-test source, 48 are inside `pith-core`'s own `value.rs` and `value_codec.rs` — the definition and the codec, which this round rewrites rather than migrates. the domain surface is about 26 in xylem, 24 of them in the single `types.rs` that becomes xylem's declaration table, and about 22 in phloem, at most four per file after `8ef5dc4` centralized its sum construction. the value constructors stay unchanged — values carry names, and the names are the coordinates the declarations now own. computation keys move, because canonical encodings changed shape; pre-release, that is the moved-aside-and-rebuilt path the record above states.

the `b"xylem-v3"` constant and phloem's per-rule constants after it lose their reason to exist as granularity; phloem's constants survive this round unchanged (its rules are host-tier like xylem's, and its migration to derived revisions follows the same path xylem's takes here). host-tier body changes remain covered by discipline until 0038's tier lands.

`value_type`'s best-effort synthesis means a diagnostic naming a nominal type renders the coordinate plus synthesized representation, which may differ from the declared representation when the value's content is indeterminate (the documented list and sum asymmetries, unchanged).

what a module identity is beyond a string accepted at registration, and how two repositories' modules relate, stays open with 0023 and 0038. the coordinate's dotted spelling means a module identity containing a dot could collide with a longer name in a dot-free module; the prototype's population (single-segment module identities) does not reach it, and the module grammar belongs to the module-system record.

### measured

the declaration table, the coordinate, the digest, `Type::Nominal` and `Type::Sum` carrying their declarations, `is_type` verifying the representation, and the recursion cut are built. xylem's six nominal types are declared in one table in `types.rs`, and the eighteen inline `Type::Nominal { name }` sites in its interfaces derive from it; phloem's five declared sums and its `VersionScheme` nominal go through `declarations.rs`, which registers lazily and derives each declared name from the coordinate spelling the crate's constants already held. 675 tests pass.

the hole is closed from the other side. `crates/xylem/tests/declaration_hole.rs` asserted that a value naming `xylem.Object` while holding a `Text` inhabited the link interface; it now asserts the refusal, at `is_type` and at the request-input gate, with a companion asserting a genuine object still passes so the refusal is not the check rejecting everything.

the table's own claims are tested against the real population rather than invented fixtures: registration order does not move a digest, two modules declaring one short name are two declarations with two digests, a changed representation and a changed module each move one, a changed constructor set moves a sum's, an alias yields its target expanded, a recursive alias is refused while a recursive nominal is not, and a cut reached only through another declaration does not make an alias recursive.

recursion is measured on a value: `test.Tree` over `List<Cut>` type-checks a nested value, and a wrong representation at depth is still refused, so the cut carries the check down rather than waving it through.

two things the round found that the record did not predict. `Coordinate::spelling` did not invert `Coordinate::parse` for a dotless name — it produced `.name` — which broke reflexivity for a nominal value whose name carries no module; the reflexivity test is what caught it, and spelling now omits the dot when the module is empty. and carrying the whole `PureComputationKey` on an evaluation frame, which an earlier round had done, grew two enums past `large_enum_variant`; the frame carries the digest alone.

one consequence landed earlier than expected. phloem's `a_toolchain_whose_representation_is_not_the_driver_path_is_refused_on_read` asserted that the nominal type *could not* see the representation and that the reader had to. the declaration makes that false: the refusal now happens at the reader's type guard rather than at its driver-path branch, one layer earlier and for every consumer rather than only the readers that thought to check. the test is inverted and the driver-path diagnostic survives as defense in depth.

### measured: the derived revision

`Rule::declared(module, label, interface, span)` derives both halves of 0023's identity: the coordinate from the module and label, and the revision from the canonical interface encoding. All nine xylem rules and all three phloem rules use it, and `grep` for a hand-written revision manifest across both libraries returns nothing. `b"xylem-v3"`, `b"phloem-resolve-v1"`, `b"phloem-package-build-v2"`, and `b"phloem-package-library-v1"` are deleted.

phloem's constants were to survive this round on the reasoning above. They did not, because the mechanism turned out to be generic and migrating them was one line per rule: leaving three hand-bumped constants beside a derivation would have been the second mechanism the principles forbid, in the crate 0049 showed was the one that mattered — phloem's revisions not moving when xylem's did is what made the stale-hydration case reachable across the library boundary.

`crates/pith-core/src/rule.rs`'s `derived_revisions` tests hold the granularity claim in both directions: declaring `CSource` over `Text` instead of `Blob` moves the revision of a rule whose interface names it, with no author edit; declaring an unrelated type, and changing what it is, moves nothing; a changed interface shape moves it through the encoding half; and the identity is the coordinate and survives a revision move. `crates/xylem/tests/derived_revisions.rs` holds it over the real population: the four pure entry rules have four distinct interfaces and now four distinct revisions where they shared one constant, each rule's identity is its coordinate, and the compile entry's interface reaches `CSource` and `Object` and does not reach `TestReport` — so an unrelated xylem declaration cannot move a compile's cache key. 690 tests pass.

one departure from what this record specified. the revision manifest is the canonical interface encoding and nothing else; the "plus the sorted digests of every declaration that encoding reaches" half is dropped as redundant, and the reason is this record's own first half. because a nominal or sum type carries its declaration rather than a bare coordinate, the encoding already contains every declaration body the interface reaches — so no declaration change can move a digest without moving the encoding, and no encoding is reachable from two different declaration sets. adding the digests would be a second mechanism for one concern. if a later record adopts the ambient-table alternative this record rejected, where a type carries a bare coordinate, the digest half becomes load-bearing and the manifest has to grow it; under 0048 that is a digest-domain bump rather than a silent basis change, and the reasoning is recorded at the derivation.

## unresolved

forward references and mutual recursion between declared sums need either a two-phase table or a module-level elaboration pass; deferred until a shape that needs them is measured.

the surface syntax for declarations — the `.pi` file — is untouched, as are type parameters on declarations, which 0026's nominal section carries as "an optional set of type parameters" and which no population has yet needed.

whether the declaration digest's inputs ever need a version separate from the encoding version — the same question 0026's unresolved section carries for the type serializer — is unchanged. the Dhall precedent is recorded as the rule that answers it when it next arises.
