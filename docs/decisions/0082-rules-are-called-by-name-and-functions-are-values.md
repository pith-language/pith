---
schema: design-doc/v1
id: decision-0082-rules-are-called-by-name-and-functions-are-values
title: rules are called by name, functions are values, and behaviour reaches a library only by being passed
summary: a call names its rule, locally or through an input; a function value is a rule's identity plus arguments already bound, with a canonical encoding; a lambda is an anonymous rule identified by its body digest; map joins fold as a builtin construct; selection by interface, ask and ambiguity refusal are removed
kind: decision
status: proposed
created: 2026-09-21
updated: 2026-09-30
tags:
  - language
  - graph
  - extensibility
relations:
  informed_by:
    - research-passing-behaviour
    - research-dispatch
    - research-extension-interfaces
  depends_on:
    - foundation-principles
    - decision-0004-first-party-without-privilege
    - decision-0018-termination-and-recursion
    - decision-0023-rule-and-cache-identity
    - decision-0026-generic-typed-calculus
    - decision-0038-represented-rule-bodies
    - decision-0050-cycle-detection-over-the-computation-key
    - decision-0080-inputs-are-parameters
  supersedes:
    - decision-0015-interface-rule-selection
  amends:
    - decision-0026-generic-typed-calculus
    - decision-0056-peerhood-is-a-registered-crate
---

# rules are called by name, functions are values, and behaviour reaches a library only by being passed

> edit, 2026-09-30: function types carry an effect category inferred from the body they refer to
> ([0087](0087-function-types-carry-effects.md)), so the `compile: (Path) -> Object` field of the
> `Source` example is written `(Path) -> action Object`, and `build` as a value has an action type. a rule
> of another project, such as `greetings.message(g)`, is reachable only when that project publishes it as an
> output ([0088](0088-outputs-are-public.md)).

> edit, 2026-09-21: after [passing behaviour](../research/passing-behaviour.md), a function value pins the revision it refers to, identities are defined for named rules and lambdas, captures are visible and bounded, and some types cannot be captured. see the edit section.

> supersedes [0015](0015-interface-rule-selection.md). amends [0026](0026-generic-typed-calculus.md) with
> function types, and [0056](0056-peerhood-is-a-registered-crate.md): replacement is shown by passing an
> input.

## context

under 0015 a rule is never called. a request asks for a type, and whichever loaded rule's interface matches
answers it. so what runs depends on everything loaded, and the file does not say which rule that is: a
project with no rule of its own for a request can have it answered by a rule some input happens to declare.
the principles say "make authority and dependency explicit", and "replacement names the value or behavior
being replaced".

a comparison of eight designs on 2026-09-21 found that selection by type does not provide the openness it
was meant to: a list has one element type, so a library building a mixed list of C and Zig sources has to
name both languages in a sum type anyway.

0015 rejected calls by name because "names live outside the typed model and would let first-party rules
hide behind identifiers third parties cannot match". with calls qualified by input and inputs replaceable
under [0080](0080-inputs-are-parameters.md), every name a call uses is one the consumer chose and can
replace.

## decision

### calls name their rule

a rule is called by its name: `message(g)` for a rule in the same project, `greetings.message(g)` for one
from an input. a call is a computation in the graph, keyed on the rule's identity and revision and on the
argument values (0023), cached and explainable as requests are today. only the addressing changes.

action rules are called the same way; the call returns once the engine has planned, authorized and run the
action.

### functions are values

a rule is a value of a function type, written `(Path) -> Object`. a function value is data: the identity of
a rule plus the arguments already bound to it. it has a canonical encoding, so it can be part of a
computation key, stored in engine state, and compared. calling a function value is a computation keyed on
the rule, its revision, the bound values and the new arguments.

a lambda, `(p) -> c.compile_with(p, flags)`, is an anonymous rule identified by the digest of its body
(0038), with the values it uses from around it, here `flags`, bound into the value.

### map is a builtin construct

`map(list, f)` joins `fold` as a construct of the language. user code still cannot declare generic rules;
0026's withdrawal of user-defined generics stands.

### behaviour is passed

a library that needs behaviour from its user takes it as an argument, a record field, a list entry, or an
input. what runs is never chosen by searching the loaded program, by scope, or by a registry. open
extension is written as data that carries its behaviour:

```
type Source = { path: Path, compile: (Path) -> Object }

rule build(sources: List<Source>) -> List<Object> = {
  map(sources, (s) -> s.compile(s.path))
}
```

### what is removed

- selection by interface, `ask`, and the ambiguity refusal `E-1102`
- `pith graph select`
- the "which rule can provide a type" query of the rules-and-graph design


## edit, 2026-09-21: after the research

the research makes five points of the decision above precise. the text above is kept as first written.

- a function value pins the revision of the rule it refers to, as Unison's hashes pin dependencies. a
  stored value keeps its meaning when the rule is edited; calling it runs the pinned revision. a stored
  value whose revision can no longer be loaded is refused, naming the rule and the revision, until a
  retention policy says otherwise.
- a named rule's identity is its coordinate together with its body digest. a lambda's identity is the digest
  of its body in the de Bruijn form 0062 already uses, so lambdas that differ only in variable names are one
  rule. two named rules with the same body stay two rules; Unison made unique types the default for the
  same reason.
- the values a function value captures are part of it and are shown by `pith explain`. a captured value
  above a size limit is held by content identity instead of inline, so accidental capture of large values,
  the problem behind Spark's ClosureCleaner and Bazel's `map_each` restriction, costs a digest and is
  visible.
- the canonical encoding shares repeated structure and contains nothing that depends on load order
  (Unison #3280, #3449).
- values that must not be captured, such as host handles, have types a function value cannot hold, and
  the type checker enforces this. equality of function values is structural; pith does not claim that two
  functions computing the same results are equal.

## alternatives considered

keeping selection by type, alone or as an opt-in extension point with a consumer-written provider list,
was compared with modules taking parameters, scoped handlers, and three uses of function values. handlers
make every computation below them depend on the handler, so results stop being shared between projects.
parameterized modules reach the same results as function values plus 0080's arguments with a larger
construct. provider lists add a second calling mechanism and need open sums or subtyping. the comparison is
kept outside the notebook, and the research note below is meant to carry its evidence.

closures with captured environments, as in most languages, have no canonical encoding. the bound
arguments of a function value are ordinary values, so a function value has one.

## evidence

- a library builds a list mixing sources from two language libraries it does not name
- a lambda with a bound value is cached, and changing the bound value recomputes it and nothing whose key
  does not involve it
- `pith explain` names the rule a function value called
- an external library replaces a first-party one by being passed as an input, which is 0056's peerhood
  claim in the new form

## unresolved

0018 needs checking against function values, which make unbounded recursion easier to write. identical
calls are caught by 0050's cycle detection on the computation key and the rest by the run bound (0059);
whether that is sufficient is the check.

host bodies that receive function values need typed imports to call them; that is an amendment to 0073.

the research note on function values (Unison's hashed definitions, OCaml's first-class modules and
functors, Scala's `given`, Nix's `callPackage`, defunctionalization) is owed.

at a call such as `s.compile(s.path)` the reader does not see which rule runs; provenance records it. the
tooling that shows it in an editor is not designed.
