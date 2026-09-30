---
schema: design-doc/v1
id: research-typed-literals
title: typed literals
summary: how Bazel, Buck2, Nix, Cargo, Terraform, CMake, Scala, Haskell, C++, JavaScript, Python, Swift, Rust, Dhall, Pkl, CUE and YAML give a literal a type other than text, what went wrong where the type stayed inside a string, and whether the set of literal kinds should be closed
kind: research
status: researching
evidence: preliminary
created: 2026-09-30
updated: 2026-09-30
tags:
  - research
  - language
  - types
relations:
  informed_by:
    - research-project-declaration
    - research-run-and-test
  depends_on:
    - research-method
  supersedes: []
---

# typed literals

pith writes many values as quoted text that means something narrower than text: a locator, a revision, a
digest, a public key, a host path, a program argument. [0081](../decisions/0081-paths-are-values.md) gave
three of them a type and a literal (`./x`, `rel"..."`, `host"..."`) and left open how the rest of the
string fields become typed. this note reads how other systems give a literal a type, and what happened
where the type stayed inside a string. sources were read on 2026-09-30.

## Bazel and Buck2

in a legacy macro a label string is resolved where the macro is called, not where it is defined: label
strings "are interpreted relative to the BUILD file in which the macro is used rather than relative to the
.bzl file", and the fix is to "wrap the label strings with the Label constructor" so the label "will
resolve to the correct target even if the canonical name ... should be different in the main repo, such as
due to repo mappings" (legacy macros documentation). bzlmod made this sharper by giving every repository a
canonical name and a per-repository apparent name, so one string means different targets in different
repositories. issue #15593 (2022-05-30) added the `@@repo//pkg:target` spelling for labels that have
passed through repo mapping. the `Label` documentation promises `Label(str(l)) == l` wherever the call
occurs, for the canonical form; the form `print` and `fail` show is a different, readable one.

users parsed canonical names as strings anyway. #23127 (2024-07-26) changed the separator from `~` to `+`
because `~` was slow on Windows (#22865), noting that "people should have already not been depending on the
canonical repo name format, as it was declared to be unstable". Bazel 8's symbolic macros convert string
arguments to labels at the call site through typed attributes.

Buck2 types command lines. `cmd_args` is "a mutable collection of strings and artifact values"; the rule
authors' guide says "a command line is both a list of string arguments and a list of artifacts they depend
on". `.as_output()` marks an output, `hidden` adds a dependency that does not appear on the line, and the
ways around the type are named: `ignore_artifacts`, `relative_to`, `absolute_prefix`, `format`.

## Nix

a Nix string is "a pair of a sequence of characters and a string context"; the context lets a string carry
store paths so users can "reference other files ... without manually keeping track of the exact paths".
the type lives inside the text, and the way out is `builtins.unsafeDiscardStringContext`, after which the
string "has lost" its dependencies.

path values are a separate type, but interpolating one copies it: `"${./.}"` "copies the whole directory to
the Nix store on evaluation", and `src = ./.` "makes the derivation dependent on the name of the current
directory" (nix.dev, working with local files). in flakes, "all files are added to the store twice"
(#9428, 2023-11-21).

platforms have three representations in nixpkgs: `system`, a two-part shorthand such as `aarch64-darwin`;
`config`, an LLVM or GNU triple; and `parsed`, a structured value of whitelisted components. the manual says
the schema "is a bit ill-defined due to a long and convoluted evolution" and of `config` that it "needs a
better name". `lib/systems/parse.nix` reproduces GNU quirks so that `i386-linuxabichickenlips` parses, and a
three-part triple is ambiguous between cpu-kernel-env and cpu-vendor-os.

hashes were written `sha256 = "..."` in hex or in "nix32", whose alphabet "is not documented nor
standardized in any way" (NixOS wiki), with the encoding inferred from the length. Nix 2.2 (release notes,
2019-01-11) added SRI hashes, "allowing the hash algorithm and hash to be specified in a single string".
nixpkgs PR #89308 (2020-06-01) removed SRI hashes again because "these break compatibility with nix 2.0",
and fetchgit gained SRI through PR #79987, opened 2020-02-13 and merged in 2022.

git revisions have two attributes. `ref` is prefixed with `refs/heads/` unless it starts with `refs/`, and
`rev` must be a full 40-digit SHA-1. a flake URL `github:o/r/<rev-or-ref>` decides which from the shape of
one path segment. #8226 (2023-04-17) asks to write both and have Nix check that the ref still points at the
rev; #9667 (2023-12-27) was a regression where `ref` was ignored when `rev` was set; #12974 (2025-04-08,
open) describes a ref left stale after its rev was updated.

## Cargo

a git dependency takes one of `branch`, `tag` or `rev`, and `rev` covers the rest: "anything that is not a
branch or a tag falls under rev key", including `refs/pull/493/head`. PR #8984 (2020-12-16) turned four years
of warnings about giving more than one into an error, "dependency specification is ambiguous". #13142 asks
to allow a tag and a rev together, the request Nix #8226 makes. when PR #8364 changed what no key means,
from `master` to the remote's `HEAD`, Alex Crichton wrote (internals, 2020-07-16) that "if you use branch =
"master" ... the lock file is no longer compatible" and that the problems were "enough that I believe we
need to revert this change".

## Terraform

before 0.12, "Terraform v0.11 and earlier allowed expressions only within interpolation sequences, like
"${var.example}"", and variables were "documented as accepting only strings, lists of strings, and maps of
strings" (0.12 upgrade guide). 0.12 gave expressions and module inputs real types; `"1" == 1`, true before,
became false. the migration ran through `terraform 0.12upgrade`, which "will perform rewrites like these
automatically" and marked the cases it could not decide with `TF-UPGRADE-TODO`.

## CMake

"every object is a string", and lists are strings joined by `;`; "most commands that construct lists do
not escape ; characters in list elements, thus flattening nested lists" (cmake-language manual). policy
CMP0054, "only interpret if() arguments as variables or keywords when unquoted", exists because
`if("${ANIMAL}" STREQUAL "MONKEY")` dereferenced the quoted value a second time. its old behaviour was
removed in CMake 4.0.

## literals validated before running

Scala's string interpolators turn `x"..."` into a call on `StringContext`, and any library can add a
prefix. typelevel's literally builds "compile time validation of literal values built from strings" on
them: `port"100000"` fails to compile with "invalid port - must be integer between 0 and 65535".

Haskell's `OverloadedStrings` gives a string literal any type with an `IsString` instance, and the instance
cannot fail at compile time. bytestring #140 (2017-10-13, open): the instance for `ByteString` "silently
truncates the bytes", so a literal ending in a CJK character loses the high bits of that character. a
quasi-quoter moves the check to compile time; modern-uri's `[uri|...|]` "cannot fail because when there is
an invalid URI inside the quote it's a compilation error".

in C++ a user-defined literal suffix "must begin with the underscore": suffixes without it are reserved for
the standard library (cppreference). the language splits one namespace into a closed half and an open one.

JavaScript's tagged templates are open and run at run time. the template literal revision fixed a case where
the language's escape grammar collided with a tag's payload: `\unicode` inside a LaTeX tag was a syntax
error, and now the cooked value is undefined while `.raw` keeps the text.

Python's PEP 750 (accepted 2025-04-10) first proposed arbitrary prefixes such as `html"..."` and dropped
them as "too complex to build in full generality", keeping one prefix, `t`, that returns a template.

Swift's regex literals (SE-0354) are checked at compile time and type their captures. a URL macro thread
(2023-03-17) made `#URL("not a url")` a compile error. a pitch for macro literal protocols (2023-08-23)
states the problem, "if your type is ExpressibleByStringLiteral but only some string literals are valid,
your only choice is to trap at runtime", and objectors preferred a visible `#URL(...)` to literals typed
silently by context.

Rust reserves `ident"..."` and `ident#...` "for exclusive use by the language" (RFC 3101, edition 2021).
user-defined literals were discussed ("custom literals via traits", 2018-07-24) and not adopted.

## Dhall, Pkl and CUE

Dhall's imports are a closed set of forms, local paths, `https://`, `env:` and `missing`, resolved before
type checking; `as Location` gives the import's location as a union value. `sha256:` hashes the normalized
expression, so a hash "does not change when we make cosmetic changes", and the algorithm prefix leaves "the
door open for new hash algorithms" (Gonzalez, 2017-11-03).

Pkl has unit literals with a closed set of units: `5.min`, `30.5.mb`.

CUE writes string constraints as lattice values or as builtin validators such as `net.IPv4`. #2432
(2023-06-06) notes that validators are not lattice members, so two contradictory prefix validators do not
reduce to bottom: an opaque validator function cannot be compared with another.

## YAML

YAML 1.1 reads `y`, `yes`, `no`, `on` and `off` as booleans. StrictYAML documents the results: `- NO` read
as `False`, `9.3` as a float, the surname `Null` as a null. its fix is that "all scalars default to strings
unless the schema explicitly specifies otherwise". YAML 1.2 narrowed booleans to `true` and `false`, and
widely used parsers still implement 1.1.

## target triples

LLVM normalizes `aarch64-none-elf` to `aarch64-none-unknown-elf`, although "it's the OS which is known to be
'none' in this case, not the vendor" (baremetal normalization RFC, 2024-04-23). Rust's compiler MCP #850
(2025-03-13) says target names "have slowly diverged ... and are at this point mostly an arbitrary mapping",
and the target tier policy calls renaming "highly disruptive". Zig put CPU features into the triple string
(#4584, 2020-02-29) and later proposed a longer grammar for it because "GNU triples simply cannot represent
most of the ABI nuance" (#20690, 2024-07-20).

## where they differ

systems that let a type ride inside a string had to add a way to take it out and a rule for when it leaks:
Nix string context and its discard function, Bazel label strings in macros, Nix path interpolation copying
the tree. Bazel's `Label` and Terraform 0.12 gave the value its own type after years of workarounds, and
Terraform could do it because a tool rewrote every configuration mechanically. Buck2 typed its command
lines with `cmd_args` from the start.

one field holding several kinds of thing turned into a request to separate them: Cargo's `rev`, the flake
URL's `<rev-or-ref>`, and Nix's hash encoding inferred from length. the requests to write a tag and a
revision together (Cargo #13142, Nix #8226) ask for a check that the two agree.

open literal prefixes (Scala, JavaScript, Haskell quasi-quoters, C++'s underscore half) let any library add
a validator, and one of them produced an instance that truncates its input (bytestring #140). CUE's builtin
validators, which are not prefixes, show a related cost: two validators cannot be compared (CUE #2432).
closed sets are the choice of Rust RFC 3101, PEP 750's single prefix, Pkl's units, Dhall's import forms and
C++'s reserved half. of these, only PEP 750 states a reason, that arbitrary prefixes were "too complex to
build in full generality".

checks at compile or load time (literally, modern-uri, Swift regex, Dhall imports) report the literal that
is wrong; checks at run time (`IsString`, tagged templates, Nix's revision length) report later or never.
the objectors in the Swift discussion preferred a type visible in the literal to one inferred from where the
literal sits.

Bazel's `Label(str(l)) == l` and Dhall's hash of the normal form give each value one printed form.
inferring an encoding from the text, as Nix did with hash length and flake URLs do with revision shape, is
where two spellings of one value began.

the escape hatches read here are named (`unsafeDiscardStringContext`, `ignore_artifacts`, `.raw`). where the
untyped form was the default instead (Cargo's `rev`, YAML plain scalars, Terraform 0.11 interpolation), the
ambiguity stayed until a later version or policy narrowed it.

## result for this project

pith requires a value that is not free text to have its type visible in its literal and to be checked
before evaluation. the decision is [0085](../decisions/0085-typed-literals.md).

the set of literal kinds is closed and fixed per language version, as builtins are under
[0084](../decisions/0084-builtins-and-machine-facts.md), following Rust's reservation and PEP 750's
narrowing. the lexer reserves every `ident"..."` prefix, so a tag added later cannot change what an existing
file means. a library that wants the same check declares a nominal only its own project can construct,
with a check its declaration names.

each literal has one printed form, and a literal in another form is refused, not normalized, so no value has
two spellings and tools can rewrite files without evaluating them. the payload of a digest or a key is the
spelling pith already writes in locks and registries.

a git input names one revision kind, `commit`, `tag` or `branch`, and the lock records the commit. the
check that a tag still names its commit, which Cargo #13142 and Nix #8226 ask to write by hand, is made by
`pith update` against the lock.

program arguments and environments, which mix text with paths, become sums rather than strings carrying
hidden context, as Buck2's `cmd_args` does. free text remains for text meant for people, and the
way to pass raw text where a typed value is expected is a named constructor visible at the call site.

platforms stay a record of declared sums. the triple a particular tool expects is produced at that tool's
boundary by the library that runs it.

## sources

- Bazel legacy macros: https://bazel.build/extending/legacy-macros; external repositories:
  https://bazel.build/external/overview; `Label`: https://bazel.build/rules/lib/builtins/Label; symbolic
  macros: https://bazel.build/extending/macros; issues #15593, #22865, #23127:
  https://github.com/bazelbuild/bazel
- Buck2 rule authors: https://buck2.build/docs/rule_authors/writing_rules/; `cmd_args`:
  https://buck2.build/docs/api/build/cmd_args/
- Nix string context: https://nix.dev/manual/nix/2.34/language/string-context.html; local files:
  https://nix.dev/tutorials/working-with-local-files.html; nix #8226, #9428, #9667, #12974:
  https://github.com/NixOS/nix; Nix 2.2 release notes:
  https://nix.dev/manual/nix/2.28/release-notes/rl-2.2.html; nixpkgs PR #79987 and #89308:
  https://github.com/NixOS/nixpkgs; Nix hash: https://wiki.nixos.org/wiki/Nix_Hash; cross compilation:
  https://ryantm.github.io/nixpkgs/stdenv/cross-compilation/; `lib/systems/parse.nix`:
  https://github.com/NixOS/nixpkgs/blob/master/lib/systems/parse.nix; rev and ref discussion:
  https://discourse.nixos.org/t/rev-and-ref-attributes-in-builtins-fetchgit-and-maybe-flakes-too/20588
- Cargo dependencies: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html; PR #8984 and
  issue #13142: https://github.com/rust-lang/cargo; default branch discussion:
  https://internals.rust-lang.org/t/evaluating-cargos-change-in-handling-default-git-branches/12748
- Terraform 0.12 upgrade guide: https://developer.hashicorp.com/terraform/language/v1.1.x/upgrade-guides/0-12
- CMake language: https://cmake.org/cmake/help/latest/manual/cmake-language.7.html; CMP0054:
  https://cmake.org/cmake/help/latest/policy/CMP0054.html
- literally: https://github.com/typelevel/literally
- bytestring #140: https://github.com/haskell/bytestring/issues/140; quasi-quoted literals:
  https://harry.garrood.me/blog/qq-literals/; modern-uri: https://hackage.haskell.org/package/modern-uri
- C++ user-defined literals: https://en.cppreference.com/w/cpp/language/user_literal
- template literal revision: https://tc39.es/proposal-template-literal-revision/
- PEP 750: https://peps.python.org/pep-0750/
- SE-0354: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0354-regex-literals.md; URL
  macro: https://forums.swift.org/t/url-macro/63772; macro literal protocols:
  https://forums.swift.org/t/pitch-macro-literal-protocols/66915
- Rust RFC 3101: https://rust-lang.github.io/rfcs/3101-reserved_prefixes.html; custom literals:
  https://internals.rust-lang.org/t/pre-rfc-custom-literals-via-traits/8050
- Dhall imports: https://github.com/dhall-lang/dhall-lang/blob/master/standard/imports.md; Gonzalez, 2017:
  https://www.haskellforall.com/2017/11/semantic-integrity-checks-are-next.html
- Pkl language reference: https://pkl-lang.org/main/current/language-reference/index.html
- CUE #2432: https://github.com/cue-lang/cue/issues/2432
- YAML 1.1 bool: https://yaml.org/type/bool.html; StrictYAML:
  https://hitchdev.com/strictyaml/why/implicit-typing-removed/
- LLVM baremetal triples: https://discourse.llvm.org/t/rfc-baremetal-target-triple-normalization/78524;
  Rust MCP #850: https://github.com/rust-lang/compiler-team/issues/850; target tier policy:
  https://doc.rust-lang.org/stable/rustc/target-tier-policy.html; Zig #4584 and #20690:
  https://github.com/ziglang/zig
