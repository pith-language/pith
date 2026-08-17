---
schema: design-doc/v1
title: a parsed text's diagnostics carry their source
summary: a diagnostic produced while parsing text attaches the file it was produced from — label, text, and byte spans selecting the offending field or line — so position is structure the renderer reads rather than prose the producer wrote; the durable record keeps the span and drops the source, which keeps a hydrated diagnostic's identity unchanged and its render honest
id: decision-0053-parse-diagnostics-carry-their-source
kind: decision
status: proposed
created: 2026-08-10
updated: 2026-08-14
tags:
  - diagnostics
  - parsing
  - lock
  - registries
relations:
  informed_by:
    - research-diagnostic-spans
  depends_on:
    - decision-0021-arena-graph-engine
    - decision-0041-the-written-lock
    - decision-0046-an-index-line-carries-the-requirement
  amends: []
  supersedes: []
---

# a parsed text's diagnostics carry their source

> takes the half of K-11 that has never run: "failures have stable semantic codes and structured context" names two things, and the codes are exercised while the context's source half is not. `Span::none()` is the only span any crate constructs — 78 sites in crate source, none elsewhere — and `SourceFile::new` is called only by pith-diag's own tests. the two parsers a person's edits actually reach today, the written lock (0041) and the index line (0046), compute line and field positions and throw them away, baking `line {number}` into prose and re-attributing the lock's messages to the index with a `line 0` that names nothing.

## context

the written lock's parser walks lines it numbers and tokens it bounds; the index reader walks lines it never numbers. both reduce that position to text the moment something is refused. the costs are measurable in the tree: a registry answer whose requirement clause is malformed is reported with the package name spelled into the message through a `reattribute` wrapper, because the index read had no position to point at; a lock bound twice reports two line numbers in one sentence, because a note with a span had nowhere to be a span; and no consumer more structured than a reading person can find the field either message talks about.

0021 already decided the representation — byte-based, utf-8-correct, pith-owned offsets, with line and column computed only at render — and pith-diag already wraps the rendering dependency. what was missing is a producer. the first frontend would have had to debug this pipeline at the same time as a grammar; this record gives it one that runs.

## proposed decision

a diagnostic produced while parsing text attaches the source it was produced from: the label naming the file, the text itself, and a span selecting what was refused. `Diag` carries an optional `Arc<SourceFile>`; a diagnostic constructed away from any text — engine evaluation, durable reads — carries none and renders exactly as before.

the span selects the field or line that failed; the message names what is wrong with it. a malformed digest's span selects the digest field including its `sha256:` prefix; a directive with the wrong arity selects its line; a twice-bound package selects the second binding line and carries a note whose span selects the first. a token-level refusal that leaves the rest of the line suspect — a quote that never closes — spans from the offending token through the end of the line. a missing directive has nothing to select and points at the start of the file.

position moves out of prose. the `line {number}` prefixes are deleted along with the index reader's `reattribute` wrapper: the package a malformed index line belongs to is named by the attached source's label, which is the file's path, and the field is named by the span. prose keeps saying *what* — the field's spelling, the complaint — and stops saying *where*.

the source is context, not identity. severity, code, span, and message persist in the durable record exactly as they already do — the sqlite adapter has carried `span_start` and `span_end` as columns since it was written — and the source is dropped at persistence. a hydrated diagnostic renders without a snippet and with its span intact, which is the degradation miette defines for a report rendered without source text: less context, no changed meaning.

the mechanisms live in the leaf crate, per 0021's wrapping rule. `SourceFile` gains `span_of` for a slice's span, `lines` for a line walk that keeps positions, and the miette `SourceCode` implementation that names the file when a handler prints `label:line:column`. `Diag`'s miette implementation gains `labels` — the primary span and each note — but only when a source is attached, so the eighty existing construction sites change not at all. phloem's `locktext` returns tokens with spans and refusals with spans; `lockfile` and `registry` attach.

## alternatives considered

### a session-wide source table the span indexes

rustc's `SourceMap` and cpplib's line table: the diagnostic carries a compact position, the producer owns a table that resolves it, and rendering asks the table. the rustc dev guide is explicit that a span "can be looked up in a SourceMap to get a 'snippet'", and cpplib defers file-and-line resolution to `line-map.cc` the same way.

rejected on the boundary. both tables assume a compilation session that outlives anything which might render, and pith's diagnostics cross that boundary by design: the durable record outlives the process that parsed, and a second process hydrates and renders. an index into a table the next process never had is a span that cannot be explained. the parse sites also have no session to own a table — `read_index` reads many small files and keeps none of them open.

### attach the text at the render boundary

miette's own pattern: "Sometimes it makes sense to add source code to the error message later. One option is to use with_source_code() method for that." the parse returns diagnostics that carry spans and no text; the entry point that renders wraps them with the file.

rejected because in pith the parse *is* the boundary that holds the file. 0044 placed the registry read as a caller-side effect and 0041 the lock's read the same way, so by the time a sink reaches a renderer, the path and the text are gone unless something carried them. miette's pattern fits a parse library whose caller owns the files; here the parser is the caller. attaching at the parse costs one `Arc` per diagnostic and no signature beyond it, and late attachment would have every future caller re-implementing the plumbing this record builds once.

### carry a uri and no text

the LSP shape: "The range at which the message applies," a uri naming the document, and the text never in the structure — the editor owns it and resolves the range when it draws.

rejected on who pith's renderers are. a language server has exactly one consumer that provably holds every document open; a lock read has a terminal and a json api, and neither has anything open. the text is in hand at the parse, and the uri-only shape turns every snippet into a second file read by a party that may not have the file — the drift a lock read is reporting can be the file having changed underneath. LSP also forces the position into negotiated line-and-column units at emit ("the only mandatory encoding is UTF-16"), which 0021 declined for the same reason rustc did: offsets stay compact and comparable, and line and column are a rendering concern.

### keep position in prose

the status quo ante, with the line number spelled into every message.

rejected as measured, not argued. nineteen `line {number}` sites produced positions no api consumer could read, the index reader had no numbers at all and re-baked its messages through `reattribute` to say which package was refused, and the twice-bound conflict named two line numbers in one sentence where it now carries two spans. rustc's guidance runs the other way — the message "able to stand on its own" while the span points at the code — and a terminal render gains the snippet that prose was impersonating.

## consequences

`Diag` grows `source: Option<Arc<SourceFile>>` behind a `with_source` builder; `Diag::new` and `Diag::engine` signatures are unchanged, and the engine, executor, and store diagnostics are untouched. the durable diagnostic record does not change, so no encoding version moves and the conformance suite is unaffected. the CLI's `render_diag` is unchanged and gains snippets and `label:line:column` headers for any diagnostic that carries a source.

a parser's diagnostics now hold an `Arc` to the whole parsed text, so a sink of lock refusals keeps the lock alive until it drops; a lock is small, and the alternative — copying the quoted region into the diagnostic — was the prose position wearing a new coat. notes render as labeled secondary spans when a source exists, which is what the twice-binding conflict wanted all along.

the rule leaves engine evaluation exactly where it was: an evaluation has no text to point at, its diagnostics carry no source, and nothing about them renders differently. the surface language is the producer that changes that, and it arrives to a pipeline that already runs.

### measured

`a_malformed_lock_read_back_carries_its_file_and_renders_its_position` (`crates/phloem/tests/written_lock.rs`) is the round's headline: a lock published through `lockpublish::write`, its bind line's digest corrupted to `blake3:not-hex`, read back through `lockpublish::read`, refuses with a diagnostic whose source label is the path it was read from, whose span selects exactly `blake3:not-hex`, and whose miette graphical render prints `pith.lock:7:27` with the field quoted in the snippet — producer to render, one chain, identity untouched by any of it.

`a_union_merge_binding_one_package_twice_is_refused_naming_both_lines` (`crates/phloem/src/lock/file.rs`) now asserts the structure the sentence used to spell: the primary span selects the second `bind` line and the note's span selects the first, both by comparing the selected bytes against the fixture.

`a_malformed_requirement_is_refused_at_its_field_in_the_package_file` (`crates/phloem/tests/dependent_resolution.rs`) is the registry half: a corrupted requirement clause refuses with the attached source labeled by the package's index file path and the span selecting `1.0` — the two facts `reattribute` used to bake into prose, now structure.

`an_attached_source_renders_its_label_line_and_column` and `a_note_becomes_a_second_label_in_the_rendered_report` (`crates/pith-diag/src/lib.rs`) hold the render mechanics in the leaf crate, and `tokens_carry_spans_of_their_written_spelling_at_their_base` and `a_comma_piece_of_a_feature_list_spans_its_written_spelling` (`crates/phloem/src/lock/text.rs`) hold the tokenizer's, including that a quoted token's span covers its written spelling at written length, escapes included.

by count: the three parser files (`lock/text.rs`, `lock/file.rs`, `registry.rs`) construct no `Span::none()` diagnostic and no `line {number}` message; before the round they held nineteen of the latter. thirty-one construction sites attach a span and a source. the workspace suite is 722 tests, 0 failures, against 710 before.

## unresolved

the 9000 range is still unallocated. phloem stamps `StableCode(9004)` on every diagnostic it emits and xylem `9002`, while pith-diag documents a 1000-based engine namespace and a reserved 2000-based composition namespace and nothing else. allocating per-domain ranges needs an answer for third-party domains — peerhood means the allocation cannot be a closed list, and a first-come registry is a coordination mechanism the project has no other need for — and that argument deserves its own record while two domains make the change cheap.

`Checkpoint::parse` still refuses without position. it is three fixed lines and lives in `witness.rs` beside the leaves read this round did thread; it was left alone rather than half-adopted, and it is the smallest next consumer of the same helpers.

the durable record keeps a span with no label, so a hydrated diagnostic renders its message and code and points nowhere. whether the label belongs in the record is a question for the first tool that re-renders hydrated diagnostics; until one exists, the drop is the honest form, and re-rendering from the still-on-disk file is a reader's choice.

engine evaluation diagnostics carry `Span::none()` today and will until the surface language gives an evaluation something to point at. the rule this record states — attach when the producer holds the text — leaves that case open on purpose.
