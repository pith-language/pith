---
schema: design-doc/v1
id: decision-0061-the-declaration-artifact
title: a source artifact elaborates into a typed host-binding surface and a semantic ABI
summary: pi declaration text has explicit effect categories, scoped imports, phase-typed host bindings, a semantic ABI, and a non-digested position sidecar
kind: decision
status: proposed
created: 2026-08-24
updated: 2026-08-24
tags:
  - language
  - declarations
  - modules
  - tooling
  - identity
relations:
  informed_by:
    - decision-0047-the-declaration-table
    - planning-language-frontend
  depends_on:
    - decision-0018-termination-and-recursion
    - decision-0023-rule-and-cache-identity
    - decision-0038-represented-rule-bodies
    - decision-0048-pre-release-version-pinning
    - decision-0053-parse-diagnostics-carry-their-source
  amends:
    - decision-0018-termination-and-recursion
  supersedes: []
---

# a source artifact elaborates into a typed host-binding surface and a semantic ABI

> amends 0018: the host escape hatch is written at the rule declaration, not at a call site. module and interface linkage belongs to the loader; the kernel receives elaborated declarations and rules.

## context

0047 gives the kernel canonical declarations, but not a source artifact, import scope, host binding lifecycle, or tooling positions. 0038 requires the implementation tier to be visible at the declaration site while keeping host and represented bodies on one rule interface. M-10 needs that boundary before represented bodies exist.

Source identity and semantic compatibility are different. Editing documentation must identify different source bytes without invalidating a consumer whose elaborated declarations did not change. Conversely, a representation or imported ABI change must invalidate the consumer even if its local rule spelling is unchanged.

## proposed decision

### grammar and names

The declaration grammar has nominal, sum, and alias declarations. A host rule writes its effect category before `rule` and its implementation tier after the interface:

```pi
pure rule compile-entry(source: CSource) -> Object = host
action rule compile(source: CSource) -> Object = host
```

Effect category and implementation tier are independent dimensions. A later represented body replaces only `host`; it does not move `pure` or `action`.

Identifiers contain ASCII letters, digits, and underscores, and cannot begin with a digit. `-` remains an operator. Names containing a hyphen are strings everywhere a declared name is accepted.

### parse, elaborate, and bind

Parsing produces `ParsedModule`, which owns the source, syntax, diagnostics, preliminary definitions, and the source artifact identity. Elaboration consumes that value. Only an error-free elaboration can construct `LoadedModule`; partially resolved declarations cannot reach registration.

`ModuleSource` requires a caller-owned `SourceId`. Source allocation is not hidden process state. `LoadedModule` retains both the source `ContentId` and `Arc<SourceFile>`. Publishing source bytes is an explicit caller effect.

The loader partitions host rules into `HostRuleDeclaration<Pure>` and `HostRuleDeclaration<Action>`. Binding is a method on the typed declaration, so a pure body cannot bind to an action declaration and category is not a runtime boolean. A host rule carries its coordinate and `RuleTier::Host` into the kernel. Represented construction remains unavailable until its encoding is decided.

Imports are lexically scoped. `ImportEnv` is availability, not visibility: only modules named by `import` enter the private elaboration scope. Duplicate imports and qualified access to an undeclared module are errors. Imported ABI digests, not source artifacts, cross the semantic module boundary.

Declaration references may point forward. A direct self reference elaborates through the existing recursion cut. A cycle among two or more declarations is refused because the current type representation has no wider recursive binding and the corpus supplies no need for one.

Duplicate declaration names, rule coordinates across categories, and interfaces within one category are refused. The same interface may have one pure and one action provider because their request types are distinct.

### identities and encodings

`Declaration::encode_canonical` commits to its module, name, kind, and representation. `DeclarationTable::encode_canonical` commits to the module and declarations in name order, independent of registration order. Both encodings use the kernel encoding version.

A module ABI manifest contains, in order:

1. module name, declaration-grammar version, and kernel encoding version;
2. declaration digests in declaration-name order;
3. explicitly imported module names and ABI digests in module-name order; and
4. the sorted multiset of provided effect-category and canonical-interface pairs.

Declaration names are already committed by their digests and are not repeated beside them. Rule labels, source spans, documentation, formatting, and host body revisions do not enter the module ABI. A declaration representation, import ABI, interface, or effect-category change does.

The raw source bytes use the content-blob identity. The ABI uses `ModuleAbiDigest`. Custom artifacts use the validated `DigestDomain`, which constructs `pith:<lowercase-hyphenated-name>:v<positive-version>\0`; callers cannot supply arbitrary prefix bytes. Phloem uses the same mechanism for all of its structured artifact identities.

### tooling and diagnostics

Positions are a non-digested sidecar. It records definition spans, documentation spans, and every reference span with the coordinate as written before alias expansion and the resolved definition. This is sufficient for completion, hover, and go-to-definition without making editor positions semantic inputs.

Frontend diagnostics occupy the append-only `E-3001` range through `E-3013`. Every diagnostic carries the source and a byte span. Recovery must consume input or explicitly synchronize after an error. Arbitrary UTF-8 input must terminate without panic.

## alternatives considered

### write the category after the body tier

`rule compile(...) -> Object = action host` groups two independent properties and makes represented migration change the same clause that selects the effect protocol. Prefixing the category keeps request typing visible and leaves the body position available for the represented expression.

### resolve every available module

Treating `ImportEnv` as scope makes undeclared dependencies compile according to process configuration. It also makes completion and ABI identity depend on modules the source never named. Availability is therefore narrowed before elaboration.

### store category as data on one host declaration

A boolean or enum requires binding to perform a runtime category check and admits an avoidable failure path. Separate instantiations of `HostRuleDeclaration<K>` make the invalid binding unrepresentable.

### put source positions in the ABI

This makes formatting and documentation edits invalidate semantic consumers. The source artifact already identifies those bytes; positions stay attached to it in the sidecar.

### permit general declaration cycles

Mutual recursive declarations need a binding representation and canonical encoding that the kernel does not have. Inferring one in the loader would make elaborated identity depend on an unrecorded encoding. Only the existing direct recursion cut is admitted.

## consequences

The loader is the module-linkage boundary. The kernel owns canonical declared types, typed rules, and digest primitives, but it does not resolve source imports.

Host binding remains explicit Rust code, but the declaration being bound owns its coordinate, interface, tier, and effect category. A source edit can move the source artifact while leaving the ABI and all rule revisions unchanged. A semantic declaration edit moves the ABI and the revisions of rules whose interfaces reach it.

The grammar is intentionally smaller than the eventual language. There are no represented expressions, general recursion groups, generics, or language-server process in M-10.

## prototype evidence

The four first-party `.pi` surfaces elaborate to declaration tables whose digests equal their live Rust tables. Xylem's nine typed host declarations derive the same rule revisions as its live registrations. Example-domain binds its real pure and action implementations through the typed declarations and passes its contract tests.

Golden tests fix the declaration and table bytes and declaration digest. Loader tests distinguish source-artifact edits from ABI edits, exercise scoped imports, duplicates, direct and mutual recursion, typed binding, documentation and alias positions, and run a property test over arbitrary UTF-8 input.

## unresolved

M-11 owns represented-body construction and encoding. M-12 owns the graph-resident elaborator and measures whether the ABI cutoff prevents downstream body invalidation. M-13 owns the complete expression notation and formatter.
