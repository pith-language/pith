---
schema: design-doc/v1
id: decision-0025-relational-engine-state
title: store engine state as normalized relations, not as canonical record blobs
summary: give the sqlite adapter real columns and edge tables so reverse queries and crash recovery are queries, and narrow the canonical-encoding contract to the payloads that already carry their own identity
kind: decision
status: proposed
created: 2026-05-11
updated: 2026-05-11
tags:
  - persistence
  - caching
  - provenance
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0021-arena-graph-engine
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
  supersedes: []
---

# store engine state as normalized relations, not as canonical record blobs

## context

decision 0024 chose sqlite for engine metadata and named four reasons: transactions, indices, reverse queries, and crash recovery. it then listed what sqlite stores — dependency edges and their categories, capability use, reuse and invalidation reasons, executor reports — as fields of those records.

the first sqlite adapter did something narrower. it wrote each attempt as two opaque blobs holding a hand-written canonical encoding, and gave the schema columns only for what it indexed: the attempt identifier, its status, its computation key, creation order, and the reusable index. the stated reason was that re-spelling record fields as columns would give the schema a second, divergent definition of a record.

that reason is real, but the arrangement costs two of the four justifications for choosing sqlite at all.

reverse queries become impossible. dependency edges live inside the blob, so "which attempts depend on this one" requires decoding every row. invalidation explanations and garbage-collection reachability are both reverse-edge traversals, and both are named as unfinished work by 0024 and by milestone M-2.

crash recovery becomes a scan. finding and failing interrupted `Pending` attempts decodes every pending record to do what a column comparison would do in one statement.

the encoding also has a weaker justification than it appears to. the record codec exists for exactly one consumer: the in-memory adapter holds record structs directly and never encodes anything. the encoding's versioning story duplicates the adapter's schema version, which already gates layout changes. and the two payloads that genuinely need canonical bytes — a typed `Value` result and an `ActionSpec` contract — already have their own versioned encodings in `pith-core`, because both are digest-bearing.

## proposed decision

engine-state adapters store records as normalized relations. an adapter is free to choose its representation; what it may not do is claim sqlite's benefits while foreclosing them.

the sqlite adapter has tables for computations, attempts, dependency edges, diagnostics and their notes, executor reports and their produced outputs, and the reusable index. dependency order is a position column, because a dependency's position in its slice is semantic. reverse queries, pending recovery, and reachability are ordinary statements against indexed columns.

the canonical-encoding contract narrows to the payloads that carry their own identity. a completed pure result is stored as `Value`'s canonical bytes; an action contract is stored as `ActionSpec`'s stored encoding alongside the contract digest it must reproduce. both encodings live in `pith-core`, are versioned there, and are what 0024 meant by "canonical typed-value encoding becomes a storage contract". the separate record encoding in `pith-engine` is removed.

`pith-engine` continues to own the `EngineStateStore` interface and the durable record types. it no longer owns a storage encoding, because there is no longer a representation every adapter must share.

## keeping the adapters honest

the objection the blob schema was defending against stands: a normalized schema is a second spelling of the record structure, and two spellings can drift.

the answer is a test rather than a design constraint. publication invariants already live in one shared module, so no adapter can accept a record another would reject. on top of that, a cross-adapter conformance suite generates valid sequences of store operations, applies them to the adapter under test, and compares every read against the in-memory adapter as a reference model. divergence becomes a failing test instead of a design argument.

this is a stronger position than the blob schema had. under the previous arrangement the two adapters shared an encoding but were tested by unrelated test files, so nothing checked that they agreed on behavior.

## a typed query layer

the sqlite adapter builds its statements through a query builder that checks column types against the rust types they are read into, and it maps the kernel's identity newtypes to storage types once. a rule identity cannot be bound where a content identity belongs, and a column cannot be read into a type the schema does not support.

this matters more after normalization than before it. six hand-written statements over three tables are easy to keep correct by reading them; the joins and edge traversals a normalized schema exists to serve are not.

the query builder is an implementation detail of one adapter. it appears in no public signature, which is the adapter discipline decision 0021 requires of every nontrivial external crate.

## alternatives considered

### keep blobs and add reverse-edge index tables

retain the canonical record encoding and denormalize dependency edges and pending status into separate index tables.

this restores reverse queries without touching the encoding. it is rejected because it makes the drift problem worse rather than better: edges would exist both inside the blob and as rows, with nothing but convention keeping them equal. the blob schema's own argument against a second definition of a record applies most sharply to this option.

### keep the blob schema and narrow 0024

accept that engine state is a transactional key-value store, and amend 0024 to stop claiming reverse queries and crash recovery as reasons for choosing sqlite.

this is internally consistent and cheapest. it is rejected because the foreclosed work is not optional: invalidation explanations are a milestone M-2 deliverable, and retention and garbage collection are named unresolved questions in 0024. deferring the representation change does not avoid it, it only moves it behind more code.

### store the whole graph as content-addressed manifests

already considered and rejected by 0024, for reasons this decision does not change.

## consequences

the sqlite schema gains tables and the engine loses a hand-written encoding. the net is less code in `pith-engine` and more structure in the adapter, which is where structure belongs when only one adapter needs it.

schema evolution now moves through migrations rather than through an encoding version bump. before release, 0024's policy is unchanged: an incompatible database is moved aside and rebuilt rather than reinterpreted. the pre-release trade of cache loss for correctness still holds.

the conformance suite becomes the contract for adding an adapter. a new backend is correct when it passes the same generated scenarios as the in-memory one, which is a clearer obligation than matching an encoding.

reverse queries, interrupted-attempt recovery, and reachability become available. this decision does not implement invalidation explanations or garbage collection; it removes the representation that blocked them.

## unresolved

retention policy, compaction, and historical-provenance limits are unchanged by this decision and still need workload evidence.

whether dependency edges want an index in both directions, or whether reverse traversal is rare enough to scan a forward index, needs a workload rather than a guess.

remote cache metadata is still a separate trust and transport decision. a normalized local schema does not imply the same shape over a network.
