---
schema: design-doc/v1
id: decision-0073-the-component-interface-is-generated-from-declarations
title: a module's WIT world is generated from its declarations, and every value a component returns is checked against its declared type
summary: the component adapter generates one WIT world per module from the `.pi` declarations, importing a `pith:core` package and one typed import per declared request; pith types map to WIT by a fixed table, with integers split into a mandatory small case and a big case, nominal identity restored by the engine, and recursive values flattened; returned values are checked with Value::is_type; the generator targets one WASI release at a time and records now how it will use each unshipped feature
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-30
tags:
  - wasm
  - types
  - extensibility
relations:
  informed_by:
    - research-wasm-components
  depends_on:
    - foundation-principles
    - decision-0047-the-declaration-table
    - decision-0048-pre-release-version-pinning
    - decision-0055-arbitrary-precision-int
    - decision-0061-the-declaration-artifact
    - decision-0071-host-bodies-bind-through-host-adapters
  supersedes: []
---

# a module's WIT world is generated from its declarations, and every value a component returns is checked against its declared type

> edit, 2026-09-30: amended by [0085](0085-typed-literals.md). a value of a guarded nominal returned across
> the component boundary is checked again, with the check its declaration names, before the engine accepts
> it, beside the type check of the returned-values section.

> edit, 2026-09-29: under [0082](0082-rules-are-called-by-name-and-functions-are-values.md) there is one
> typed import per rule a body may call, where this record had one per requested interface, and a body that
> receives a function value needs a typed import to call it. `Path`, `RelPath` and `HostPath`
> ([0081](0081-paths-are-values.md)) need rows in the type table.

## context

under [0071](0071-host-bodies-bind-through-host-adapters.md) a module's `.pi` declarations define its
types and interfaces, and a host adapter binds its `= host` bodies. for the component adapter that
binding needs a WIT world.

[the research note](../research/wasm-components.md) records three facts that constrain it. WIT has no
recursive types and no unbounded integer. type identity at the component boundary is structural: a
record's name is not part of its type, so a WIT name cannot carry a pith nominal type. and Wasmtime's
security policy tells embedders they "should never blindly trust values from the guest".

## decision

### the world is generated

the component adapter generates one WIT world per module from its declarations. authors build against the
generated WIT with `wit-bindgen` and do not write WIT by hand. the declarations remain the only
definition of the module's types.

the world imports:

- `pith:core`, a package pith publishes with the types every domain shares: content identities, the
  `ActionSpec` contract, action executions, diagnostics, and `read-blob`
- one typed import per interface in each host declaration's request list (0071)

it exports one function per host declaration: an `async func` for a pure rule, and `plan` and `complete`
for an action rule. each allowed request is a separate import, so a component that requests an undeclared
interface fails to link.

### names

pith names are mapped to WIT's kebab-case identifiers by a fixed rule. a module in which two pith names
map to the same WIT identifier is refused at generation.

### types

| pith | WIT | note |
| --- | --- | --- |
| `Unit` | empty tuple | |
| `Bool` | `bool` | |
| `Int` | `variant { small(s64), big(list<u8>) }` | `small` whenever the value fits; `big` holds 0055's encoding |
| `Text` | `string` | |
| `Bytes` | `list<u8>` | |
| `Blob` | a `pith:core` content identity | the bytes are read through `read-blob` |
| `Nominal` | a named WIT type over the representation | the engine restores the nominal identity |
| `List<T>` | `list<T>` | |
| `Record` | `record` | fields in pith's canonical order |
| `Sum` | `variant` | |
| `Cut` | a node list and a root index | each self-reference becomes a `u32` index |

`Int` has exactly one encoding per number because `small` must be used whenever the value fits. 0055 took
the same rule from RFC 8949: two encodings of one number would give it two digests.

a nominal value crosses as its representation, and the engine restores its identity from the declared
type on the way back. the generated WIT type is named after the pith type so that guest bindings keep the
distinction in their own language.

a recursive value crosses as a list of nodes and a root index. pith allows only direct self-reference, so
every recursive value is a tree and can be written this way. a list whose indices do not form a tree
reachable from the root is a host fault.

### returned values are checked

every value a component returns is decoded into a pith value and checked with `Value::is_type` against
the declared output type. every `ActionSpec` is validated as a host rule's plan is, and then authorized
under [0072](0072-action-plans-are-authorized-against-consumer-grants.md). a value that fails a check is a
host fault naming the component, the export and the declaration, and it is never stored as a result.

### WASI releases and unshipped features

the generator targets one WASI release at a time, currently 0.3.1. it uses async functions, `future` and
`stream`, and `implements` once a grant takes the form of an import. it does not use `map`, because pith
has no map type (0047 removed it).

the use of each unshipped feature is decided now, so a new release only requires regenerating worlds:

| feature | use |
| --- | --- |
| canonical interface names | adopt, for matching older `pith:core` versions |
| nested namespaces | adopt, since `domain/name` subjects map onto it |
| fixed-length lists | adopt, so content identities become `list<u8, 32>` |
| async lift without a callback | adopt, so guests do not need a callback loop |
| `stream.forward` and `future.forward` | adopt, so blobs are not copied through the guest |
| value imports | not used; inputs are request arguments |
| `error-context` | refused at load; its contents are nondeterministic |
| threading built-ins | refused at load; scheduling is nondeterministic |
| memory64, getters and setters, more async ABI options | not used until a guest needs them |

### versions of pith:core

under 0048 nothing is released and every format stays at version 1; `pith:core` counts as a format.
until pith's first release it can change freely, and components built against an older copy are rebuilt.
from the first release on, each published `pith:core` version is frozen and the adapter converts results
from older versions to current types, as Zed does for its extension API.

## alternatives considered

hand-written WIT beside the `.pi` declarations would be a second definition of the same types that could
drift from the first.

a generic `value` type in `pith:core`, with every request and result passed as encoded bytes, would avoid
per-interface imports. the WIT boundary would be untyped, `is_type` would be the only check, and a guest
could request any interface.

refusing recursive types at the boundary would be simpler, but it would rule out tree-shaped domain
values. flattening is mechanical because pith allows only direct self-reference.

`Int` as `s64`, refusing values that do not fit, would give up 0055's unbounded integer at the boundary.

## evidence

the witness is `example-domain` rewritten as a module outside the workspace: declarations in `.pi`, the
render action and rules in a component built from rust with `wit-bindgen`, loaded by path. it passes when
its contract tests give the same results as the rust crate's, and its render action plans an `ActionSpec`
with the same digest.

each refusal is a test that checks its diagnostic:

| case | stage |
| --- | --- |
| a component importing an interface outside its request list | load, at link |
| a component using `error-context` or threading built-ins | load |
| two pith names mapping to one WIT identifier | generation |
| a returned value that does not match its declared type | run, host fault |
| an `Int` in the `big` case that fits in `s64` | run, host fault |
| a flattened recursive value whose indices do not form a tree | run, host fault |

## unresolved

the name mapping rule needs to be written out and tested against the corpus's names.

whether `Text` must be checked for anything beyond what WIT's `string` already guarantees depends on
pith's own `Text` invariants, which should be stated where `Text` is defined.
