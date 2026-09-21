---
schema: design-doc/v1
id: decision-0084-builtins-and-machine-facts
title: builtins are the language's own vocabulary, unqualified and fixed per language version, and facts about the machine enter a project only as input defaults
summary: a builtin must be total, pure, defined by the evaluator, and free of domain meaning; builtins are unqualified and cannot be shadowed; after the first release the set is fixed per language version a project declares; the cli contract types and machine facts live under pith.; pith.native and anything else read from the machine may appear only as a default in the inputs block
kind: decision
status: proposed
created: 2026-09-21
updated: 2026-09-21
tags:
  - language
  - compatibility
relations:
  informed_by: []
  depends_on:
    - foundation-principles
    - decision-0062-the-ir-constructor-set
    - decision-0077-pre-release-pith-is-experimental
    - decision-0078-kernel-surface-grows-on-demand
    - decision-0080-inputs-are-parameters
  supersedes: []
---

# builtins are the language's own vocabulary, unqualified and fixed per language version, and facts about the machine enter a project only as input defaults

## context

the elaborator resolves twelve names without a declaration (`BUILTIN_NAMES` in
`crates/pith-elaborator/src/body.rs`): `module`, `describe`, `append`, `concat`, `decode`, `split`,
`before`, `contains`, `holds`, `sort`, `strip_prefix` and `fail`. a local binding with one of those names is
refused (`FrontendCode::BuiltinShadowed`).

the explicit model requires every name to have a stated origin. for builtins the origin is the same in every
project: most are composites over the IR constructors (0062), their meaning is pinned by the evaluator's version, and
nothing can add to or replace them.

## decision

### what may be a builtin

a builtin is total, pure, defined by the evaluator, and has no domain meaning. `concat`, `split`, `map` and
`fold` qualify. anything with meaning beyond the language belongs in a library, even one pith ships, under
0078's rule.

### builtins are unqualified and cannot be shadowed

builtins are used without a prefix. a declaration or binding with a builtin's name stays refused, so a
builtin name means the same thing in every file.

### the set is fixed per language version

after the first release a project declares the language version it is written for, and the builtin set is
fixed per version, so a builtin added later cannot collide with a name an existing project declares. before
the first release the set changes freely (0077). where the version is declared, in the project file or the
lock, is left to the release.

### what lives under pith.

- `pith.Exec`, `pith.Verdict` and `pith.Platform`, the types a project and the CLI share
- `pith.native`, the platform pith is running on, and any other fact read from the machine

### machine facts enter only through inputs

a fact read from the machine may appear only as the default of an input:

```
inputs {
  target : pith.Platform = pith.native
}
```

anywhere else it is refused, and the diagnostic points to the input to use. every machine fact a project
uses is then in its inputs block, where a consumer can pass a different value.

## alternatives considered

qualifying every builtin, `pith.concat`, would make `pith` the only implicit name and remove collisions. it
adds a prefix to every use that tells the reader nothing.

an explicit import list per file, `use pith.{concat, split}`, is short, but every file would repeat a list
that means the same thing in all of them.

letting a project shadow a builtin would remove collisions, and one name would then mean two things.

## evidence

- a local binding named `concat` is refused
- `pith.native` outside an input default is refused with a diagnostic naming the input form
- an output that depends on `target` gets a different key when `target` is passed

## unresolved

whether `map` is a builtin function or a construct like `fold` is the implementation round's; 0082 treats it
as a construct.
