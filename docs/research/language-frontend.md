---
schema: design-doc/v1
id: research-language-frontend
title: language frontends, interface artifacts, and module resolution
summary: what shipping systems do about exporting a module's types for tooling, resolving imports statically, identifying and versioning modules, structuring an incremental frontend, binding native definitions to surface declarations, and reading return-type-directed dispatch
kind: research
status: researching
evidence: preliminary
created: 2026-08-16
updated: 2026-08-16
tags:
  - research
  - language
  - tooling
  - modules
relations:
  informed_by:
    - research-tooling
    - research-declarations
    - research-configuration
    - research-dispatch
    - research-dependency-resolution
    - research-index-formats
  depends_on:
    - research-method
  supersedes: []
---

# language frontends, interface artifacts, and module resolution

this note collects the prior art for a pith language frontend. it is organized by the question each
finding answers rather than by system, because the systems answer different questions and surveying
them one at a time hides the convergences, which are the useful part.

the framing question is narrower than "how do languages work". pith has a built kernel with a
declaration table (0047), exact interface dispatch (0015, 0057), canonical value and type encodings, a
content store, and a span-carrying diagnostic type — and no surface syntax, no parser, and no
serialized form of a declaration at all. what a frontend has to do first is publish a surface that
today exists only as a `OnceLock<DeclarationTable>` inside four rust crates. so the questions below are
about boundaries and artifacts before they are about notation.

## what gets exported so a tool can read another module's types

twelve systems were read on this. the convergence is the strongest in the whole note: every batch
compiler that achieved cross-module parallelism did it by making a canonicalized interface the unit
that crosses a module boundary, and by making that interface's digest the downstream cache key.

.NET states the mechanism outright: a reference assembly "only changes when its public API is
affected", so using it instead of the implementation assembly "allows skipping the build of the
dependent project in some cases". bazel's `ijar` and `turbine` derive the same artifact from source with
a cheaper compiler than javac, and that is what makes java builds parallel. gradle enumerates the
ABI-compatible changes it ignores: method body, comment, private member, parameter rename. GHC computes
per-declaration MD5 fingerprints and stops early when all of them match. ocaml exposes the choice as a
flag, `-opaque`.

rust is the outlier and the ranking is itself the finding. `.rmeta` is, in one contributor's words,
"effectively unstable partial compilations that only the exact same compiler can interpret", the SVH
hashes HIR so a private body edit invalidates downstream, and the consequence is that
"rust-analyzer does not support reading rlibs. It needs the crate source". rust's ecosystem answer to
its own interface format was to write a second frontend. if the only way to learn what xylem declares
is to link xylem and call `table()`, pith is in rust's position exactly.

three further mechanisms matter.

references travel as coordinate plus digest, not as inlined bodies. GHC overrides `putName` to
serialize a name's hash rather than the name, so a declaration's fingerprint moves when anything in its
transitive closure moves. go measured the alternative: deep export data gave kubernetes packages "over
1MB of 'deep' export data, even when they have almost no content of their own", around 300x the source
size, which forced the shallow format where the closure is fetched lazily. 0047 chose deep embedding
deliberately, and rejected ambient-table resolution as its alternative — coordinate-plus-digest is a
third position that record did not weigh, and it is where both GHC and post-2023 gopls ended up.

the interface artifact is not the tooling artifact. GHC ships `.hie` behind `-fwrite-ide-info`
because `.hi` holds the exported interface and not a position-to-type map. ocaml ships `.cmt`/`.cmti`,
carrying "precise location information for every token" and explicitly "partial if type checking was
unsuccessful". gopls persists xrefs, methodsets and typerefs beside its type information. since 0038
strips spans, labels and comments from the IR, and those are exactly what an editor needs, a frontend
needs a second position-carrying artifact that participates in no digest.

interface files are generated, never hand-written by a consumer. `.d.ts` has no reconciliation
mechanism and a wrong one is silently unsound. `.pyi` has stubtest, which introspects at runtime and
admits it "cannot tell if a return type of a function is accurately typed". `.hs-boot` and `.mli` are
checked because both sides are the same language. pith's position is better than any of these: both
sides of a host binding reduce to one `Interface` value with one canonical encoding, so drift is an
exact digest comparison rather than an approximation.

two smaller findings worth keeping. swift distributes the textual `.swiftinterface` because the binary
`.swiftmodule` is "tied to the current version of the compiler", and carries its format version,
compiler version and module flags in-band; its cache validity is mtime-and-size for local builds and a
content hash for the SDK, memoized through "forwarding modules" — a layer pith does not need, because a
`ContentId` is already the strong form. and typescript 5.5 shipped `--isolatedDeclarations` because
declaration emit "requires a whole lot of logic to figure out the types" and therefore a cross-file
typechecker, which is why project builds cannot parallelize; the fix was a language restriction,
requiring explicit annotations on exported declarations. typescript took twelve years to reach that.

## what makes an import statically resolvable

two independent questions hide inside "can a language server resolve `import xylem`". the first is locating:
which bytes does this name? that holds when the target is a literal, in a grammatically fixed position, and
every symbolic name is closed by a resolution artifact on disk before evaluation. the second is knowing the
contents, and that holds when the module has a declared interface.

| system | locating | contents |
|---|---|---|
| Pkl | yes, literal URI plus `PklProject.deps.json` | yes, and an import clause *is* a type |
| CUE | yes, literal path in a fixed prefix position | yes, exportedness is syntactic |
| Starlark and Bazel | yes, literal label plus repo mapping | names yes, types no |
| Nickel | yes, literal or identifier plus a package map | partial |
| Dhall | yes, but in expression position | no separate interface |
| Nix | no — `import` is an ordinary function of a path | no |

Pkl's sentence is the crux: "unlike import clauses, import expressions only import a value, and do not
import a type." Pkl ships two import forms with deliberately different power, and globbed clauses also
cannot be used as types. that is the shape a pith import clause should take.

the sharpest datum is negative. `nil`'s feature list ships "source of flake inputs" checked while
"cross-file analysis" and "real flake outputs from evaluation" are unchecked — the one part of Nix a
static tool resolves is the one part Nix made a fixed-shape declaration. `nixd` links C++ nix and has
the user hand-write an evaluation entry point into editor settings.

seven mechanisms recur in every system that works: the import target is a literal; library names are
indirected through a per-module local binding so the import site never names a version; a module can
only find its direct dependencies; the registry index carries the manifest rather than the code, which
is 0046 already; integrity is separate from version selection; a name-to-location map is materialized on
disk for tooling — `bazel mod dump_repo_mapping` exists explicitly "when interacting with Bazel from an
external tool such as an IDE or language server", and Nickel's server errors `NoPackageMap` without one;
and a reverse edge records failed resolutions so a dependent re-elaborates when an import lands. no
language server in the survey resolves versions.

WORKSPACE's failure is the structural lesson. `load` must be top level, therefore a macro cannot load,
therefore a program cannot compute its own dependency set, so bazel needed bzlmod as a separate
declarative layer. statically resolvable imports and self-computed dependency sets do not come from one
mechanism; they come from two layers.

## how modules are identified and versioned

the survey's most useful result is a deletion. go's minimal version selection escapes NP-hardness by
restricting its input — Cox proved version selection NP-complete by reduction from 3-SAT — and the
enforcement mechanism that makes MVS safe is semantic import versioning, the `/v2` in the path. bazel
copied it as a numeric `compatibility_level` and then killed it: "People hate it when compatibility
levels aren't bumped … People _also_ hate it when compatibility levels _are_ bumped: it causes a
cascading 'sweep' of the entire ecosystem." both settings are no-ops from 8.6.0 and 9.1.0 and the
replacement is prose. so adopting MVS inherits an enforcement problem a build system tried and
abandoned.

elm is the one system that computes compatibility instead of asserting it. `elm diff` compares two
published documentation values and classifies major, minor or patch, and `elm bump` sets the version from
that computation. pith is in a strictly better position than elm, because a module's public surface is
already a value with a canonical encoding: added declaration is compatible, and a removed declaration, a
moved nominal representation or a changed interface is not. that turns the problem bazel abandoned from
a social one across an ecosystem into a differ in one place.

does content addressing remove the need for versions? no, for two reasons. 0039's own timing argument
one level down: constraints and locks speak about packages before content exists, and a digest is
unspeakable then. and a sharper one: for *nominal* types, "the diamond dissolves" means "the diamond
becomes a type error between two identically-printed types". two module versions declaring
`xylem.Object` are two declarations at one coordinate with two digests, and a value from one fails
`is_type` against the other. cargo has the verified analogue: the types "are considered different by the
Rust compiler, even if they have the same name". unison presents coexistence as deduplication because
unison has no nominal types to collapse.

everyone converged on the same three layers: the source file carries a name; a project manifest maps
name to coordinate and acceptable versions; a witness maps coordinate to content digest. go's import
path, `go.mod`, `go.sum`, where `go.sum` "may contain hashes for multiple versions of each module", so
it is a witness set that merges by union rather than a pin. cargo's `use`, `Cargo.toml`, `Cargo.lock`.
deno after its retreat from URL imports: bare specifier, import map, `deno.lock`. deno's published
reasons for retreating are worth recording because they are the case against putting a locator in source
text: long URLs, "URLs lack semantic versioning … so projects wind up with several variants of the same
library", and the reliability of "random websites or personal servers".

the property to design for follows: import-name resolution never consults a solver's output, so a
broken lock or an unfetched dependency degrades hover and completion and never breaks go-to-definition
on a name.

bazel's lock is subtler than a version pin and more useful: it records input hashes plus impure
extension outputs, "since the resolution algorithm is fully deterministic when given the same inputs and
all remote inputs are hashed", plus the two things a hash cannot cover: negative registry lookups and
yanked-version status. that is a witness, not a pin, and it is the same question 0041 leaves implicit.

## how incremental frontends and language servers are structured

the question that matters for pith is whether its frontend belongs inside its own incremental engine,
since pith already is one. four systems answer, all pointing the same way.

bazel: yes, and it built two escape hatches. `.bzl` loading is a SkyFunction, but in the default
configuration there is no Skyframe node per parsed `.bzl`, and the code comment explaining why is the
most valuable quotation in the survey: `BzlCompileValue` "doesn't have an interesting equality relation,
so we have no hope of getting any interesting change-pruning … If we had an interesting equality relation
that was e.g. able to ignore benign whitespace, then there would be a hypothetical benefit … BzlCompileValue
contains syntax trees, and that business object is really just a temporary thing for bzl execution.
Retaining it forever is pure waste." the equality relation bazel says it lacks is precisely 0038's
elaborated, span-free, alpha-normalized, canonically encoded digest. note that it argues for an
*elaborate* node, not a *parse* node. the second escape hatch, inlining `BzlLoadFunction` off Skyframe,
cost them a hand-rolled two-level cache, hand-rolled cycle detection — "we don't have the benefit of
Skyframe's internal cycle detection", and a deadlock hazard.

buck2: yes, and it gets zero early cutoff. both frontend DICE keys declare
`EqualityBehavior::AlwaysUnequal`, with the comment that "practically it is too hard to make it work
correctly for every case", so any `.bzl` edit invalidates everything downstream. buck2 buys
deduplication, parallelism and cancellation from DICE, not incrementality.

rustc: yes, fully, and here is the bill. "Computing fingerprints is quite costly. It is the main
reason why incremental compilation can be slower than non-incremental compilation." "Span information is
very volatile." `eval_always` is an author-maintained honesty annotation. and the residual bug class is
the unstable-fingerprint ICE where cache and recomputation disagree, whose only recovery is a clean
build.

sorbet: no incremental engine at all, and one of the fastest language servers in production. "in
response to certain edits, Sorbet is forced to give up and retypecheck the codebase from scratch", at
around 100,000 lines per second per core, with `sorbet -e 1` under 30 ms. a workspace of a few hundred
modules can plausibly be re-elaborated from scratch inside a keystroke budget, and no incrementality
mechanism is needed for the interactive loop.

two premises worth correcting, because both are commonly assumed. rust-analyzer does not read
`.rmeta`; it runs `cargo metadata` and a sysroot scan and then analyzes dependency *source*, which is why
`rust-src` is required. and there is no salsa document arguing its separation from cargo: the separation
is structural, with cargo's knowledge lowered into salsa inputs by a non-salsa subsystem.

every mature system has a coarse path for stable outside code and a fine path for code being edited.
rust-analyzer keeps a resilient in-memory analyzer for unsaved half-typed text and invokes the
authoritative batch compiler on save. gopls distinguishes `DiskFile` from `Overlay` and uses "export
packages for packages outside the workspace" while producing "syntax packages for all packages inside
the workspace". GHC's `Usage` carries two dependency granularities chosen by trust boundary: package
modules contribute only an ABI hash, home modules contribute module hash plus export-list changes plus
per-entity usages.

roslyn's red-green split is the exact rule for syntax trees: the green tree is immutable, parent-less,
tracks width not absolute position, and an edit rebuilds about O(log n) nodes; the red tree is built
top-down on demand and thrown away on every edit, manufacturing parent pointers and absolute positions
from widths. absolute positions and parent pointers are derived, transient and never persisted.

rust-analyzer's guide also names a precondition that reads as a warning: "each file can be parsed in
isolation. Unlike, say, `C++`, an `include` can't change the meaning of the syntax." if a surface
admits anything import-dependent at the lexical level, `parse(file)` stops being a pure function of the
bytes and the whole architecture collapses.

finally, rust-analyzer's durability — the mechanism that makes whole-world-from-source viable
interactively, without which "any change to `src/lib.rs` necessitates checking all the queries related to
standard library (which adds up to about 300ms)" — is a property attached to a mutable input slot. a
content-addressed engine has no mutable input slot, so it has no analogue and cannot easily acquire one.

## how native definitions bind to surface declarations, and how migration goes

pith already has the two-argument bind site: `register_rule(rule, body)` is structurally
`CREATE FUNCTION … AS 'funcs','add_one' LANGUAGE C`. the question is which side moves into a source file,
who authors it, and what checks agreement.

drift is detected exactly when the checker can read both sides in a language it understands. GHC's
`.hs-boot` errors on mismatch because both sides are haskell. erlang compares inferred success typings in
both directions, `-Wunderspecs` and `-Woverspecs`, which is possible because the implementation is
readable bytecode. python's stubtest introspects at runtime and cannot see return types. postgresql has
a coarse load-time major-version gate, and its two-tier vocabulary in one sentence — `LANGUAGE internal`
for compiled-in, `LANGUAGE C` for dynamically loaded and required to declare its convention. and everyone
else checks nothing: lean's `@[extern]` says "ensuring consistency of both definitions is up to the
user"; rust's `unsafe extern` says "it is the responsibility of the author … If the signatures are not
correct, then it may result in undefined behavior"; ocaml's `external` and julia's `ccall` the same.
no surveyed system checks a cross-language declaration against its implementation.

lean's `@[extern]` inverts the obvious assumption. `Nat.add` carries a complete typechecked lean body
with the comment "The definition provided here is the logical model", and the native symbol is an
override. migration to the pure-surface tier is then deleting the override, with no identity change.
the honest cost is that a rule with two bodies has two things that can be wrong, and a never-executed
model rots exactly like a stub; lean escapes this because its kernel reduces the model.

the declaration file must be authored by the implementer, in the implementer's own tree, and be the
input to everything else. the sharpest contrast in the survey is typeshed against GHC's
`primops.txt.pp`. both are declaration files for native code. typeshed's are written by non-implementers
in a separate repository, version independently, admit an explicit `Incomplete` marker, and need a runtime
tool that still cannot see return types. `primops.txt.pp` sits beside the implementation and is the
*source* the typechecker tables, the GHCi wrappers and the documentation are all generated from, so there
is nothing to drift from. its brace-delimited documentation slot reaching both the manual and the editor
is worth copying literally.

GHC's wired-in versus known-key split is the mechanism a language server needs. a wired-in thing "is
fully known to the compiler, not read from an interface file"; a known-key thing gives the compiler a
resolvable name with a fixed unique seeded into the name cache, with the definition still arriving from
the interface file. two separate privileges, and the second is cheap.

the negative example is nix, twice. nix's `PrimOp` records parameter names, arity and optional prose
but no types, plus an `internal` flag that hides a builtin entirely, so `nil` shells out to
`nix __dump-language` at build time and, on its fallback path, heuristically classifies a builtin from
its name string, while `nixd` hardcodes twelve names and marks them as keywords. and the migration failure
mode: `derivation`, the single most important name in the language, moved from the native tier into the
surface language and fell out of the machine-readable catalogue both language servers consume, which
is why nixd hardcodes it. a name that migrates out of the host registry disappears from whatever
enumerates, unless the enumeration covers both tiers by construction.

the migration timeline, corrected. 0056 cites bazel's starlarkification as four years. the design doc
was approved 2020-05-16; bazel 8.0's debundling shipped 2024-12-09, four years and seven months later,
with an autoload flag on by default; 9.0 completed 2026-01-20 at five years eight months; the flag is not
removed until 10.0; and the release announcement admits "a small number of language-specific flags,
actions, and toolchain types remain in Bazel for now." the original plan — require a `load()` in every
BUILD file — was abandoned, because "it requires changes to literally every single BUILD file, which
is too much" and "would still have a version skew problem". the replacement rewrote the rules in starlark
and shipped them inside the binary, injecting them into the BUILD namespace by name. the lesson is not
that injection makes migration transparent; it is that injection decouples the rewrite from the
relocation, and the relocation is what breaks callers.

how long does a "temporary" native tier last? forever. GHC primops ("the primitives cannot be defined
in source Haskell"), nix builtins, python C extensions with typeshed as a permanent institution,
postgresql's two languages as stable features, lean's `@[extern]` as a permanent performance decision,
erlang NIFs, and erlang added a runtime function, `erlang:nif_error/1`, so "Dialyzer does not generate
false warnings" about a stub body it must not believe. a permanent native tier is not type-system
neutral; it grows its own construct. 0038's permanent host tier is the empirically normal choice, not a
compromise.

## how return-type-directed dispatch reads

pith selects on the whole interface, inputs and output, and two xylem rules differ only in output —
`(Toolchain, Executable) -> TestReport` and `(Toolchain, Executable) -> CSource`. so a request cannot be
spelled as a named function call. eight precedents were read and they converge on five points.

a name over a return-type-directed interface is normal and fine. haskell's `read` and `mempty`;
mercury's `append`, one predicate with two mode declarations where "each mode of a predicate or function
is called a _procedure_" and selection is by which positions are bound; bazel's provider bracket. the
problems come from candidate sets that are invisible or from resolution that is a search. pith has
neither: the registry is closed and 0057 made selection one bucket lookup.

giving two different questions one name is bad API design in every language that permits it. the
swift forum thread on exactly xylem's shape ends with respondents recommending different function names
and "noting that incompatible return-type-only overloads are generally poor API design", and
`@_disfavoredOverload` — precisely the ranking 0015 rejects — is documented as temporary and disowned by
its own docs.

the resolution failure must be reported as a candidate list, never as an inference failure. GHC
reports an ambiguous type variable because instance resolution has not run; rust reports "type annotations
needed"; swift says "ambiguous use of". pith's `E-1102` already lists every candidate with its interface,
because selection is a bucket lookup over a closed registry. pith's ambiguity diagnostic is strictly
better than haskell's and always will be.

an import must never change selection. this is the one hard constraint from the survey rather than a
preference. scala's own reference states the failure: "over-reliance on implicit imports … leads to
inscrutable type errors that go away with the right import incantation … it is hard to see what implicits
a program uses since implicits can hide anywhere in a long list of imports", and "the syntax of implicit
definitions is too minimal … conveys mechanism instead of intent." pith is immune by construction, since
the registry is global to a run and 0015 forbids order from mattering, but only if an import may bring
a type into scope while the interface it elaborates to remains the fully-qualified coordinate.

idris is the cautionary structural case: type-directed disambiguation in a language with inference needs a
search, a search needs a bound, and the bound is `%ambiguity_depth` with a default of 3, a number that
rots, which 0015 rejects by name. pith's equivalent search is depth 1. what to take from idris is the
interactive half, holes plus proof search, whose pith analogue is a scan of the interface index's keys for
buckets the in-scope bindings can fill.

gradle is 0015 already shipped in a build tool: "variant-aware selection by matching the attributes
specified by the consumer with those defined by the producer", ambiguity refused with "a list of all
compatible candidates … to help with debugging attribute matching failures", and guidance to "add this
attribute to the consumer's configuration to resolve the ambiguity". gradle differs in one place: it has
disambiguation rules and attribute precedence, the ranking 0015 refuses, and gradle's error messages are
a recurring complaint area. so the ergonomics live in the diagnostic quality, not in the notation.

the notation ranking is best-to-worst on reading and exactly reversed on honesty: a declared request name;
ascription at the binding; the type in head position; a provider bracket; a full interface literal;
mercury-style mode annotations. the resolution is not a compromise, because pith's honest form is short
enough to render everywhere it is not written — `Display for Interface` already produces one readable
line. write the short form, read the interface literal, and make the two inseparable in tooling.

## what a total surface feels like to write

dhall itemizes its own bill. "Prohibiting direct recursion is one of the core design decisions in
Dhall"; the workaround is Boehm-Berarducci encoding; the admitted costs are that "the performance of
Church-encoded values is slower than that of directly implemented data structures" and that "any single
pattern matching will be linear in the data size". and "folds are the baseline" taken literally is
`List/fold : ∀(a : Type) → List a → ∀(list : Type) → ∀(cons : a → list → list) → ∀(nil : list) → list`,
five arguments and two of them types, to sum a list. pith needs none of it: 0047's declaration table with
a recursion cut makes a recursive sum a finite canonical form, and rank-1 prenex generics with no
subtyping mean a monomorphic call site determines its own type arguments.

starlark is the mainstream restricted surface and its restriction is dynamic. "It is a dynamic error
for a function to call itself or another function value with the same declaration. This rule, combined
with the invariant that all loops are iterations over finite sequences, implies that Starlark programs are
not Turing-complete." so the industry's most-used restricted build language reaches
non-Turing-completeness with `for` over finite sequences plus comprehensions and no folds in the
surface at all. that is a direct answer to 0018's open question about which total constructs the
fragment needs: the population that writes build files has been served by comprehensions for a decade.
pith can claim strictly more, because 0038 makes it an elaborator rejection rather than a dynamic error,
and that is worth claiming explicitly because starlark is the comparison everyone will make.

the comprehension grammar is settled across CUE, starlark, python and pkl: three clause kinds, `for`,
`if` and `let`, nesting left to right. and the comprehension is the natural surface for a declared-
independent batch, because a comprehension body cannot refer to another iteration's result; if it needs
one, the author writes a fold, which is sequential. so the shape of the source tells the reader whether
the work is parallel.

exhaustiveness should be a record type, not a checker. dhall spells sum elimination as a record of
handlers and makes exhaustiveness a typing property: an expression "is well-typed if there is a
one-to-one correspondence between the fields of the handler record and the alternatives of the union",
and a handler without a matching alternative "is a type error". pith has closed records, no row
polymorphism, and declared sums whose constructor set lives in the declaration, so it can adopt this
exactly and get exhaustiveness for free — a missing arm is a missing field, an extra arm an extra field,
both ordinary record mismatches. three absences follow: no wildcard arm, because a `_` is a
width-subtyped handler record the calculus does not have and it would silently absorb a new constructor;
no guards, because nickel's guarded arms mean a covering match can still fail, so guards and
exhaustiveness are incompatible; and no or-patterns.

closed records force conditional presence into the composition layer. pkl's `when` generator and
CUE's and starlark's `if` clause express conditional presence of a *field*, which a closed record type
cannot have. and pkl's late binding is the feature to avoid: "object properties are late-bound", so a
value depends on a property a later amendment may change — which is the NixOS module fixpoint by another
name, where 0052's merge is a function of the *set* of contributions plus a policy, order-independent and
without a fixpoint.

a merge surface should be a call with a mandatory policy, not an infix operator, and the evidence is
nickel's. nickel merges with `&`, commutative and associative and idempotent — and because an infix
operator is invisible inside an expression, the only place left to put conflict policy is on the value as
an annotation, far from the site, which is how nickel acquired `default`, `priority n` and `force`, where
"the value with the highest priority simply erases the other". CUE has the same operator and avoided the
ladder only by refusing override entirely. 0052 requires policy declared at the merge site, and a site
you cannot see cannot carry policy. nickel also warns that two spellings of an annotation are identical
at runtime and behave differently under merge, which is the positional subtlety this project's prose calls
a hidden hook.

two cautionary tales for removal and replacement. kustomize: strategic-merge-patch policy lives out of
band in the resource's schema, and without it kustomize "replaces the whole array because it is missing a
mergeKey and patchStrategy"; the annotation was deprecated in v5.0.0. so default-shaped merge policy is
how you get silent whole-list replacement, and the policy must be un-omittable. helm: "if you need to
delete a key from the default values, you may override the value of the key to be `null`, in which case
Helm will remove the key" — deletion by sentinel, a value in the data changing the operation, so `Unit`
and absence must never mean delete. helm's second lesson is structural: it generates YAML by text
templating, which is why its own best-practices page legislates indentation and whitespace chomping. a
surface must give authors no way to produce a target format by string concatenation; a rendered artifact
is a value a rule serializes.

the escape-hatch finding. every checked-total system grows one of a pragma, a measure clause, or a
fuel parameter, all because the author writes the recursion and the checker judges it. turner's total
functional programming needs structural recursion plus a data/codata distinction, and a build kernel has
no infinite streams, which removes the half that is hard to teach. there is a third option: generate the
catamorphism from the declaration and offer no surface in which a non-structural recursive call can be
written. then there is nothing to check and nothing to escape.

## sources

- reference assemblies, .NET — <https://learn.microsoft.com/en-us/dotnet/standard/assembly/reference-assemblies>
- `ijar` and `turbine`, bazel — <https://github.com/bazelbuild/bazel/tree/master/third_party/ijar>
- interface files and recompilation avoidance, GHC — <https://gitlab.haskell.org/ghc/ghc/-/wikis/commentary/compiler/recompilation-avoidance>
- `.hie` files — <https://ghc.gitlab.haskell.org/ghc/doc/users_guide/separate_compilation.html>
- swift module interfaces — <https://github.com/swiftlang/swift/blob/main/lib/Frontend/ModuleInterfaceLoader.cpp>
- `--isolatedDeclarations`, typescript 5.5 — <https://devblogs.microsoft.com/typescript/announcing-typescript-5-5/>
- shallow export data, go — <https://go.dev/issue/58497>
- stubtest, typeshed — <https://mypy.readthedocs.io/en/stable/stubtest.html>
- rmeta as unstable partial compilation — <https://internals.rust-lang.org/t/rmeta-stability/17040>
- import expressions and clauses, Pkl — <https://pkl-lang.org/main/current/language-reference/index.html>
- CUE modules and registry — <https://cuelang.org/docs/reference/modules/>
- bzlmod and MODULE.bazel — <https://bazel.build/external/module>
- `compatibility_level` removal — <https://github.com/bazelbuild/bazel/issues/24302>
- starlarkification design — <https://bazel.build/rules/rules-tutorial>
- `nil` feature list — <https://github.com/oxalica/nil>
- minimal version selection — <https://research.swtch.com/vgo-mvs> and <https://go.dev/ref/mod>
- version selection is NP-complete — <https://research.swtch.com/version-sat>
- http imports retrospective, deno — <https://deno.com/blog/http-imports>
- `elm diff` and `elm bump` — <https://github.com/elm/compiler/blob/master/terminal/src/Diff.hs>
- `BzlLoadFunction` change-pruning comment — <https://github.com/bazelbuild/bazel/blob/master/src/main/java/com/google/devtools/build/lib/skyframe/BzlLoadFunction.java>
- DICE and starlark keys, buck2 — <https://buck2.build/docs/developers/architecture/buck2/>
- incremental compilation costs, rustc — <https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html>
- sorbet performance — <https://sorbet.org/blog/2019/07/09/enforcing-invariants>
- red-green trees, roslyn — <https://ericlippert.com/2012/06/08/red-green-trees/>
- rust-analyzer architecture and parse-in-isolation — <https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/architecture.md>
- salsa durability — <https://github.com/salsa-rs/salsa>
- `Overlay` and export packages, gopls — <https://github.com/golang/tools/blob/master/gopls/doc/design/implementation.md>
- primops, GHC — <https://gitlab.haskell.org/ghc/ghc/-/blob/master/compiler/GHC/Builtin/primops.txt.pp>
- `@[extern]` and the logical model, lean 4 — <https://lean-lang.org/lean4/doc/dev/ffi.html>
- `nif_error/1`, erlang — <https://www.erlang.org/doc/man/erlang.html>
- `derivation` missing from the builtin catalogue — <https://github.com/NixOS/nix/issues/7753>
- return-type overloading thread, swift — <https://forums.swift.org/t/overloading-on-return-type/56952>
- modes and procedures, mercury — <https://mercurylang.org/information/doc-latest/mercury_ref/>
- `%ambiguity_depth`, idris — <https://idris2.readthedocs.io/en/latest/>
- variant-aware selection and ambiguity errors, gradle — <https://docs.gradle.org/current/userguide/variant_model.html>
- prohibiting recursion, dhall — <https://docs.dhall-lang.org/howtos/How-to-translate-recursive-code-to-Dhall.html>
- union elimination typing, dhall — <https://github.com/dhall-lang/dhall-lang/blob/master/standard/type-inference.md>
- non-Turing-completeness, starlark — <https://github.com/bazelbuild/starlark/blob/master/spec.md>
- merge and priorities, nickel — <https://nickel-lang.org/user-manual/merging>
- patch strategy deprecation, kustomize — <https://github.com/kubernetes-sigs/kustomize/releases/tag/kustomize%2Fv5.0.0>
- deleting a default key, helm — <https://helm.sh/docs/chart_template_guide/values_files/>
