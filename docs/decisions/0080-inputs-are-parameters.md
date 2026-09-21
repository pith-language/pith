---
schema: design-doc/v1
id: decision-0080-inputs-are-parameters
title: a project's inputs are its parameters, each with a default, and a consumer passes others with `with`
summary: an input is another project or a typed value, each with a default; the inputs block is ordinary pith that may not use anything coming from an input; `with` passes arguments to an input, including replacing a project it uses, and the loader checks a replacement against every name the consumer uses; grants are written on the input they apply to
kind: decision
status: proposed
created: 2026-09-21
updated: 2026-09-21
tags:
  - modules
  - language
  - configuration
relations:
  informed_by:
    - research-project-declaration
    - research-module-distribution
  depends_on:
    - foundation-principles
    - decision-0069-module-authority-is-consumer-configuration
    - decision-0070-module-acquisition-resolution-and-replay
    - decision-0072-action-plans-are-authorized-against-consumer-grants
    - decision-0079-a-project-is-one-file
  supersedes: []
  amends:
    - decision-0072-action-plans-are-authorized-against-consumer-grants
---

# a project's inputs are its parameters, each with a default, and a consumer passes others with `with`

> edit, 2026-09-21: after [declaring a project](../research/project-declaration.md), locators and revisions are literals, arguments not declared by the input are refused, instances are keyed by their arguments, and replacement below a direct input is left to the root. see the edit section.

> amends [0072](0072-action-plans-are-authorized-against-consumer-grants.md): grants are written on the
> input they apply to, in the project file. there is no separate manifest section for them.

## context

flakes declare dependencies in one place and let one flake's outputs be another's inputs. their inputs
section is a literal attribute set that is parsed and never evaluated, because Nix evaluation can fetch and
build (import-from-derivation, `builtins.fetch*`), and locking and fetching must not run code. so a flake
starts with a restricted sublanguage and continues in the full one.

pith evaluation is pure and bounded, and effects happen only through actions the engine plans and
authorizes. evaluating an inputs block, including one from an untrusted repository, has no effects.

the decision taken on 2026-09-21 is that values and behaviour are passed explicitly wherever possible.
inputs are where a project receives them.

## decision

### an input is a parameter with a default

an input is either another project, named by a locator, or a typed value:

```
inputs {
  greetings = path "../greetings"
  cards = git "https://example.org/cards" at "v2"
  style : (Text) -> Text = (t) -> t
  target : pith.Platform = pith.native
}
```

the locator or value after `=` is the default. a locator is not a path value; it says where a project is
fetched from, and 0069's rules for routing and 0070's for acquisition and replay apply to it.

### the inputs block is ordinary pith

`let`, records, functions defined in the file and conditionals may be used in the inputs block. nothing in
it may use anything that comes from an input, since the block decides the inputs. the elaborator refuses a violation and names the reference.

### `with` passes arguments

a project that uses another passes arguments with `with`:

```
inputs {
  loud = path "../greetings" with { style = (t) -> concat(t, "!!!") }
  plain = path "../greetings"
  cards = git "https://example.org/cards" at "v2" with { greetings = loud }
}
```

- the same project used twice with different arguments is two inputs
- passing a project for one of an input's own project inputs replaces it wherever that input uses it,
  which covers what flakes do with `follows`
- a dependency's manifest cannot pass arguments to the consumer, only the consumer to its inputs, as 0069
  requires of routing

### replacements are checked

when a project is passed in place of an input's default, the loader checks it against every name the
receiving project uses from that input, with the same types, using the ABI. a missing or changed name is
refused before evaluation, naming the name, the type expected, and where the replacement was passed.

### grants sit on the input

a grant is written on the input it applies to:

```
inputs {
  render = path "../render" grant {
    programs: [host"/usr/bin/pandoc"],
    network: deny,
  }
}
```

0072's rules otherwise stand: only the root project grants, there is no default grant, and a dependency
cannot grant.


## edit, 2026-09-21: after the research

the research changes four points of the decision above, whose original text is kept.

- locators and revisions are literals. the rest of the inputs block (value defaults, `with` arguments,
  grants) stays ordinary pith under the scoping rule. the research gives two reasons: tools must be able to
  rewrite locators without evaluating the file, and a locator computed from other values could carry those
  values off the machine. Dhall refuses computed import hosts for the second reason (dhall-lang #378).
- an argument in `with` that the input does not declare is refused, naming it, as Zig refuses unknown
  options.
- an input instance is keyed by its argument values. two consumers passing different values get two
  instances, and values from different consumers are never merged; Cargo's feature unification shows that
  merging forces parameters to be additive.
- `with` passes arguments to a direct input. only the root project replaces something deeper in the graph,
  as with Bazel overrides, Cargo `[patch]` and Go `replace`. there are no chains of nested redirections;
  Nix's `follows` bugs are in that mechanism (nix#4808, #5790). how the root names a deep input is left to
  the implementation round.

## alternatives considered

a literal-only inputs section, as in flakes, would be simpler to read without evaluating. evaluating the
block in pith has no effects, so the restriction would take away `let`, functions and conditionals without
making loading any safer.

letting an input depend on another input's outputs, a generalized `follows`, is possible: to the engine it
is an ordinary dynamic dependency (0007). it is left out of the first version because it makes the input
set depend on evaluation, which complicates locking and editor loading.

replacing an input without a check would let a replacement fail deep inside evaluation with an error that
does not mention the replacement.

## evidence

- two inputs of one project with different `with` arguments evaluate to different results and share
  nothing whose key does not involve the argument
- a replacement missing a used name is refused at load with the diagnostic above
- an inputs block that uses a name from an input is refused, naming the reference

## unresolved

how value inputs are passed from the command line (`--input name=value`) needs a rule for which types can
be written as text.

the research note owed by [0079](0079-a-project-is-one-file.md) covers flakes' inputs and `follows`, and
should confirm or refute that a scoping rule is enough where flakes chose a sublanguage.
