---
schema: design-doc/v1
id: research-declarations
title: declarations
summary: how four systems give a declaration its identity — protobuf's and cap'n proto's ordinals, WIT's package-qualified names, Dhall's semantic hash, and Unison's content-addressed definitions — and what each refused to give up
kind: research
status: researching
evidence: preliminary
created: 2026-07-20
updated: 2026-07-20
tags:
  - research
  - types
  - language
  - identity
relations:
  informed_by:
    - research-configuration
    - research-build-systems
  depends_on:
    - research-method
  supersedes: []
---

# declarations

a type declaration forces two questions a value never asks: what is this declaration's identity, and what happens to that identity when the declaration changes? pith's calculus put every nominal type behind these questions in 0026 and then deferred the declaration site — three records point at the gap from different directions. 0023 split rule identity from rule revision and needs a declaration coordinate to hang the identity half on. 0038 named the module's declaration table as where that coordinate lives, without building it. 0026 landed constructor after constructor while saying the declaration site "does not exist yet."

the question splits into two decisions the precedents answer independently. by what is a declaration named — an ordinal, a coordinate, or a digest of its content. and by what is a declaration's revision tracked when its content changes — the same thing, a second thing, or nothing at all. this note reads four systems at those two questions from their primary documents. they disagree, and the disagreement is not about encoding: it is about whether a declaration's identity is something an author chooses, something a registry assigns, or something the content computes.

## protobuf and cap'n proto: identity is the ordinal, and the author manages it forever

protobuf is the oldest lineage here and the one with the most operational miles. its answer is stated in one sentence in the proto3 guide: a field's number "cannot be changed once your message type is in use because it identifies the field in the message wire format." the number is the identity; the name is presentation. renaming a field is safe because the name never reaches the binary wire — it matters only "in JSON and TextFormat, where the field name is serialized." reordering fields in the source is safe because the numbers travel with them. the type is a third, weaker axis: changes like `int32` to `int64` can be "wire-compatible," meaning the same bytes parse both ways, possibly lossily.

the discipline the model demands is documented as a set of prohibitions whose violation mode is named: "Field numbers should never be reused. Never take a field number out of the reserved list for reuse with a new field definition," because the lean wire format has no mechanism to detect a mismatch, and the guide lists the consequences — debugging time, parse errors, "Leaked PII/SPII," "Data corruption." deletion is not removal but reservation: "you must reserve the deleted field number," and the compiler enforces it: "The protoc compiler will generate error messages if any future developers try to use these reserved field numbers."

cap'n proto, designed thirty years later by a protobuf author, kept the answer and formalized the discipline. the evolving-your-protocol section of the language guide states the invariant in its most compressed form: "You cannot change a field, method, or enumerant's number." names are explicitly demoted — "Any symbolic name can be changed, as long as the type ID / ordinal numbers stay the same" — and source layout is explicitly irrelevant — "Members can be re-arranged in the source code, so long as their numbers stay the same." what cap'n proto added over protobuf is the explicit ID for types and the `@N` annotation as a visible record: "The @N annotations show how the protocol evolved over time, so that the system can make sure to maintain compatibility." a type without an explicit ID has an implicit one derived from its name, and therefore cannot be renamed — the guide says exactly that, which is the ordinal model's one regression: an author who declines to choose an identity gets one derived from a presentation detail, and loses the rename freedom the model otherwise grants.

what this lineage refused to give up is the cheap wire format and the compatibility guarantee that old readers and new data interoperate in both directions. what it paid is permanent bookkeeping: reserved-number lists, append-only numbering, and a failure mode — silent corruption — that only manifests far from its cause. nothing in the format detects a reused ordinal; the detection is procedural.

the version half of the question these systems barely answer. a schema is versioned as a whole file or package, compatibly or not, and the ordinals are what make "compatibly" possible. there is no per-declaration revision concept at all: two schema versions are related by the diff of their ordinals, and readers cope.

## WIT: identity is a package-qualified name, and a registry sits behind it

the webassembly component model's interface definition language answers for the modern multi-language case. a WIT package is `namespace:package@version` — `wasi:clocks@1.2.0` — where the namespace exists to "disambiguate between registries, top-level organizations, etc." every interface inside is reachable as `namespace:package/interface-name[@version]`, and `use` is "not implemented by copying type information" — it stays a reference, so a type used from another interface is still that interface's type, reached transitively.

identity is the fully qualified name plus, when present, the version. it is not a hash: the spec document contains no content-addressing of declarations anywhere. what holds identity steady across change is a social contract rendered in annotations — `@since(version = x.y.z)` with the expectation that "once applied to an item, the item is not modified incompatibly going forward," `@unstable` for items that "may change type or be removed at any time," `@deprecated` paired with `@since` and resolved by a semver-major removal. at the binary layer, type identity flows through aliasing — an importer aliases an exporter's type rather than restating it, and a re-export can mark itself `(eq $b)` structurally equal to the original — but this is linking machinery around the name, not a replacement for it.

what WIT refused to give up is the human-named, registry-publishable package — the thing a wasi working group can debate, version, and deprecate in public, and that bindings generators for many languages can agree on because it is a name and not a language-native construct. what it paid is the semver discipline as an obligation the annotations only document, not enforce, and a name grammar (namespace, kebab-case, case-insensitive uniqueness per scope) that every tool must now police. the disagreement with the ordinal lineage is real but narrow: both keep author-chosen identity; WIT qualifies it by package where protobuf flattens it into one number space per message.

## Dhall: identity is a digest of the normal form, and the digest has a history

Dhall is the closest precedent for pith's actual need — a total, canonicalizable configuration language whose import integrity rests on hashing declarations. the semantic hash is defined by a pipeline, stated by the language author on the project's discourse: β-normalize ("ordinary evaluation," which "also includes sorting the fields of records"), α-normalize ("renames all variables to _ and uses de bruijn indices"), "convert the expression to a standard CBOR encoding," then "compute the SHA256 hash." the invariances follow from the pipeline: field order, binder names, formatting, and any expression difference that normalizes away do not move the hash. the α-normalization standard states the guarantee in one line — "If two expressions are α-equivalent then they will be identical after α-normalization" — and one limit: "An expression with unresolved imports cannot be α-normalized."

the binary encoding standard ties the hash to its two uses: users "can import expressions protected by a 'semantic integrity check', which is a SHA-256 hash" of the encoded normal form, and interpreters "can locally cache imported expressions" under it. the determinism rules are spelled out per constructor — record fields "should be sorted before translating them to CBOR maps," unions likewise, doubles must use "the shortest floating point encoding that preserves its value" with fixed encodings for NaN and signed zero "so that identical semantic hashes [result] on different platforms" — and the decoder is deliberately permissive where the encoder is strict: "A decoder MUST accept an integer that is not encoded using the most compact representation."

the hash-instability record is the part pith most needs. dhall 1.17.0, moving to language standard 2.0.0, states the breaking change plainly: "The hash used by the semantic integrity check is now based on the binary representation instead of a text representation of the" expression — every frozen semantic-integrity hash in every user's codebase moved at once, with a protocol-version pin as the migration crutch. the lesson is not that hashing is wrong; it is that a digest's basis is a versioned protocol in itself. a system that hashes its declarations has committed to treating "what participates in the digest" as an interface, with the same compatibility weight as the bytes it produces.

what Dhall refused to give up is integrity without a registry: any import can be pinned by a hash the importer computes, no index has to vouch for it. what it paid is the instability above, plus a normal form expensive enough that hashing is a pipeline rather than a serializer — Dhall must run the program before it can name it.

## Unison: identity is the content hash, and the name is metadata

Unison takes the position the other three all decline: a definition's identity is not chosen or assigned, it is computed. from the project's own account, each definition is named by "a hash of its syntax tree" after α-renaming and dependency resolution — "all named arguments are replaced by positionally-numbered variable references, and all dependencies" become hashes themselves, transitively. names are demoted completely: "names are just separately stored metadata that don't affect the function's hash," pointers into "a vast immutable address space" whose addresses are deterministic. because the definition associated with a hash "never changes," editing produces a new hash and the old one stays live — two versions of an `Email` type "exist as different types, with different hashes, and you can work with both at the same time."

the model buys what the account says it buys: no builds (everything cached forever, "this cache is never invalidated"), no dependency conflicts (the diamond problem dissolves because both branches reference hashes), durable typed storage of any value, and refactoring as a database operation over name-metadata rather than a text rewrite.

what it refused to give up is global immutability of definitions and the detachment of identity from authorship. what it paid is the whole runtime: a codebase manager that stores definitions processed rather than as text, a hashing scheme coupled to the compiler's internal representation, and an authoring experience that needs structured sessions for what other languages do with a file edit. the position is coherent and the argument for it is real — but note what it means for types specifically: two structurally identical declarations hash the same and are the same. Unison's account presents this as deduplication; for nominal types it would be collapse.

## what the lineages disagree on

on naming, two against two, and the split is exactly the split pith has to resolve. protobuf, cap'n proto, and WIT say a declaration's identity is something the author or the registry fixes — an ordinal, a qualified name — stable across content changes, with change tracked separately (protobuf: nothing per-declaration; cap'n proto: the `@N` record; WIT: semver annotations). Dhall and Unison say identity is computed from content, differing in what they hash — Dhall the normal form of an expression for integrity pinning, Unison the elaborated definition as the identity itself.

on revision, the ordinals have the deepest operational answer: identity and wire-compatibility are separate concerns, and conflating them is how formats die. Dhall's contribution is the invariance discipline — the digest must not move for anything a reader cannot observe — and its warning that the digest basis itself is versioned surface. Unison's contribution is the proof that content identity scales to a whole codebase, and the cost curve that comes with it.

none of the four settles the question pith actually has, which the next section states: identity by coordinate with revision by content digest, over a calculus whose nominal types exist precisely to make structurally identical declarations distinct.

## result for this project

adopt the invariant through a mechanism assembled from the lineages: a declaration's identity is its coordinate — module identity plus declared name, the WIT position in miniature, without the registry — and its revision is a domain-separated digest over what the declaration says, the Dhall invariance discipline applied to declarations rather than expressions. two nominal types over `Text` are distinct because their coordinates differ; changing a representation moves the digest and not the coordinate; doc comments, declaration order, and formatting do not participate, on Dhall's ground that a digest must not move for what a reader cannot observe.

reject Unison's position for the type layer specifically, not generally: content identity is the right answer for definitions whose meaning is their behavior, and pith keeps it for values (content identity) and will keep it for rule bodies when 0038's represented tier lands. it is the wrong answer for nominal declarations because 0026 requires two declarations with identical content — `MachineId = Text` and `ClientId = Text` — to be distinct types; identity by content hash cannot express a distinction the content does not carry. the ordinal lineage's failure mode — reuse of an identifier meaning silent corruption — arrives in pith as the wrong-representation value, and the round's checking closes it at the type side rather than by procedural discipline.

carry the Dhall instability lesson as a standing rule, already half-written in 0023: the digest basis is versioned surface, and a change to what participates in the declaration digest is a recorded event, not a silent refactor. keep the protobuf append-only instinct where it applies: constructor sets and record fields are name-keyed in pith rather than number-keyed, but the closed-set discipline — a name means one thing per module, forever within a digest — is the same discipline, enforced by the table rather than reserved in it.

leave open what WIT leaves open: the cross-repository, cross-version compatibility model for module identities, which is 0023's and 0038's standing deferral and not this round's to close.
