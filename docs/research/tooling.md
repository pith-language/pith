---
schema: design-doc/v1
id: research-tooling
title: tooling and inspectability
summary: early research into editor protocols, incremental parsing, embedded languages, and graph-aware project tooling
kind: research
status: researching
evidence: preliminary
created: 2026-03-09
updated: 2026-03-09
tags:
  - research
  - tooling
relations:
  informed_by: []
  depends_on:
    - research-method
    - research-build-systems
  supersedes: []
---

# tooling and inspectability

tooling should consume the same parser, types, graph, diagnostics, and provenance as evaluation. building a separate editor approximation would repeat the information loss the project is trying to avoid.

## language tooling

the Language Server Protocol separates language-specific analysis from editor integration. one server can provide completion, navigation, references, edits, hover information, and diagnostics to several editors.

LSP is a transport and interaction protocol. it should not become the canonical semantic API. command-line tools, policy checks, documentation generators, and tests also need direct structured access to the compiler and graph.

Tree-sitter provides incremental concrete syntax trees that remain useful while a file contains errors. its injection queries can identify source ranges written in another language and parse them with another grammar.

that is relevant if build actions or configuration values contain shell, SQL, regular expressions, or another embedded language. syntax injection can improve editing without pretending a string has acquired a typed contract. typed embedded code needs a library-level value and validator in addition to highlighting.

## graph tooling

Buck2's BXL exposes graph queries and orchestration through typed Starlark values instead of asking tools to parse terminal output. Buck2 also ships Starlark tooling for language-server, debugger, lint, and typechecking workflows.

the project should expose values, rule selection, provenance, invalidation, capabilities, and plans through one versioned query API. the CLI is one client.

## generated views

documentation, editor hints, JSON output, plan rendering, and graph visualizations are derived views. they should retain stable semantic IDs so a diagnostic or graph node can be followed across tools.

human-readable rendering remains important. a structured error that requires a custom UI to understand is still a poor error.

## questions

- does the main parser need error recovery comparable to Tree-sitter, or can a separate concrete parser share syntax definitions safely?
- how are generated files and cross-repository symbols indexed?
- can embedded-language types provide completions from declared tool schemas?
- what part of the dynamic dependency graph is available before a request is evaluated?
- how does an editor show provenance and overrides without turning every value into a wall of metadata?
- which query schemas need compatibility guarantees before version one?

## sources

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [Tree-sitter](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter syntax injection](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection)
- [Buck2 BXL](https://buck2.build/docs/bxl/)
- [Buck2 Starlark development](https://buck2.build/docs/developers/starlark/)

