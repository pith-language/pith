---
schema: design-doc/v1
id: decision-0066-frontend-diagnostics-carry-source-identity
title: a frontend diagnostic value names the source content that owns its span
summary: diagnostics returned by graph-tier frontend rules carry the input source blob identity beside their stable code, message, and byte span, allowing a multi-file caller to attach the right text without making source presentation part of an engine diagnostic
kind: decision
status: proposed
created: 2026-08-28
updated: 2026-08-28
tags:
  - diagnostics
  - language
  - graph
  - identity
relations:
  informed_by:
    - planning-frontend-architecture
  depends_on:
    - decision-0023-rule-and-cache-identity
    - decision-0053-parse-diagnostics-carry-their-source
    - decision-0063-the-frontend-graph-tier
  amends:
    - decision-0053-parse-diagnostics-carry-their-source
  supersedes: []
---

# a frontend diagnostic value names the source content that owns its span

> amends [0053](0053-parse-diagnostics-carry-their-source.md). source text remains context and is still
> absent from durable engine diagnostics, but a diagnostic returned *as a frontend computation value* must
> identify which source input owns its byte span. a content identity is the durable, process-independent
> form of that association; a `SourceId` is only a parse-session index.

## context

0053 attached an `Arc<SourceFile>` while a parser held the text and deliberately dropped it from durable
diagnostics. that was correct for a lock parse with one obvious source and for engine failures with no
source. the graph frontend changes the cardinality: one `Source` value can contain several path-sorted
source blobs, and imported interface blobs can contribute diagnostics too. returning only `(code, message,
span)` makes the span ambiguous as soon as two files participate.

the graph rules also treat invalid source as a completed, reusable value. therefore its diagnostic is data
inside the computation result, not the `DiagnosticSink` that marks an engine attempt failed. it needs a
portable reference to an input, not the source text copied into every result.

## decision

the frontend diagnostic value is a closed record containing stable code, message, source `ContentId`, and
start/end byte offsets. the source field names one blob in the canonical `FrontendSource` input. the driver
or language server may resolve that identity to the path and text it already owns, attach a `SourceFile`,
and render the same miette snippet 0053 established.

parser and elaborator diagnostics attach their source at the production site through the file set. when a
diagnostic becomes a graph result, projection replaces the process-local attachment with the corresponding
blob identity. diagnostics whose source cannot be associated are not fabricated into a file; the graph
projection includes only source-bound frontend diagnostics, while internal engine failures continue down
the failing-attempt channel.

the source identity is context carried by the result; it does not enter a diagnostic-specific digest or
stable code. it already participates in the frontend computation key as part of `FrontendSource`, so the
record does not create a second invalidation axis. identical bytes at two paths share content identity;
the caller uses the source input's path mapping to choose presentation, while semantic diagnostics remain
about the bytes.

## alternatives considered

### persist `SourceId`

rejected because IDs are assigned while a file set is assembled and have meaning only in that process.
hydration in another process cannot recover which bytes an integer referred to.

### copy source text or paths into every diagnostic value

rejected because text duplicates an input blob and paths are layout, not source identity. both enlarge
reusable results with data the caller already has; a path also gives identical bytes different diagnostic
semantics.

### keep the source implicit

rejected by the multi-file case. a span without the source whose byte offsets it measures is not structured
context; it is two integers a reader has to guess how to use.

## evidence

the graph-tier user-error test evaluates invalid source as a successful frontend computation and asserts
that its first diagnostic carries the broken input blob beside the unknown-name code and exact span. parser
and elaborator construction routes through source-aware file helpers, so parse, name, type, body, import,
and duplicate-definition refusals retain the source that produced them. this closes the sentence 0053 left
for “the surface language” with a process-independent representation suitable for hydration.

## unresolved

the diagnostic value names content rather than a preferred path. if the same blob is present under more
than one path, choosing which alias an editor displays belongs to the source-set adapter and is not decided
here. durable engine diagnostics still carry no source label, exactly as 0053 records; this amendment is
only for diagnostics that are frontend result data.

