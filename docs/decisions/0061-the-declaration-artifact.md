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
    - planning-frontend-language
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

source identity and semantic compatibility are different. editing documentation must identify different source bytes without invalidating a consumer whose elaborated declarations did not change. a representation or imported ABI change must invalidate the consumer even when its local rule spelling is unchanged.

## proposed decision

### grammar and names

the declaration grammar has nominal, sum, and alias declarations. a host rule writes its effect category before `rule` and its implementation tier after the interface:

```pi
pure rule compile-entry(source: CSource) -> Object = host
action rule compile(source: CSource) -> Object = host
```

effect category and implementation tier are independent dimensions. a later represented body replaces only `host`; it does not move `pure` or `action`.

identifiers contain ASCII letters, digits, and underscores, and cannot begin with a digit. `-` remains an operator. names containing a hyphen are strings everywhere a declared name is accepted.

### parse, elaborate, and bind

parsing produces `ParsedModule`, which owns the source, syntax, diagnostics, preliminary definitions, and the source artifact identity. elaboration consumes that value, and only an error-free elaboration can construct `LoadedModule`, so partially resolved declarations never reach registration.

`ModuleSource` requires a caller-owned `SourceId`: source allocation is not hidden process state. `LoadedModule` retains both the source `ContentId` and `Arc<SourceFile>`. publishing source bytes is an explicit caller effect.

the loader partitions host rules into `HostRuleDeclaration<Pure>` and `HostRuleDeclaration<Action>`. binding is a method on the typed declaration, so a pure body cannot bind to an action declaration and category is not a runtime boolean. a host rule carries its coordinate and `RuleTier::Host` into the kernel. represented construction remains unavailable until its encoding is decided.

imports are lexically scoped. `ImportEnv` is availability, not visibility: only modules named by `import` enter the private elaboration scope. duplicate imports and qualified access to an undeclared module are errors. imported ABI digests, not source artifacts, cross the semantic module boundary.

declaration references may point forward. a direct self reference elaborates through the existing recursion cut. a cycle among two or more declarations is refused because the current type representation has no wider recursive binding and the corpus supplies no need for one.

duplicate declaration names, rule coordinates across categories, and interfaces within one category are refused. the same interface may have one pure and one action provider because their request types are distinct.

### identities and encodings

`Declaration::encode_canonical` commits to its module, name, kind, and representation. `DeclarationTable::encode_canonical` commits to the module and declarations in name order, independent of registration order. both encodings use the kernel encoding version.

a module ABI manifest contains, in order:

1. module name, declaration-grammar version, and kernel encoding version;
2. declaration digests in declaration-name order;
3. explicitly imported module names and ABI digests in module-name order; and
4. the sorted multiset of provided effect-category and canonical-interface pairs.

> amended by [0067](0067-local-module-workspaces.md): item 3 becomes a sorted, deduplicated set of imported
> subject/ABI pairs. a local binding name is an elaboration input and a tooling sidecar, not a semantic
> fact, so renaming a `use` alias leaves the module ABI unchanged.

declaration names are already committed by their digests and are not repeated beside them. rule labels, source spans, documentation, formatting, and host body revisions do not enter the module ABI. a declaration representation, import ABI, interface, or effect-category change does.

the raw source bytes use the content-blob identity. the ABI uses `ModuleAbiDigest`. custom artifacts use the validated `DigestDomain`, which constructs `pith:<lowercase-hyphenated-name>:v<positive-version>\0`; callers cannot supply arbitrary prefix bytes. phloem uses the same mechanism for all of its structured artifact identities.

### tooling and diagnostics

positions are a non-digested sidecar. it records definition spans, documentation spans, and every reference span with the coordinate as written before alias expansion and the resolved definition. this is sufficient for completion, hover, and go-to-definition without making editor positions semantic inputs.

frontend diagnostics occupy the append-only `E-3001` range through `E-3013`. every diagnostic carries the source and a byte span. recovery must consume input or explicitly synchronize after an error. arbitrary UTF-8 input must terminate without panic.

## alternatives considered

### write the category after the body tier

`rule compile(...) -> Object = action host` groups two independent properties and makes represented migration change the same clause that selects the effect protocol. prefixing the category keeps request typing visible and leaves the body position available for the represented expression.

### resolve every available module

treating `ImportEnv` as scope makes undeclared dependencies compile according to process configuration. it also makes completion and ABI identity depend on modules the source never named. availability is therefore narrowed before elaboration.

### store category as data on one host declaration

a boolean or enum requires binding to perform a runtime category check and admits an avoidable failure path. separate instantiations of `HostRuleDeclaration<K>` make the invalid binding unrepresentable.

### put source positions in the ABI

this makes formatting and documentation edits invalidate semantic consumers. the source artifact already identifies those bytes; positions stay attached to it in the sidecar.

### permit general declaration cycles

mutual recursive declarations need a binding representation and canonical encoding that the kernel does not have. inferring one in the loader would make elaborated identity depend on an unrecorded encoding. only the existing direct recursion cut is admitted.

## consequences

the loader is the module-linkage boundary. the kernel owns canonical declared types, typed rules, and digest primitives, but it does not resolve source imports.

host binding remains explicit rust code, but the declaration being bound owns its coordinate, interface, tier, and effect category. a source edit can move the source artifact while leaving the ABI and all rule revisions unchanged. a semantic declaration edit moves the ABI and the revisions of rules whose interfaces reach it.

the grammar is intentionally smaller than the eventual language. there are no represented expressions, general recursion groups, generics, or language-server process in M-10.

## prototype evidence

the four first-party `.pi` surfaces elaborate to declaration tables whose digests equal their live rust tables. xylem's nine typed host declarations derive the same rule revisions as its live registrations. example-domain binds its real pure and action implementations through the typed declarations and passes its contract tests.

golden tests fix the declaration and table bytes and declaration digest. loader tests distinguish source-artifact edits from ABI edits, exercise scoped imports, duplicates, direct and mutual recursion, typed binding, documentation and alias positions, and run a property test over arbitrary UTF-8 input.

## unresolved

M-11 owns represented-body construction and encoding. M-12 owns the graph-resident elaborator and measures whether the ABI cutoff prevents downstream body invalidation. M-13 owns the complete expression notation and formatter.
