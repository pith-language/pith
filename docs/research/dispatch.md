---
schema: design-doc/v1
title: finding the implementation for a call
summary: how the JVM, CLOS, Julia, Prolog, GHC and make each find the code that answers a call, and why the five that rank candidates can only ever index approximately while the one that demands an exact signature looks the answer up
id: research-dispatch
kind: research
status: researching
evidence: reviewed
created: 2026-08-16
updated: 2026-08-16
tags:
  - research
  - rules
  - performance
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - research-method
  supersedes: []
---

# finding the implementation for a call

a call names something, and a system with more than one candidate implementation has to decide which one answers it. the systems that do this at scale have converged on the *question* — which of the registered definitions applies here — and split on the answer, in a way that decides what the lookup can cost. this note reads six of them at one question: is finding the implementation a lookup on a key the call computes, or a search over the population that has to consider candidates one at a time?

the reason to ask it as a cost question is that the cost question and the semantics question are the same one. a system whose selection rule ranks candidates by specificity has to *compare* candidates, and comparing candidates means having them; a system whose selection rule admits an exact match only can hash the call and stop. every mechanism below is downstream of that.

## the JVM: name and descriptor, and the match is exact

method resolution in the JVM specification is a lookup with no ranking in it. resolving a method reference against a class C succeeds when "C declares a method with the name and descriptor specified by the method reference," and otherwise recurses into the superclass and then the superinterfaces. the descriptor is the full argument and return signature, and it has to match exactly — the one exception the specification carves out is signature-polymorphic methods, for which "it is not necessary for C to declare a method with the descriptor specified by the method reference."

what makes this the cleanest pole is where the ranking went. java has overloading, and overloading is resolved by the *compiler*, which picks the descriptor and writes it into the constant pool. by the time the runtime sees a call there is nothing left to rank, and the vtable index the resolved reference turns into is the lookup. exactness at the boundary is what buys the constant-time dispatch, and the cost is paid earlier, once, by a compiler holding the whole program.

## CLOS: select the applicable, then sort them

the Common Lisp standard states the other pole as a procedure. determining the effective method is three steps: "Select the applicable methods," then "Sort the applicable methods by precedence order, putting the most specific method first," then apply method combination to the sorted list. the sorted list is load-bearing. standard method combination calls the before methods "in most-specific-first order" and the after methods in the reverse, so the order is part of the semantics and not a tie-break.

selecting and then sorting is a search by construction. the applicable set is a function of the arguments' classes, the precedence is a function of the class precedence lists, and neither is knowable without looking at the candidates. what a CLOS implementation can cache is the *outcome* — the effective method for a tuple of argument classes — and that is what production implementations do, but the thing being cached is a search that ran.

## Julia: most specific, and a tie is an error

Julia's manual gives the same selection rule for the same reason, and then makes the choice this note cares about at the tie. "the most specific method applicable to those arguments is applied," and when two methods are equally specific "Julia raises a `MethodError` rather than arbitrarily picking a method," reporting the conflicting candidates and suggesting a more specific definition. ambiguity is a refusal, not a resolution.

the implementation notes say plainly what indexing a ranked search looks like. methods live in a method table; a call forms a tuple type of its arguments and looks it up. the structure is not a hash of signatures but a search structure: "the method table and cache splits up on the structure based on a left-to-right decision tree so allow efficient nearest-neighbor searches," and the justification is a statistical claim about calls: most dispatched calls have one or two arguments, and "many of these cases can be resolved by considering only the first argument." nearest-neighbor is the right word: the index narrows the population, and the ranking still runs.

## Prolog: an index built on demand, over one argument, with a scan underneath

SWI-Prolog is the sharpest source on what a demand-built index over a ranked search costs, because the manual describes the index as an optimization the system declines to build until it is needed: "clause indexes are not built by the compiler (or asserta/1 for dynamic predicates), but on the first call to such a predicate where an index might help."

the index's shape is a narrowing, stated as such. the principal clause list "maintains a key, normally for the first argument" — refined to the first argument for which at least one clause has an indexable nonvar value — and deeper structure is reached by "deep indexing," which "creates hash tables distinguish clauses that share a compound with the same name and arity," applied recursively and "limited to 7 levels." and beneath the index there is still a scan: for a small predicate the system does "a linear scan for a possible matching clause using this index key."

the depth limit is the tell. an index over a structural key has to stop somewhere, and where it stops is where the linear work resumes. underneath all of it is clause order, which is the selection rule Prolog actually has: the first matching clause wins, so the population's order is semantics.

## GHC: match everything, then eliminate, with a rough filter in front

instance resolution in the GHC user's guide is CLOS's shape at compile time. step one is "Find all instances I that _match_ the target constraint; that is, the target constraint is a substitution instance of I." step two eliminates any candidate for which a strictly more specific candidate exists, subject to the overlap pragmas. the ending is Julia's: exactly one surviving non-incoherent candidate succeeds, "if more than one non-incoherent candidate remains, the search fails," and the incoherent case is the deliberate escape — the search then "succeeds, returning an arbitrary surviving candidate."

GHC's index is explicitly an approximation, and its source note says what it approximates and why: during instance lookup the class name and the argument type constructor names are used "to perform a 'rough match', _without_ poking inside the DFunId." the point of not poking is that resolving an instance's full type would force interface files to be loaded, so the filter is over the coarsest key that can rule a candidate out. matching runs afterward, on whatever survived.

## make: shortest stem, then the order of the file

the build system in the set makes the same choice as the languages and pays for it in the place a build system can least afford. when several pattern rules match a target, "make will choose the rule with the shortest stem (that is, the pattern that matches most specifically)," and on a tie, "if more than one pattern rule has the shortest stem, make will choose the first one found in the makefile."

so make ranks by specificity like CLOS, and resolves the residual tie by textual position. that is the property a rule system with registration cannot have if it wants the same answer under a different registration order, and make is the demonstration that the tie-break has to be decided deliberately: nothing about specificity ranking implies file order, and make chose it because a search over a list already has an order lying around.

## what the disagreement is

the six split on one rule and inherit everything else from it. the JVM demands an exact signature and finds one implementation or none. the other five admit inexact matches and therefore need a ranking, and the ranking forces a search.

the systems that search cannot index the answer, only the population. Julia's decision tree, Prolog's first-argument hash, and GHC's rough map are all the same construction: a cheap key that removes candidates which certainly do not apply, followed by the real rule over what remains. each states its own approximation — Julia's is statistical, Prolog's is depth-limited, GHC's is deliberately shallow so it does not have to read a type. CLOS's caches memoize a completed search against argument classes, which is a different move: not indexing the population but remembering an answer.

the second disagreement is what happens at a tie, and it does not follow from the first. Julia raises an error and names the candidates; GHC fails the search unless a pragma has licensed an arbitrary pick; make takes the first line in the file; Prolog takes the first clause; CLOS orders by class precedence and calls that an answer. the two that refuse are the two whose users write the candidate set as an unordered collection across files — which is the position a rule registry is in.

none of the six is a system where selection is both exact and dynamic. the JVM's exactness is a compile-time fact about a closed program; the five dynamic ones all rank. that gap is where a rule engine with a typed interface and no subtyping sits, and it is unoccupied without being rejected: the systems that could have taken it had subtyping, inheritance, or unification in the problem they were solving.

## questions for this project

pith already made the semantic half of this choice, and made it at the exact pole: 0015 selects on the request's interface alone, refuses to rank, and reports every candidate when two match. K-4 adds what make violates — "load and registration order cannot select behavior." so the selection rule is the JVM's, in a system whose population is built at run time by registration.

that combination is what makes an index total here, where every system above has an approximate one. every mechanism above hedges because a narrowed candidate set still has to be ranked; with no ranking to run, the key *is* the answer, and the bucket a key reaches is the whole outcome including the ambiguity case. the questions that remain are the ones the exactness does not settle: what the key is derived from, whether two interfaces the type system considers equal can encode differently, and what the population's growth actually costs — which 0050 already named, in the observation that distinguishing two rules costs a nominal type, so the rule count grows with the domain model.

what none of the six offers is a precedent for the cost being negligible. the JVM's constant-time dispatch is bought by a compiler; the dynamic five all pay a per-call search their indexes only bound. a rule engine that wants the JVM's cost profile without the JVM's compiler has to get it from the selection rule being strict enough to hash — which is an argument for 0015 that 0015 did not make, and a reason not to weaken it later for ergonomics.

## sources

- [The Java Virtual Machine Specification, Java SE 21: §5.4.3.3 Method Resolution](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html)
- [Common Lisp HyperSpec: 7.6.6 Method Selection and Combination, 7.6.6.1 Determining the Effective Method](https://www.lispworks.com/documentation/HyperSpec/Body/07_ffa.htm)
- [Julia manual: Methods](https://docs.julialang.org/en/v1/manual/methods/)
- [Julia developer documentation: Julia Functions](https://docs.julialang.org/en/v1/devdocs/functions/)
- [SWI-Prolog manual: Just-in-time clause indexing](https://www.swi-prolog.org/pldoc/man?section=jitindex)
- [GHC User's Guide: instance declarations and resolution](https://ghc.gitlab.haskell.org/ghc/doc/users_guide/exts/instances.html)
- [GHC source notes: `compiler/GHC/Core/InstEnv.hs`, the rough-match fields](https://ghc-compiler-notes.readthedocs.io/en/latest/notes/compiler/types/InstEnv.hs.html)
- [GNU make manual: how patterns match](https://www.gnu.org/software/make/manual/html_node/Pattern-Match.html)
