---
schema: design-doc/v1
id: research-passing-behaviour
title: passing behaviour
summary: how Unison, Dhall, Cloud Haskell, Spark, Bazel, Salsa and Adapton identify and serialize functions, what type-based dispatch cost Haskell, Rust, Scala and Bazel, how OCaml functors and Nix's callPackage and overlays parameterize code, and what is known about effect handlers and memoization
kind: research
status: researching
evidence: preliminary
created: 2026-09-23
updated: 2026-09-23
tags:
  - research
  - language
  - extensibility
relations:
  informed_by:
    - research-dispatch
    - research-extension-interfaces
  depends_on:
    - research-method
  supersedes: []
---

# passing behaviour

[0082](../decisions/0082-rules-are-called-by-name-and-functions-are-values.md) removes selection by type
and makes functions values: a rule's identity plus arguments already bound, with a canonical encoding that
can be part of a cache key. this note reads the systems that identify functions as data, the systems that
select implementations by type, and the systems that parameterize code explicitly. sources were read on
2026-09-23.

## identifying functions as data

Unison, Dhall and Cloud Haskell give functions an identity that can be stored, sent or hashed. Spark,
Bazel, Salsa and Adapton show what happens when what a closure captures is implicit.

### Unison

Unison names every definition by "a 512-bit SHA3 digest of a term or a type's internal structure,
excluding all names"; arguments become positional references and dependencies become their hashes.
mutually recursive definitions hash as a cycle ordered by the members' own hashes. "The hash of a function
incorporates the hashes of all of its dependencies and will change if any of its dependencies change."

every value is serializable, and a serialized closure references code by hash; the receiver fetches
hashes it lacks. `Value.memo` consults "a cache, keyed by the hash of `v`", with the caveat that
"controlling the memoization of computations whose determinism is unknown requires some additional
thinking."

the issue tracker records three costs. #3280 (2022-07-29): equality gave false negatives because
"constructor tags ... are dependent on the order in which types are loaded into the code cache", a
runtime detail inside the canonical form. #3449 (2022-09-23, open): serialization does not share repeated
references. #3604 (2022-11-14): full hashes in serialized values made them large, and a compact form was
added.

types with the same shape have the same hash, so `Optional` and
`Maybe` were the same type. Unison added `unique type` and later made it the default, since "most types
that appear in application code are not intended to be treated as the same type."

### Dhall

Dhall pins imports by a hash of the normalized expression: beta-normalized, then alpha-normalized, then
encoded and hashed. since 2017 (dhall-haskell PR #167) the hash "reflects changes to the expression's
semantic value", so moving `double = \(x : Natural) -> x * 2` into its own file keeps it. this works because
Dhall has no recursion and every expression has a normal form. an implementation that ran the two
normalizations in the wrong order broke real hashes (#553). normal forms give
identity by structure, not by behaviour: `x * 2` and `x + x` hash differently. the standard version was an
input to the hash until v6.0.0, so every release invalidated every hash.

### Cloud Haskell, defunctionalization and distributed-closure

Reynolds' defunctionalization (1972) replaces each function with a constructor holding its free variables
and a case that applies it; Danvy and Nielsen (2001) describe it as "a whole-program transformation".

Cloud Haskell (Epstein, Black, Peyton Jones, 2011) explains why a closure cannot simply be serialized:
"serializability of a function is not a structural property of the function, because Haskell's view of a
function is purely extensional." it adds that "it is crucial that some types are not serializable", such as
ports, and that serialization affects cost and "should not be invisible". it uses `Static`, a closed code
address, combined with an explicit environment. GHC's `StaticPointers` requires the body of `static`
to be closed. `distributed-closure` builds on it: a closure is a static pointer plus serializable bound
values.

### accidental capture

Spark serializes each closure to every executor; passing a method "requires sending the object that
contains that class". its ClosureCleaner prunes what closures capture through `$outer`, and PR
#5685 calls it "one of the least understood" files.

Bazel requires `Args.add_all(map_each=...)` to be a top-level function "to avoid unintended retention of
large analysis-phase data structures into the execution phase". issue #12701 (2020-12-15) is titled
"closures are a great way to smuggle state".

Salsa requires tracked-function arguments to be interned or implement `Eq + Hash`, which Rust closures do
not. Adapton's authors note that in OCaml "we cannot ... examine a thunk's 'arguments' (that is, the values
of the variables in a closure's environment)", and tie memoized functions to explicit names.

## selecting implementations by type

Haskell only guarantees that the instances one module sees are coherent; Edward Yang (2014-07-11) shows
two modules with different `Ord` instances producing a `Set` with a duplicate, and calls global uniqueness
"inherently nonmodular". Rust enforces coherence with the orphan rule, and RFC 1023 (2015) describes
defining impls as "a zero-sum game" between crates. Scala 3's reference says implicits "are easily
over-used and mis-used", that failed searches "give very unspecific error messages", and splits the feature
into `given`, `using` marked at the call site, and explicit conversions. Bazel's toolchain resolution picks
"the first available toolchain ... compatible with this execution platform and the target platform" over a
six-level priority order, and its maintainers describe the debug output as "overly verbose, while still not
giving details" (#17814, 2023-03-17).

## parameterizing explicitly

OCaml functors take modules as parameters. modular implicits (2014) are elaborated into explicit functor
application, and modular explicits (2024) are described as the language implicits "should be elaborated"
into; they "do not strictly increase expressiveness". Gabriella Gonzalez's "Scrap your type classes" (2012)
argues type class programming can be done "purely at the value level", at the cost of verbosity.

Nix's `callPackage` fills a function's arguments by name from the package set; it evaluates a file several
times with different arguments for cross compilation. overlays compose `final: prev:` functions into a
fixpoint. nixpkgs #99100 (2020-09-29) lists the problems: several override functions "and it's hard to know
which one is right", knowing "when to use `self` and `super` is very tricky", and "It's just a big mess!"

## effect handlers and memoization

no paper found treats whether a handler in scope becomes part of a memo key. handlers are dynamically
scoped in Eff, OCaml 5 and Unison. "Build Systems a la Carte" (Mokhov, Mitchell, Peyton Jones, 2018) gives a
task a `fetch` callback supplied by the build system and keys caches on the task's key alone, so the
build system is the only handler.

## where they differ

systems that select implementations by type all acquired coherence rules or explicitness patches, and in
each the complaints are about what was selected and why. systems that pass behaviour explicitly give
up inference and brevity. OCaml and Scala both moved toward an explicit core with optional sugar on top.

## result for this project

pith adopts function values as data, with the invariant and mechanism of Cloud Haskell's `Static` plus
environment and of Unison's closures that reference code by hash. the decision is
[0082](../decisions/0082-rules-are-called-by-name-and-functions-are-values.md), and the research added five
rules to it as an edit. a function value pins the revision of the rule it refers to, as Unison's hashes pin
dependencies, so a stored value keeps its meaning. a named rule is identified by its coordinate and its body
digest, and a lambda by its body digest over the de Bruijn form 0062 already uses, so lambdas that differ
only in variable names are one rule. captured values are visible in the value and in `pith explain`, and a
value above a size limit is captured by content identity. the encoding shares repeated structure and
contains nothing that depends on load order. values that must not be captured, such as host handles, have
types a function value cannot hold.

equality of function values is structural. following Dhall's example, pith does not claim that `x * 2`
and `x + x` are the same function.

explicit passing replaces selection by type. it costs verbosity, and a structure can be used with a
function it was not built with; storing the function in the structure, as the `Source` record does,
prevents that.

overrides stay the single mechanism of 0080: written by the consumer and not recursive. Nix's `callPackage`
and overlays show the three things this avoids, which are filling arguments by name from a shared set, an
open fixpoint over the whole package set, and several kinds of override.

the engine remains the only handler across cached calls, as the build system is in "Build Systems a la
Carte".

two uses of type classes are not covered by passing values: dispatch on a return type, where there is no
argument to dispatch on, and automatic derivation of instances for composite types. in pith both become
explicit arguments or combinators.

## sources

- Unison: https://www.unison-lang.org/docs/language-reference/hashes/, https://www.unison-lang.org/docs/the-big-idea/,
  https://www.unison-lang.org/articles/distributed-datasets/incremental-evaluation/,
  https://www.unison-lang.org/docs/fundamentals/data-types/unique-and-structural-types/; issues #1728,
  #3077, #3280, #3449, #3604, #4539: https://github.com/unisonweb/unison
- Dhall: https://github.com/dhall-lang/dhall-lang/blob/master/standard/imports.md; dhall-haskell PR #167 and
  issue #553; Gonzalez, 2017: https://www.haskellforall.com/2017/11/semantic-integrity-checks-are-next.html;
  https://docs.dhall-lang.org/discussions/Safety-guarantees.html
- Danvy and Nielsen, Defunctionalization at Work, 2001: https://www.brics.dk/RS/01/23/BRICS-RS-01-23.pdf
- Epstein, Black, Peyton Jones, Towards Haskell in the Cloud, 2011:
  https://www.microsoft.com/en-us/research/publication/towards-haskell-cloud/; GHC static pointers:
  https://downloads.haskell.org/ghc/latest/docs/users_guide/exts/static_pointers.html;
  https://hackage.haskell.org/package/distributed-closure
- Spark: https://spark.apache.org/docs/latest/rdd-programming-guide.html and PR #5685
- Bazel Args: https://bazel.build/rules/lib/builtins/Args; issue #12701; toolchains:
  https://bazel.build/extending/toolchains; issue #17814
- Salsa: https://salsa-rs.github.io/salsa/overview.html; Hammer et al., Incremental Computation with Names,
  2015: https://arxiv.org/abs/1503.07792
- Yang, 2014: http://blog.ezyang.com/2014/07/type-classes-confluence-coherence-global-uniqueness/;
  Rust RFC 1023: https://rust-lang.github.io/rfcs/1023-rebalancing-coherence.html;
  Scala 3 contextual abstractions: https://docs.scala-lang.org/scala3/reference/contextual/index.html
- OCaml generative functors: https://ocaml.org/manual/5.2/generativefunctors.html; modular implicits:
  https://arxiv.org/abs/1512.01895; modular explicits, 2024:
  https://gallium.inria.fr/~remy/ocamod/modular-explicits.pdf
- Gonzalez, Scrap your type classes, 2012: https://www.haskellforall.com/2012/05/scrap-your-type-classes.html
- Nix overlays: https://nixos.org/manual/nixpkgs/stable/#chap-overlays; nixpkgs #99100;
  https://jade.fyi/blog/flakes-arent-real/
- Mokhov, Mitchell, Peyton Jones, Build Systems a la Carte, 2018: https://dl.acm.org/doi/10.1145/3236774
