---
schema: design-doc/v1
title: diagnostic spans
summary: where four systems keep the source text a diagnostic points into — rustc's session-wide SourceMap, cpplib's translation-unit line counter, miette's late attachment at the render boundary, and LSP's uri the editor resolves — and when line and column are computed
id: research-diagnostic-spans
kind: research
status: researching
evidence: preliminary
created: 2026-08-10
updated: 2026-08-10
tags:
  - research
  - diagnostics
  - tooling
relations:
  informed_by:
    - research-tooling
  depends_on:
    - research-method
  supersedes: []
---

# diagnostic spans

a diagnostic that says what went wrong still has to say where, and "where" forces two questions the systems that ship diagnostics answer differently: does the diagnostic carry the text it points into, and is its position an offset or a line and a column? the answers decide who can render a snippet, what crosses a process boundary, and whether a diagnostic's identity survives being shown in a terminal, an editor, or an api.

this note reads the primary documents of four systems at those two questions: the rustc dev guide's span and diagnostics chapters, cpplib's internals manual, miette's api documentation, and the language server protocol specification.

## rustc: the span is an index, the text lives in a session-wide table

the rustc dev guide opens with the representation: "Span is the primary data structure in rustc used to represent a location in the code being compiled." a span is not printable on its own; it "can be looked up in a SourceMap to get a 'snippet'" through methods like `span_to_snippet`, which returns the covered text or fails when the text is unavailable — macro-generated spans can refuse. the path, line, and column are computed at emission, not stored: a rendered diagnostic includes "The path, line number and column of the beginning of the primary span."

two style rules in the same chapter carry the position's semantics. the message should be "general and able to stand on its own, so that it can make sense even in isolation," and the spans should be minimized — "reduce the span to the smallest amount possible" — with primary labels allowed to be more succinct than the message because they point at the code. the diagnostic's identity is its code and message; the span is where it points.

the design rests on an assumption the guide does not state: the SourceMap lives as long as the compilation session and holds every file the compiler read. a span is an index into that table, and rendering it means the table is still there.

## cpplib: one integer per line, resolved by a separate line table

cpplib's tokens carry position eagerly, but compressed to a counter: "The cpp_token structure contains line and col members. The lexer fills these in with the line and column of the first character of the token." the line member is not a file line — "this number is not the number of the line in the source file, but instead bears more resemblance to the number of the line in the translation unit," maintained as "a monotonic increasing line count, which is incremented at every new line character."

mapping the counter back to a file and line is deferred to a separate component: "it is straight forward to map from this to the original source file and line number pair," and the mapping lives in `line-map.cc` and `line-map.h`. the stated motive is size — the integer is small, which matters "whenever line number information needs to be saved." this is rustc's shape at a different granularity: a compact position whose resolution needs a table the producer also owns.

## miette: the diagnostic carries offsets, the text is attached later, by whoever renders

miette's `Diagnostic` trait separates the two halves explicitly. the span type is "a basic byte-offset and length into an associated SourceCode," and the source is its own concept: `SourceCode` "Represents readable source code of some sort," with the trait documentation naming the intended breadth — "simple `SourceCode` types like `String`s, as well as more involved types like indexes into centralized `SourceMap`-like types, file handles, and even network streams."

attachment is deliberately late: "Sometimes it makes sense to add source code to the error message later. One option is to use with_source_code() method for that." an inner error carries labels and no text; the boundary that renders attaches the text once — miette's own pattern for a parse library whose errors must survive without the file. naming is a wrapper, not a field: `NamedSource` is a "Utility struct for when you have a regular SourceCode type that doesn't implement name." the handlers are swappable over the same diagnostic — the graphical terminal handler, a narratable one for screen readers, and a json one — which is the render-independence requirement stated as an api.

the diagnostic and its text therefore have different lifetimes by design, and the attachment can be forgotten: a report rendered without a source prints the message and labels but no snippet.

## LSP: the diagnostic carries a range and a uri, and never the text

the protocol's Diagnostic carries "The range at which the message applies" and a message, with severity, code, source, tags, and `relatedInformation` beside them. the document the range indexes is named by the enclosing notification's uri; the text itself is nowhere in the structure. the editor owns the text, has it open, and resolves the range when it draws the squiggle.

the position encoding is negotiated, and the mandatory baseline is neither bytes nor characters: "To stay backwards compatible the only mandatory encoding is UTF-16 represented via the string utf-16," with utf-8 and utf-32 available when both sides announce them. a server cannot emit a range without knowing which encoding the client will apply to it, so the position is eagerly computed in a coordinate system the protocol fixes.

## what the disagreement is

the four systems disagree on who holds the text when the diagnostic is constructed. rustc and cpplib hold it in a producer-owned table that the position indexes — the SourceMap and the line table — and both assume the table outlives anything that might render. miette holds it nowhere until a renderer attaches it, keeping the parse library free of file ownership while accepting that a forgetful boundary renders no snippet. LSP holds it on the other side of the wire entirely: the diagnostic names a document the client already has.

they disagree secondarily on when position becomes line and column. rustc and cpplib store compact positions and resolve them at render through their tables. miette stores byte offsets and computes lines and columns inside `read_span`. LSP forces the server to compute them before sending, in negotiated units.

none of the four puts the text inside the diagnostic. the closest approach is miette's, and even there the text rides beside the diagnostic as an attachment. the reasons each gives are consistent: text is heavy, may be unavailable, and one file's diagnostics are few while one diagnostic's renderers are many.

## questions for this project

pith's diagnostics cross a process boundary the compiler designs do not consider: a durable record in sqlite outlives the process that parsed, and a hydrated diagnostic renders where no parse ever ran. a session table indexed by an id would strand the position at the boundary; the label-and-span a parse site can attach travels with the diagnostic and survives the store as structure. the producer-attaches position is miette's, taken one step earlier — the parse holds the text, so the parse attaches — and the offset position is rustc's and miette's, resolved to line and column only at render, which 0021 already chose for the span representation.

## sources

- [rustc dev guide: spans and diagnostics](https://rustc-dev-guide.rust-lang.org/diagnostics.html)
- [cpplib internals: line numbering](https://gcc.gnu.org/onlinedocs/cppinternals/Line-Numbering.html)
- [miette api documentation](https://docs.rs/miette/latest/miette/)
- [Language Server Protocol 3.17 specification: Diagnostic](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
