---
schema: design-doc/v1
id: planning-surface-notation
title: the surface notation
summary: five request constructs matching the step protocol, rules whose interface derives from the signature, metadata split by whether anything reads it, and declared entry points
kind: planning
status: draft
created: 2026-08-18
updated: 2026-08-24
tags:
  - planning
  - language
relations:
  informed_by:
    - research-language-frontend
    - research-dispatch
  depends_on:
    - planning-language-frontend
    - decision-0015-interface-rule-selection
    - decision-0018-termination-and-recursion
    - decision-0026-generic-typed-calculus
    - decision-0052-the-merge-operator
  supersedes: []
---

# the surface notation

this is the surface notation, round four of [the language frontend](language-frontend.md).

## the calling convention

five request constructs, one per step variant, so the surface's yield vocabulary and the kernel's are the
same five words and no construct elaborates to two steps:

```
ask  T (e, …)                                          -- Need
run  T (e, …)                                          -- NeedAction
ask all T [ for x in e { if e | let y = e } (e, …) ]    -- NeedAll, homogeneous
ask all ( req, … )                                     -- NeedAll, heterogeneous
bytes of e                                             -- NeedBlob
```

`ask` and `run` are two keywords because the kernel has two rule tables and one interface is legitimately
registered in both. the effect category is not part of an interface and must not be declared as if it were;
the keyword is 0019's effect visibility, read off a fact the kernel already carries rather than a category
axis the surface invents. `bytes of` carries no type because its interface is monomorphic.

the head type may be elided when a `let`'s annotation supplies it, by one purely syntactic pass over the
tree before name resolution and before any type exists. a head-typeless request surviving that pass is a
parse error with a mechanical fix. this is what keeps `parse(bytes)` a pure function of the bytes, which is
the precondition every architecture in the survey rests on.

a request appears only in checking position — as the whole right-hand side of a `let`, or in a rule's
tail position. adopt this because the IR wants it, not as the dispatch's price: every suspension is then a
statement, so 0038's control point is a statement ordinal assigned in elaboration order, the environment is
the live binder set at that boundary, and the span side table is keyed by `(body digest, control point)`
with no new identity. A-normal form at the surface is what makes defunctionalization mechanical.

`ask all (…)` binds a parenthesized binder group in statement position only. it is not an expression, no
tuple type is constructed and no tuple value exists, so 0026's refusal of positional tuples holds — `NeedAll`
already takes a request vector and `Resumption::Many` already is a binder vector. this is the one place a
request nests inside another construct, and the reason is principled: the construct whose meaning is "these
are independent" is the one that may contain several. the kernel limit is that `NeedAll` carries
`Request<Pure>` only, so a heterogeneous `ask all` cannot contain a `run`. widening that is a kernel change
and this proposal does not take it.

declared independence is the comprehension's meaning, because an element cannot reference another
iteration's result; sequential dependence is `fold` with an explicit accumulator. so the shape of the source
says whether the work is parallel, which answers 0038's open `NeedAll`-versus-sequence question.

the notation ranking is worth stating plainly, because the honest form loses on reading and wins on
everything else. a declared per-question name — `xylem.compile(tc, src, [])` — is the shortest thing any
design offers, and it was the close loser. it fails on three counts: it adds a per-module namespace over the
`Interface` value that is not authoritative by its own admission, since any module may mint a second name
over any bucket, with three diagnostic codes and a warning that exist only to police it; it puts the effect
category on the interface, which the kernel contradicts by registering one interface in two tables, forcing
eighteen interfaces into twenty-two declarations plus a redundant call-site keyword; and a third party
serving an interface xylem never named must mint a local name, so U-10's replacement test passes with a
*different call-site spelling*, which is the hidden hook U-10 is written against.

the resolution is that pith's honest form is short enough to render everywhere it is not written.
`Display for Interface` already produces one readable line, so write the short form and read the interface
literal in inlay hints, hover, signature help and both selection diagnostics.

## rules and declarations

```
pure rule compile-entry(tc: Toolchain, src: CSource, provided: Provided) -> Object = { … }
action rule compile(…) -> Object = requires { process } { plan { … } complete(x) { … } }
pure rule resolve(…) -> Resolution = host
```

the interface is *derived* from the signature, written once, and cannot drift; definition and use spell the
same interface in the same order. `requires` goes on the rule, not on the interface: capabilities live
on `ActionSpec`, which the body plans after selection, so an interface-level clause would be a claim about a
callee the elaborator does not know and would put one fact in two modules. and state 0003's limit plainly,
because every notation inherits it identically — capability *propagation* is not statically expressible
under 0015, since which rule answers a request is decided by the registry at run time. the closure is a
provenance query over the durable capability tables. that is weaker than 0003's wording and the record
should say so.

the three declaration forms 0047 fixed and no fourth: `nominal X = R`, `sum X = | c(payload) | …`,
`type X = <structural>`. recursion is the declaration's own name inside its body, elaborating to `Type::Cut`.
the alias form is the most used shape in the corpus — twenty-six structural types that are not table entries
— and it deletes every `fn …_type() -> Type` currently rebuilt from scratch on each call inside `is_type`.

every declaration is public. there is no export list and no privacy sigil. a peer re-declares any
structural signature in one line and reaches the same bucket, so privacy would be a lie the language told
about a boundary it cannot hold, and an unenforced internal marker is the typeshed failure. the cost is that
a module's declaration set is its whole API.

## metadata splits on whether anything reads it

```
about {
  description: "a lightweight command-line JSON processor",
  homepage:    "https://jqlang.org",
  maintainers: ["karol"],
}

let subject : phloem.Subject = {
  license:   phloem.spdx "MIT",
  platforms: gnu.unix,
}
```

nixpkgs conflates these into one `meta` attribute and then has to special-case three of its fields:
`meta.license` is read by the unfree predicate, `meta.broken` by the broken predicate, and `meta.platforms`
by availability, so "documentation" turns out to be semantic at evaluation while staying inert to the
derivation hash. pith already has the rule that separates them, because 0047 requires a doc-comment change
not to move a declaration digest and the ABI digest above excludes doc text: anything a policy reads is a
declared value inside a digest; anything only a human reads is doc text outside every digest.

so an `about` block rides in the position-carrying sidecar and reaches no key, and editing a description
invalidates nothing. `license` and `platforms` are ordinary fields of a declared value, because something
reads them — license is a policy question and `bark` is already `name.md`'s reserved name for the policy
library, and platforms connect to `PlatformRequirement::Exact`, which exists on `ActionSpec` today. and
`spdx "MIT"` is a nominal with a validating pure rule rather than a declared sum over the whole SPDX list,
because 0026 forbids refinement types and puts predicates in rules.

## entry points, so the CLI is not a table of hardcoded verbs

```
entry dev   : phloem.Exec      = ask (environment, lock)
entry check : xylem.TestReport = ask (toolchain, program)
```

an entry is a name bound to a request, and `pith run <entry>` computes it. the CLI's verbs stay generic, so
no subcommand is added per domain and no path is hardcoded anywhere — the toolchain paths, the environment
and the program to exec are all derived from the graph, which is what `Toolchain::discover` and 0030's
declared closure already produce. this also replaces `pith eval <label>`, which is the vestigial form of the
same idea: a name selecting a request, against an empty rule set, with nothing behind the name.

a name is safe here for one specific reason: `label` is decorative by design with a test asserting it
never participates in selection, so an entry name selects *which request to build* and never *which rule
answers it*. 0015 is untouched.

one mechanism keeps this from being a task runner. `scope.md` says pith is not an ordered task
runner and [milestones](milestones.md) records that the record settling that contradiction does not exist. so
an entry denotes a value and the caller performs the effect, which is 0041's rule for the lock — "the
written lock is a text projection whose write is a caller effect" — applied to execution. `Exec` is one
nominal any domain can produce, so there is no first-party verb table and 0002 holds, because the entry
declares what should be the case rather than a sequence of steps. when someone wants three things in order
they get no construct: they write a rule producing a single `Exec` whose program is a script the graph built,
which 0036 already covers as a produced program being content. sequencing becomes content, not syntax,
and that is what keeps `scope.md` from needing an amendment.

two constraints belong in the record. an entry is invocable only from the root module, never from a
dependency — an entry that execs is a supply-chain surface, and a dependency that could contribute one is
npm's `postinstall`, so this is a resolution-level refusal rather than a convention. and an `Exec` entry
carries `requires`, checked against what the value declares, subject to the same limit as every other rule:
propagation is a provenance query, not an annotation.

worth naming because it is the first honest end-to-end test: `pith run dev` on a cold machine has to
resolve, fetch, build, substitute and exec, which is a vertical slice through M-3, M-4 and the module system
in one command — and it is the first thing that needs the timeout and resource limits that exist nowhere in
the tree.

names are `ident | string` everywhere a name appears. this is forced by the corpus, not chosen:
`"expected-owner"` is a live field name of a structural record inside two stele interfaces, and
`Type::record` validates only duplicates, so the kernel admits names no identifier grammar can spell. a
surface that cannot spell one cannot express a peer's declarations, which is 0004 at the lexical level. the
quoted form is primary and the identifier form its abbreviation, and the two elaborate to identical bytes.
this keeps `-` free as an operator with no import-dependent lexing, and it pays a second dividend: the
corpus's rule labels are hyphenated, so those labels survive migration byte for byte and every
`RuleIdentity` over them is preserved.

local definitions are module-private, annotated, earlier-in-file-only, non-recursive, and elaborate to a
first-order call node — not inlined. inlining admits exponential elaborated bodies for a chain of doubling
definitions. privacy is forced by the ABI cutoff: an exported helper whose body changed would move a
downstream rule's body digest, which is the rust-SVH behavior the cutoff exists to avoid. the cost is real
and must not be dressed up — there is no cross-module helper below rule granularity, so a shared helper is
either a builtin or a rule, and "duplicate the text" is not one of the options, because a language whose
own record set forbids parallel tables that drift cannot sanction copy-paste as a composition mechanism.
that gap is named, not resolved.

iteration is comprehension, `fold`, and the catamorphism generated at a sum's cut positions. there is no
termination checker, no pragma and no fuel parameter, because with no surface in which a non-structural
recursive call can be written there is nothing to check and nothing to escape — and by 0047's
no-reader-no-constructor discipline, 0018's reserved escape hatch should not be built until a consumer
appears. a rule may not request its own interface. that is a one-line elaboration-time refusal and it
costs the corpus nothing today. the alternative reading — that 0018 names the graph as a sanctioned
repetition mechanism, so self-requests are legal and 0050's cycle detection is the backstop — does not
survive 0050's own unresolved section: "a predicate refuses a request that repeats; it does not bound a body
that yields an unbounded sequence of distinct requests." legalizing self-requests without the backstop
breaks totality by construction and K-3 with it. the refusal is cheap; the backstop limit owed to four
callers is still owed, and indirect cycles through two modules remain bounded only by 0050's runtime
predicate.

`match` is dhall's handler record, so exhaustiveness is a closed-record mismatch under `is_type` — no
checker, no diagnostic class, no wildcard, no guards, no or-patterns, and adding a constructor breaks every
incomplete match by construction.

six operators: `== != < + - *`. equality is one structural walk on any type, which 0052's agree behavior
already needs; `<` covers `Int` and `Text`, which the corpus's thirteen hand-rolled canonical sorts need.
no division and no modulo, because division is partial and 0047's ground for arbitrary-precision `Int` is
that closure under addition and multiplication makes arithmetic total. a domain needing integer division
asks by record.

builtins are unqualified names in a closed set, and shadowing one is refused. the corpus fixes the floor at
about seventeen — text splitting, trimming, prefix and containment tests, replacement, list concatenation
and flattening, canonical sorting and set construction, UTF-8 decoding, relative-path and component checks,
and a compile-time `module` constant so a merge contribution names its owner by derivation rather than by a
string literal repeated at every site. two holes are named rather than hidden: xylem's depfile prefix strip
is a repeated-strip loop needing either a builtin or a bounded fold, and its parse composes set dedup with a
drop. the total text library is a floor, not a set — the corpus has about forty formatting sites, three
whole file formats and a shell script with quoting, none of it enumerated. that is the largest unfinished
edge and it deserves its own record, gated on the corpus rather than on taste.

merge gets no syntax, per 0052. a merge is a request whose output type is the merged type — stele already
registers it that way — the policy is an *input position* so omitting it fails to select rather than silently
defaulting, and removal is a named replacement with an expected-owner check rather than helm's null
sentinel. because `ask` is a visible keyword, nickel's invisible-site problem cannot arise, and with it the
priority ladder nickel acquired because an infix operator has nowhere to put policy.

interface inputs stay positional and unnamed: names outside the digest are a drift class, and names inside it
move every existing rule's revision. the seven positions of stele's system-composition interface go to
signature help, and the record should say the honest thing — a wide positional interface is a signal to take
a declared record instead, which the closed-record calculus supports and which puts the field names inside
the digest where they cannot drift.

## diagnostics

the frontend takes a new stable-code range at 3000, leaving the allocator question open beyond it. the 1000
block is the engine's and tops out at `E-1105`; the 2000 block is reserved for composition and unused; and
`9001`–`9006` were squatted first-come by four domains, each with a comment saying it read the others,
because 0053 declined to build an allocator on the ground that peerhood means the list cannot be closed and
a first-come registry "is a coordination mechanism the project has no other need for". it has one now.

three messages carry the design. the no-match message partitions candidates into
same-output-different-inputs, same-inputs-different-output ("you asked for `Object`; from these arguments you
can get `Depfile`"), and nothing-with-that-output. the representation hazard gets a dedicated rendering,
printing the representation and both declaration digests whenever two coordinates print identically. the
ambiguity message keeps `E-1102`'s candidate list and gains the teaching note 0057 earns — distinguishing
two rules over one shape costs a nominal type, which is visible three times in the corpus and is not
guessable — plus a code action that narrows the ascription at a known span.
