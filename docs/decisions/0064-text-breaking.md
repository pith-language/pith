---
schema: design-doc/v1
id: decision-0064-text-breaking
title: text breaking and joining are total, keep empty fields, and treat an empty separator as no match
summary: add TextBreak and TextJoin body constructors — split preserving leading, trailing, and adjacent empty fields with an empty separator matching nothing, and the join that re-joins those fields — enough to express both corpus parsers without adding a new value type, with the join a primitive because the fold spelling is quadratic
kind: decision
status: proposed
created: 2026-08-28
updated: 2026-08-28
tags:
  - language
  - ir
  - text
  - identity
relations:
  informed_by: []
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0038-represented-rule-bodies
    - decision-0062-the-ir-constructor-set
  amends:
    - decision-0062-the-ir-constructor-set
  supersedes: []
---

# text breaking and joining are total, keep empty fields, and treat an empty separator as no match

> amends [0062](0062-the-ir-constructor-set.md), which left `xylem.compile-entry` and
> `example.render-entry` outside the represented corpus pending one decided text-splitting semantics.

## context

The two waiting bodies need the same smaller primitive. A depfile parser separates fields and strips a
prefix; a template scanner separates on `{{` and then on `}}`. Substring indexes, regexes, and a family of
prefix/containment primitives would each enlarge the evaluator with more partial edge cases. A split that
returns ordinary `List<Text>` composes with the existing `MatchList`, `Fold`, `Equal`, and `TextConcat`
machinery and makes all of those operations derivable.

0062 refused to add it without deciding the empty cases because represented semantics participate in the
body digest. Delegating those cases to a host library would make `pith:body-ir:v1` mean whatever that
library happened to mean.

## decision

`BodyExpr::TextBreak { text, separator }` has type `Text × Text -> List<Text>`. It splits on every
non-overlapping occurrence and keeps empty fields. Its complete semantics are:

| text | separator | result |
| --- | --- | --- |
| `""` | `","` | `[""]` |
| `"a"` | `","` | `["a"]` |
| `",a,"` | `","` | `["", "a", ""]` |
| `"a,,b"` | `","` | `["a", "", "b"]` |
| any `text` | `""` | `[text]` |

Thus a break always contains at least one field, adjacent separators remain observable, and joining the
fields with a non-empty separator reconstructs the original text. An empty separator never matches. It is
not an error and does not enumerate Unicode boundaries; returning the whole text keeps the constructor
total without inventing a second operation under the same spelling.

The canonical body grammar gains tag 30. The digest domain remains `pith:body-ir:v1`: the constructor is
additive, no existing bytes change meaning, and the pre-release pinning rule in 0048 does not move a
version merely because a previously unknown tag becomes known. No new `Value` or `Type` variant is added.

The surface builtin is `split(text, separator)`. `before`, `contains`, and `strip_prefix` elaborate into
`TextBreak` plus the existing sum-free list machinery; `holds` and `sort` similarly elaborate into
existing fold and sort constructors. They are notation conveniences, not kernel constructors — with one
exception the measurement below forces.

`BodyExpr::TextJoin { list, separator }` has type `List<Text> × Text -> Text`. It places `separator`
between adjacent fields and nothing else: an empty list joins to the empty text, a single field joins to
itself, and an empty separator joins to the plain concatenation. Joining the fields of a break with the
same non-empty separator reconstructs the text exactly, which is the round trip the two constructors
exist for. It is the thirty-second constructor, tag 31, additive on the same terms as tag 30.

The reason it crosses the kernel line is cost, not expressibility. The fold-of-concatenations spelling
`strip_prefix` first shipped with re-copies its accumulator at every step — `TextConcat(TextConcat(j,
sep), field)` allocates the whole prefix of the result once per field, quadratic in the field count on a
text whose separators are dense. The evaluator's allocation behavior is part of what a body's bytes mean
under reuse, and 0018's totality asks for the total cost too, so the total-by-construction spelling is
the primitive. `strip_prefix` now joins through `TextJoin` and is linear in its input; `before` and
`contains` keep the head-of-split spelling, which is linear and does not re-join.

## alternatives considered

### use the host language's whitespace split

Rejected because its delimiter is a character class rather than a value and its empty-field behavior is
different. Xylem's written depfile body deliberately narrows the host implementation to the single space
the fixture's `make` output emits; that divergence is named by the corpus test.

### add substring indexes or regexes

Rejected because neither corpus body needs an index value or a pattern language. Both add bounds, Unicode,
and failure semantics that the consumer cannot price. The list result is already a type the IR can inspect.

## consequences and evidence

Both formerly waiting bodies are represented. `xylem.compile-entry` reads the depfile blob, joins
backslash-newline continuations, takes the dependency fields, strips one `./`, sorts them, and requests the
compile action. `example.render-entry` reads and decodes the template, extracts placeholder names, refuses
missing or repeated bindings, and requests the render action. Their written bodies live in the two corpus
modules and elaborate to the same hand-built bodies measured in
`crates/pith-core/tests/corpus_bodies.rs`.

The corpus is now fourteen represented bodies and one permanent host-tier body, `phloem.resolve`, whose
host dispatch and general recursion are unrelated to text breaking. Codec round trips, validator type
checks, evaluator edge cases, and the two corpus bodies pin the new semantics.

The join's cost claim was measured against the spelling it replaced before this record landed: at 8,192
fields, the fold-of-concats join evaluated in about 15 ms where `TextJoin` evaluated in about 4 ms, and
the gap widens with the field count because the fold is quadratic while the primitive is linear.

## unresolved

The xylem body accepts only the single-space depfile spelling the measured producer emits, while the old
host helper accepted every Unicode whitespace. It also removes one leading `./`, not arbitrarily many.
Those are explicit corpus divergences, not unspecified `TextBreak` behavior; broader depfile syntax should
arrive with an input that needs it.

