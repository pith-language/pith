---
schema: design-doc/v1
id: decision-0062-the-ir-constructor-set
title: the represented-body constructor set is enumerated, typed, and de Bruijn, with structural case on lists and the step protocol as its yield constructs
summary: thirty constructors over the 0026 calculus — pure expressions between yields, one request per shape of fan-out, and MatchList as the eliminator folds cannot replace — encoded in its own versioned grammar under the pith:body-ir digest domain, validated by typecheck against the rule's interface, and total by the absence of any recursion constructor
kind: decision
status: proposed
created: 2026-08-24
updated: 2026-08-24
tags:
  - language
  - ir
  - identity
  - encoding
relations:
  informed_by:
    - research-language-frontend
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0026-generic-typed-calculus
    - decision-0029-declared-independence
    - decision-0038-represented-rule-bodies
    - decision-0047-the-declaration-table
    - decision-0048-pre-release-version-pinning
    - decision-0055-arbitrary-precision-int
    - decision-0060-observation-identity-and-freshness
  amends:
    - decision-0038-represented-rule-bodies
  supersedes: []
---

# the represented-body constructor set is enumerated, typed, and de Bruijn, with structural case on lists and the step protocol as its yield constructs

> amends [0038: rule bodies are data in one kernel-facing core ir](0038-represented-rule-bodies.md): its unresolved section's first item — "the constructor set of the expression language between yields is not enumerated here ... that enumeration is the first design task this record gates" — is this record. the other three unresolved items are answered below: the evaluator-abi version is the `pith:body-ir` digest domain's version segment, the fan-out question splits into `NeedAll` and `NeedEach`, and binders are de Bruijn indices in the ir itself, so there are no binder names to normalize.

## context

0038 settled that a rule body is data and left the data unenumerated, gating surface syntax and its own acceptance on the enumeration. M-9 closed the vocabulary it depends on — `NeedObservation` is in `PureStep` — so the step half of the set is closed and can be encoded without an amendment arriving a record later. the corpus that measures the set exists and is small enough to read in an afternoon: fifteen pure rule bodies across xylem, stele, phloem and example-domain, whose between-yield computation is the whole demand signal the pure expression language has.

the enumeration discipline is 0026's and 0047's: a constructor arrives because a measured consumer needs it, and the set is closed under amendment rather than accretion. the corpus read found four constructors the a-priori design did not have — `NeedEach`, `MatchList`, `Describe`, `TextOfBytes` — and rejected a longer list, most of which had no spelling in any corpus body.

## proposed decision

### the shape

a body is one expression over the 0026 calculus, checked against its rule's interface: the inputs are the deepest binders and the expression must inhabit the output type. binders are de Bruijn indices throughout — `Bound(0)` is the most recently bound value — so binder names never exist, alpha-normalization is vacuous, and formatting cannot participate in a digest because there is no formatting to spare. spans and labels do not survive either, on 0038's own terms.

the set is first-order. there are no function values, no closures, no lambda: the corpus needs none, and a first-order grammar is one a validator can check bottom-up with structural equality, which is what keeps 0015's exact-match discipline honest at the body layer too.

### the constructor set

the pure expressions: `Literal` (an embedded value), `Bound`, `Let`, `Fail` (a `Text` message; the construct inhabits every type, the way a value-level failure must), `Record`, `Field`, `MakeSum`, `Match`, `Wrap`, `Unwrap`, `List` (with its element type — an empty list cannot recover one), `Cons`, `Append`, `Fold` (left to right; the step sees the element at `Bound(0)` and the accumulator at `Bound(1)`), `MatchList`, `SortBy` (stable, comparing the canonical encoding of each element's key), `If`, `Equal`, `IntAdd`/`IntSubtract`/`IntMultiply` (0055's three; division keeps its absence), `Describe` (the diagnostic rendering — decimal integers, digest-named blobs — which is how a body names what it refuses), `TextConcat`, and `TextOfBytes` (UTF-8 decode; invalid bytes fail the body).

the yield constructs, one per shape the corpus actually requests: `Need` (one pure computation), `NeedAll` (a static batch that may mix interfaces — the shape compose-system's three merges and three renders have), `NeedEach` (one interface, one request per element of a runtime list, with the declared independence of a batch on 0029's terms — the shape a package build's per-source compiles have, which no static batch can spell), `NeedBlob`, `NeedAction`, and `NeedObservation`. each request carries its full `Interface` in the body, on the ground 0047 fixed for types: no ambient rule table resolves a request, so a body is self-describing data a decoded copy can check as well as a constructed one.

`Fail` and `TextOfBytes` are the only failing constructs, and both are deterministic in their inputs — a failure is a value, not a divergence, and the empty-cache equivalence 0018 claims survives them the way it survives any other function of the inputs.

### why MatchList: the wall the corpus found

folds alone cannot express grouping, first-match, or head-extraction, and the reason is structural rather than ergonomic: a fold's accumulator starts at a value, and under strict evaluation the init is evaluated whatever the list holds. "the first carrier's value" has no value to start from — an init of `Fail` fails the whole fold even when the list is non-empty, and a fabricated default corrupts the semantics — so a list whose head is data-dependent has no fold spelling. `MatchList` is the structural eliminator the two list constructors already imply: `empty`, or `cons` under the head at `Bound(0)` and the tail at `Bound(1)`. it is not recursion — nothing in it re-enters itself — and with it the corpus's grouping becomes the direct translation of the host's peekable loop, in linear time, where the fold-only spelling was quadratic *and* wrong.

### totality is an absence

0018's construction becomes a property of the set: there is no recursion constructor, repetition is a structural fold over a finite list or a case on one, and every primitive is total. a body can fail; it cannot diverge. the validator therefore checks totality by checking nothing — the absence is in the grammar, and a body that arrives from a file cannot spell a loop the way it also cannot spell a filesystem.

### identity, revision, and the encoding

`Rule::represented` derives the revision 0038 promised: a manifest of the body's digest under `pith:body-ir:v1` followed by the interface encoding, hashed under `RuleRevision`'s domain — the same two-halves shape a host rule's `BodyRevision ‖ interface` manifest has, with the author's counter replaced by the derived digest. the body's types carry their declarations (0047), so the encoding already contains every declaration body the body reaches and no second digest list is needed — the same redundancy 0047's derived revision refused. the coordinate stays the identity, and a compatible refactor is unexpressible: there are no binder names, so two elaborations of one body are one tree.

the encoding is a grammar beside `Value` and `Type`, not a rider on `RECORD_ENCODING_VERSION`, exactly as 0038's serialization section requires: tag-numbered in its own namespace (thirty tags), length-prefixed, depth-bounded at 128 on both the decode and the validate side so a file-delivered body cannot overflow the stack, version byte 1 pinned under 0048, digests domain-separated the way `pith:action-computation:v1` is. embedded values and types ride as length-prefixed payloads of the existing encodings, which is the same seam the declaration table embeds declarations through. the domain's version segment *is* the evaluator-abi version 0038's unresolved section asks for: a change to evaluator semantics — anything that would move what a body means without moving its bytes, such as the sort algorithm behind `SortBy` — is a domain bump, never a silent basis change.

### what the round names as unexpressible

three corpus bodies cannot be spelled, each with its reason. `xylem.compile-entry` and `example.render-entry` wait on the same constructor: text splitting. the depfile parse splits bytes on whitespace, joins continuation lines, and strips prefixes; the template scan walks `{{`..`}}` delimiters by index. splitting has no agreed total semantics to encode — empty fields, adjacent delimiters, and unclosed delimiters each need a decided answer before a constructor exists — so both bodies stay host-tier until a record decides one. `phloem.resolve` is out on two grounds at once: version ordering is a host trait object selected by a request-visible name, which is dispatch the ir cannot see, and the search itself is backtracking with an undo stack — general recursion, the thing 0018 excludes by construction and this set excludes by absence. both halves stay host-side, which is 0038's permanent tier doing what it exists for.

one expressed body carries a named divergence: `stele.render-passwd` narrows uid and gid to a machine integer because its formatter takes one, refusing ids outside that range; the represented body renders the arbitrary-precision decimal through `Describe` and accepts what the host body refuses. the narrowing is a formatter boundary, not a designed contract, and integer comparison — the constructor that would restore the refusal — stays out until a domain needs it for its own sake, on 0055's rule that a partial operation arrives with the consumer that can answer its edge.

## alternatives considered

### a larger primitive set

`Map`, `Option`, `Result`, `Length`, `Compare`, `Substring`, `BoolOr`, `Dedup` — each spellable by what is there, each with no corpus demand. `Map`/`Option`/`Result` left 0026's set by 0047's audit; the rest are folds over the constructors that exist. rejected on the audit discipline: an unread constructor is 0047's `Type::Nominal` history waiting to repeat.

### comparator-based sort instead of key-based

a sort taking a two-argument comparator expression is more general than `SortBy`'s decorate-key form. rejected on canonicality: a comparator's result participates in the order without participating in the digest of what it orders, and an inconsistent comparator makes the sorted result a function of the sort algorithm's internals rather than of the input. key-based sort with canonical-encoding comparison has one deterministic answer for one input, which is the property a body digest needs behind it.

### de Bruijn levels or a named-binder form with an alpha-normalization pass before digesting

0038 left the choice open with Dhall as the precedent either way. levels and names both exist to make construction ergonomic; this round's construction happens through an elaborator that does not exist yet and hand-built trees that do, and names would add a normalization pass whose only job is to remove what construction added. indices are the choice with nothing to normalize.

### resumption as one list binder for NeedAll

binding `Many`'s values as a single `List` binder is closer to the protocol's shape. rejected on typing: a heterogeneous static batch would hand the continuation an untyped list, and every downstream use would lose the static types the interfaces already carry. one binder per request keeps the continuation fully typed at no cost to the protocol, which never sees the binders at all.

## consequences

the kernel gains a second body tier's data half: `RuleTier::Represented`, a `Rule::represented` constructor whose revision derives from the body digest, the validator that checks a body against its interface, and the canonical codec. no evaluator lands with it — M-12's interpreter and M-13's notation are the readers this set was enumerated for, and 0038's own migration rounds follow the notation.

the measured claim is the corpus: twelve of the fifteen bodies are expressed, validated, and round-tripped in `crates/pith-core/tests/corpus_bodies.rs`, against interfaces and declarations mirrored coordinate-for-coordinate from the live tables; the three that cannot be are named there with the constructors they wait for. the two text-splitting waits share one future record; the resolve case is permanent host tier unless a record amends this one with a bounded-search constructor, which 0018's open list still holds a place for.

the corpus test also carries the round's honest residues as comments: the typed body refuses a replacement naming a list field, where the untyped host body lands the text and fails later at a decode gate; and it refuses a `concat`-named text field at the body, where the host fails it at the same discovery. both refusals happen strictly earlier and name the same fact.

## unresolved

> the two evaluator sentences below aged within the hour: the interpreter landed as the round's
> follow-on engine commit, and its stability test pins `SortBy`'s committed order — the price is one
> `Vec::sort_by`, which is stable by contract. the `pith:body-ir` domain stays at v1.

the text-splitting constructor: its semantics (empty fields, adjacency, unclosed delimiters) need a record before either waiting body can spell its parse, and the two bodies that wait for it are the argument for designing it soon.

`SortBy` commits the evaluator to a stable sort algorithm as part of the domain's v1 meaning; the commitment is recorded here and priced by nobody yet, because no evaluator exists. M-12 prices it.

a body for an action rule — a represented `plan()` — is outside this set's scope and outside 0038's, as the language-frontend proposal's amendment list already names. the yield constructs here request actions; they do not plan them.

whether `NeedAll`'s static batch and `NeedEach`'s dynamic one eventually share a parent form — a batch whose shape is one expression — is open with 0029's wider question about what fan-out constructs the ir needs. no corpus body has wanted the third combination, a dynamic batch over mixed interfaces, and it is not spellable today.
