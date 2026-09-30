---
schema: design-doc/v1
id: decision-0085-typed-literals
title: values narrower than text are written as tagged literals from a closed set checked at load
summary: a tagged literal `tag"..."` is one token; the tag set is fixed per language version and every `ident"` prefix is reserved; each tag has one printed form and a total check that runs at load, and a literal in another form is refused; locators, revisions, digests and keys in the header are respelled as tagged literals, with lower-case hex; a git input names one revision kind and the lock pins the commit, which a locked read does not re-judge and `pith update` reports as conflicted when a tag moves; a guarded nominal names its check, can be constructed only by the project that declares it, and runs its check at every construction and on every value a host rule or component returns; a value input can be given on the command line exactly when its type has a literal form
kind: decision
status: proposed
created: 2026-09-30
updated: 2026-09-30
tags:
  - language
  - types
  - modules
relations:
  informed_by:
    - research-typed-literals
    - research-project-declaration
  depends_on:
    - foundation-principles
    - decision-0026-generic-typed-calculus
    - decision-0047-the-declaration-table
    - decision-0054-a-written-digest-names-its-algorithm
    - decision-0069-module-authority-is-consumer-configuration
    - decision-0070-module-acquisition-resolution-and-replay
    - decision-0077-pre-release-pith-is-experimental
    - decision-0079-a-project-is-one-file
    - decision-0080-inputs-are-parameters
    - decision-0081-paths-are-values
    - decision-0084-builtins-and-machine-facts
  supersedes: []
  amends:
    - decision-0026-generic-typed-calculus
    - decision-0047-the-declaration-table
    - decision-0069-module-authority-is-consumer-configuration
    - decision-0070-module-acquisition-resolution-and-replay
    - decision-0073-the-component-interface-is-generated-from-declarations
    - decision-0079-a-project-is-one-file
    - decision-0080-inputs-are-parameters
    - decision-0081-paths-are-values
    - decision-0084-builtins-and-machine-facts
---

# values narrower than text are written as tagged literals from a closed set checked at load

> amends [0079](0079-a-project-is-one-file.md) and [0080](0080-inputs-are-parameters.md): locators,
> revisions, digests, keys and workspace members are respelled. amends [0081](0081-paths-are-values.md):
> its `rel"..."` and `host"..."` become two tags of the set below. amends
> [0084](0084-builtins-and-machine-facts.md): the tag set is fixed per language version beside the builtins,
> and `pith.` gains the types the tags produce. amends [0026](0026-generic-typed-calculus.md): nominal
> declarations gain the `guarded` attribute and the check it names. amends
> [0047](0047-the-declaration-table.md): guarded nominals narrow the forgeable-value residual for values
> built in pith source or returned by a host rule or component. amends
> [0073](0073-the-component-interface-is-generated-from-declarations.md): a guarded value returned across
> the component boundary is checked again. amends
> [0069](0069-module-authority-is-consumer-configuration.md): a root key is written `key"..."` and a quoted
> key is refused. amends [0070](0070-module-acquisition-resolution-and-replay.md): a git revision names its
> kind, and one revision string no longer means either a commit or a ref.

## context

the project header writes most of its values as quoted text: `path "modules/greeting"`, `git
"https://example/f" at "abc123" subpath "sub"`, `archive "..." digest "blake3:ff"`, `registry main =
"https://..." root "ed25519:AAAA"`, `workspace { members: ["modules/greeting"] }`. the parser turns each into
a typed field by reading it as text and then checking its contents, and some of these checks are weaker or
later than the spelling other records fix: `RootKey::parse` accepts any two non-empty halves around the first colon,
and the `ed25519:<hex>` shape [0069](0069-module-authority-is-consumer-configuration.md) records is checked
only when the registry uses the key. one git revision string means either a commit or a ref: 0080's example
writes a tag, `at "v2"`, while the parsed locator documents the field as one immutable revision.

inside the language, `pith.Exec` is `{ arguments: List<Text>, program: Text }` and `ActionSpec`'s toolchain,
arguments, paths and environment are strings. [0081](0081-paths-are-values.md) typed paths and left open how
the other string fields become typed.

[typed literals](../research/typed-literals.md) reads how other systems handled this.

## decision

### the token

a tag written directly before a string literal, with no space between, is one token: `url"https://..."`.
the lexer reserves every identifier immediately followed by `"`, so `ident"..."` is a tagged literal or an
error, never an identifier followed by a string. a tag outside the set is refused at the tag, naming it. the
payload uses the escapes ordinary strings use, and a diagnostic inside a payload points at the source bytes
rather than the decoded text.

`./x` remains the `Path` literal of 0081.

### the set

| literal | type | accepts |
| --- | --- | --- |
| `./src/main.c` | `Path` | an existing file or directory under the project root, as 0081 defines |
| `rel"bin/hello"` | `RelPath` | a relative location inside a tree, without `.`, `..` or empty components |
| `host"/usr/bin/cc"` | `HostPath` | an absolute location on the machine, without `.`, `..` or empty components |
| `url"https://..."` | `pith.Url` | an absolute URL whose scheme is in the tag's fixed set |
| `digest"blake3:..."` | `pith.Digest` | 0054's written form, in lower-case hex |
| `key"ed25519:..."` | `pith.PublicKey` | 0069's written form, in lower-case hex |
| `commit"..."` | header syntax only | 40 lower-case hex digits |
| `tag"v3"`, `branch"main"` | header syntax only | a ref name git accepts |
| `path"modules/greeting"` | header syntax only | a directory inside the project that holds a `pith.pi` |

anything else is refused at load with its own code. for `Path` that includes the refusals of 0079's
containment edit (E-3072, E-3039).

the checks are total, pure and defined by the evaluator. they read nothing but the literal, except that
`Path` and `path"..."` read the project directory, which is the project's declared input (0081). a
`host"..."` check never looks at the machine.

the algorithms a digest or a key admits are the ones the tree already defines in one place, `pith_ids`'s
constant for digests (0054) and the registry keys' `ALGORITHM` constant for keys; the tag set does not
restate them.

hex in a digest, a key or a commit is lower-case. the restriction is new to this record; 0054 and 0069 do
not fix the case, and the current parsers accept upper-case hex.

the set is closed and, after the first release, fixed per language version, beside the builtins (0084).
before the first release it changes freely (0077).

### one printed form

each literal kind has one printed form. a literal written in another form is refused with the form it
should take. nothing is normalized, so no value has two spellings and `pith fmt` prints each literal as
written.

### the header

| before | after |
| --- | --- |
| `path "modules/greeting"` | `path"modules/greeting"` |
| `git "https://example/f" at "abc123" subpath "sub"` | `git url"https://example/f" at commit"..." subpath rel"sub"` |
| `archive "https://example/g.tar" digest "blake3:ff"` | `archive url"https://example/g.tar" digest"blake3:..."` |
| `registry main = "https://..." root "ed25519:AAAA"` | `registry main = url"https://..." root key"ed25519:..."` |
| `workspace { members: ["modules/greeting"] }` | `workspace { members: [path"modules/greeting"] }` |
| `host wasm-component ./host/a.wasm` | unchanged; the adapter name is an identifier and may not be quoted |

locators stay syntax in fixed positions of the header. there is no locator value type, so a locator cannot
be computed (0080). the quoted forms are removed.

### git revisions

a git input names exactly one revision kind:

- `at commit"..."` names that commit, and the lock records it.
- `at tag"v3"` is resolved when the lock is written, and the lock records the commit it named. a locked
  read fetches the recorded commit and checks nothing about the tag, as 0070 replays a selection without
  judging it. `pith update` resolves the tag again; a tag that now names a different commit is reported as
  conflicted, naming both commits, and the lock is not moved. the author writes the new commit, or a new
  tag, in the source.
- `at branch"main"` is resolved the same way. a locked read fetches the recorded commit, and `pith update`
  moves the lock to where the branch now points.

a git input with no revision stays refused at parse (E-3060).

### guarded nominals

a guarded nominal names its check:

```
guarded nominal Spdx = Text checked by is_spdx
pure rule is_spdx(t: Text) -> Bool = { ... }
pure rule spdx(t: Text) -> Spdx = { Spdx(t) }
```

the check is a pure rule from the representation to `Bool`. it is part of the declaration, so the
declaration digest covers it.

a guarded value can be constructed only in the declaring project; constructing it anywhere else is refused
at load, naming the type and the project. every construction runs the check, and a failing check fails
evaluation with a diagnostic naming the type. the declaring project publishes the type as an `output type`
and constructor rules such as `spdx` as `output rule`s ([0088](0088-outputs-are-public.md)) for other
projects to call.

the `pith.` types the tags produce are guarded. their checks are pith's own and run at load, and the literal
is how a project obtains one.

a guarded value returned by a host rule or across the component boundary is checked again with the same
check before the engine accepts it, so 0073's return gate does not admit a value the declaring project would
not have constructed.

a value built by an in-process crate is not checked, as for every nominal under 0047.

`guarded` is an attribute of a nominal declaration, as nominal identity is in 0026; the three declaration
forms [0047](0047-the-declaration-table.md) fixes (`nominal`, `sum`, `type`) are unchanged. the word `sealed` is not used because it names an authority mode in 0070.

### command-line inputs

a value input can be given as `--input name=text` exactly when its type has a literal form, and the text is
that literal, read by the same lexer: `--input 'cc=host"/usr/bin/clang"'`. an input of any other type is
refused on the command line, naming its type. this is the rule 0080 left open.

### free text

`Text` remains for text meant for people: messages, descriptions, failure reasons. where a typed value is
expected and raw text must be accepted, the call site wraps it in a named constructor, such as a library's
`raw(Text)`.

## alternatives considered

quoted text checked by position, as today, keeps each value's type in the grammar position it sits in. the
position does not travel with the value: once a string is passed on, a URL and a revision are both `Text`.
objectors in the Swift thread on literals typed by context preferred a visible `#URL(...)`.

open tags, which any library could declare, are the design of Scala's interpolators, JavaScript's tagged
templates and Haskell's quasi-quoters. they bring partial instances (bytestring #140), validators that
cannot be compared (CUE #2432), and collisions between libraries' prefixes. Rust (RFC 3101) reserves
`ident"..."` for the language, and PEP 750 dropped arbitrary prefixes as too complex. a tag added by one input
would also make a file's tokens depend on its inputs, which the [surface
notation](../planning/frontend/surface-notation.md)'s lexing rule forbids.

constructor calls such as `Url("https://...")` would need no new token, but they run at evaluation rather
than at load, and a constructor any project can call does not guarantee anything about the values of the
type.

accepting several spellings and normalizing them, as Nix did with hash encodings inferred from length, gives
one value two spellings and a formatter that rewrites what was typed.

writing a tag and its commit together, as Cargo #13142 and Nix #8226 request, would repeat the lock in the
source. the lock already holds the commit, and `pith update` compares the tag against it.

checking the tag on every locked read would make a replay judge its selection, which 0070 refuses, and
would make the result depend on whether the network is available.

a `Locator` value type would allow computed locators, which 0080's edit refuses because a locator built from
other values could carry them off the machine.

## evidence

to be measured when the slice closes:

- each malformed literal in the set is refused at load with its own code and a span inside the payload
- `pith fmt` prints every literal kind in its one form and the printed file parses to the same header
- a quoted locator, a quoted member and a quoted root key are refused, naming the tagged form
- constructing `pith.Url` outside pith, and a library's guarded nominal outside its project, are refused
- constructing a guarded value whose check fails fails evaluation, naming the type
- a guarded value returned from a host rule that fails its check is refused before the engine records it
- a locked read of a tag input fetches the recorded commit without consulting the tag, and `pith update`
  after the tag moved reports the conflict with both commits and leaves the lock unchanged
- `--input` accepts a `HostPath` in its literal form and refuses one written without the tag
- the existing parity fixtures keep their digests, because the respelling changes no value

## unresolved

a conversion that returns failure as a value, rather than failing evaluation, needs a result type.
`Unchecked<T>` from 0026 is the intended type; nothing produces or consumes it yet, and this would be the
first use.

a changed check changes the declaration digest. whether that makes a new type is left open, as it is for
any declaration change under 0047's coordinate identity.

a `commit"..."` of 40 hex digits excludes git repositories that use SHA-256 object names, whose commits
have 64.

`pith.Platform` has no literal form, so a platform cannot be given with `--input`. whether it gets one, or
the command line accepts `pith.` constants by name, is left to the slice that builds the platform record.

whether `branch"..."` belongs in the first version, or only `commit` and `tag`.
