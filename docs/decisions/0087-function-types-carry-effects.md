---
schema: design-doc/v1
id: decision-0087-function-types-carry-effects
title: function types carry their effect category, and an action is authorized against the root's grant on the input whose rule plans it
summary: a function type is written with its category, `(Path) -> Object` for a pure function and `(Path) -> action Object` for one whose call leads to an action; the category of a function value is inferred from the body of the rule or lambda it refers to; categories must match exactly; the `pure` and `action` keywords on a rule keep describing the form of its own body; only an action rule plans an action; a plan is authorized against the grant the root wrote on the input instance whose rule planned it; the root project writes its own grant in a header clause, which has no effect when the project is loaded as an input
kind: decision
status: proposed
created: 2026-09-30
updated: 2026-09-30
tags:
  - language
  - types
  - effects
  - security
relations:
  informed_by:
    - research-passing-behaviour
  depends_on:
    - foundation-principles
    - decision-0019-effect-categories-and-nondeterminism
    - decision-0026-generic-typed-calculus
    - decision-0047-the-declaration-table
    - decision-0072-action-plans-are-authorized-against-consumer-grants
    - decision-0080-inputs-are-parameters
    - decision-0082-rules-are-called-by-name-and-functions-are-values
  supersedes: []
  amends:
    - decision-0026-generic-typed-calculus
    - decision-0047-the-declaration-table
    - decision-0072-action-plans-are-authorized-against-consumer-grants
    - decision-0080-inputs-are-parameters
    - decision-0082-rules-are-called-by-name-and-functions-are-values
---

# function types carry their effect category, and an action is authorized against the root's grant on the input whose rule plans it

> amends [0082](0082-rules-are-called-by-name-and-functions-are-values.md): function types are written with
> a category. amends [0026](0026-generic-typed-calculus.md): the category is part of a function type, the
> one place a value type carries it. amends [0047](0047-the-declaration-table.md): its constructor-set audit
> removed categories from value types, and they return in function types. amends
> [0072](0072-action-plans-are-authorized-against-consumer-grants.md) and [0080](0080-inputs-are-parameters.md):
> a root's grant to itself is written in a header clause.

## context

0082 writes a function type as `(Path) -> Object`. the type does not say whether calling the value leads to
an action. in 0082's `Source` example a library's `compile` field can hold an action rule, such as a C
compile, and 0082's `build` calls it through the record field. a reader of `build`'s signature cannot see
that it runs a compiler, and a type check cannot tell a pure function from one that needs a grant.

0019 made the categories distinct types in the kernel IR. [0047](0047-the-declaration-table.md) removed them
from value types because no value ever had the type `Action<Object>`: rules had categories and values did
not. function values change that, since a function value is a rule and its bound arguments.

0072 authorizes a plan against "the grant for the module whose rule planned it", and says the root gets no
grant by default, "so that everything a project allows is written in one place". 0080 moved grants onto
inputs. the root is not an input of anything, so there is no place left to write the root's grant to
itself.

lambdas are identified by the digest of their body (0082's edit), so two projects that write the same lambda
share one identity. if a lambda could plan an action, its identity would not say which project's grant
applies.

## decision

### the category is part of the function type

```
(Path) -> Object                 -- pure
(Path) -> action Object          -- its call leads to an action
(Query) -> observation Status    -- makes an observation
```

`Mutation` and `Opaque` are written the same way when 0019's markers become operational.

categories must match exactly: a value of type `(Path) -> Object` is not accepted where
`(Path) -> action Object` is expected, and the reverse is refused too. a library that takes behaviour of
either kind declares the category it will call.

### the category is inferred from the body

the category in a function value's type is inferred from the body of the rule or lambda it refers to, by
one rule for both: it is `action` if the body plans an action or calls an action-typed function, by name or
as a value, and pure otherwise. `(p) -> c.compile_with(p, flags)` is an action-typed lambda, and 0082's
`build`, which calls `s.compile`, has the type `(List<Source>) -> action List<Object>` as a value; another
project reaches it as `cpackage.build`.

the `pure` or `action` keyword on a rule declaration keeps its existing meaning: it says whether the rule's
own body plans an action with `plan` and `complete`. it says nothing about what the body calls, so a `pure`
rule can have an action-typed function value.

a pure rule may call a function whose type says `action`, as a pure body may request an action today; the
call is a graph request, and the engine plans, authorizes and runs the action before the pure rule
continues.

only an action rule plans an action. a lambda or a pure rule that calls an action rule plans nothing
itself; the action is planned by the rule called.

### authorization follows the rule that plans

a plan is authorized against the grant the root wrote on the input instance whose rule planned it,
whatever path of calls reached it: a direct call, a call through a function value stored in a
record, or a call inside a lambda written in another project. in the `Source` example, a C compile reached
through `cpackage.build` from an application whose root declares a `c` input is authorized against the
grant on that input.

`pith explain` names that input and grant for every action.

### the root grants to itself in its header

a root project's own action rules are authorized against a `grant` clause in its header, after `inputs`:

```
inputs {
  c = path"modules/c-support" grant { programs: [host"/usr/bin/cc"], network: deny }
}

grant {
  programs: [host"/usr/bin/pandoc"],
  network: deny,
}
```

the clause is parsed in every project and carries authority only in the root, as registry bindings and
domain routes do. when the project is loaded as an input, its `grant` clause has no effect, and its actions
are authorized against the grant the root wrote for it. 0072's rules otherwise stand: no project has a grant
the root did not write, and a dependency cannot grant.

## alternatives considered

keeping function types without a category, as 0082 wrote them, leaves the question of whether a call needs a
grant to evaluation time and hides it from the signature a caller reads.

taking a function value's category from its rule's keyword would type `cpackage.build` as pure although
calling it runs a compiler.

letting a pure function be passed where an action-typed one is expected would add the calculus's first
subtyping rule. 0026 makes structural equality type equality, and this record does not change that; the cost
is that a library declares one category for the behaviour it accepts.

authorizing an action against the project where the call was written would make a library that forwards a
function value responsible for actions it cannot see, and would give two identical lambdas in two projects
two different authorities under one identity.

a default grant for the root, or reading the root's grant from outside the project file, would put what a
project allows somewhere other than its project file, which 0072 refuses.

## evidence

to be measured in the slice that builds function values:

- a value of type `(Path) -> Object` passed where `(Path) -> action Object` is expected is refused, naming
  both types, and the reverse is refused
- a lambda that calls an action rule is typed as an action, and one that does not is pure
- a `pure` rule that calls an action-typed field, used as a value, has an action type
- a C compile reached through a function value stored in a record is authorized against the grant the root
  wrote on the input instance whose rule planned it, and refused, naming that input, when the grant does not cover the
  compiler
- `pith explain` names the input and the grant for each action
- a root's own action rule is refused without a `grant` clause and runs with one
- the same project loaded as an input ignores its own `grant` clause and is authorized against the grant
  the root wrote for it

## unresolved

a project reached only transitively has no input in the root on which to write its grant. this waits on
0080's open question of how the root names a deep input.

0018 has not been checked against function values, which make unbounded recursion easier to write; this
record adds no bound.

a host body that receives a function value needs a typed import to call it, which is owed as an amendment to
0073.

whether observation-typed functions need grants of their own, which depends on how observations are
authorized when M-15 exercises them.
