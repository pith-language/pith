---
schema: design-doc/v1
id: decision-0024-persistent-engine-state
title: persist content in a filesystem cas and engine state in sqlite
summary: keep arena handles process-local while durable computation, dependency, cache, and provenance records use typed digests and transactional sqlite state
kind: decision
status: proposed
created: 2026-05-06
updated: 2026-06-01
tags:
  - persistence
  - caching
  - provenance
relations:
  informed_by:
    - research-build-systems
    - research-artifacts-and-trust
  depends_on:
    - decision-0005-separate-identities
    - decision-0021-arena-graph-engine
    - decision-0023-rule-and-cache-identity
  supersedes: []
---

# persist content in a filesystem cas and engine state in sqlite

## context

the prototype keeps content, graph nodes, dependency edges, reuse decisions, and provenance in one engine instance. arena handles deliberately include an instance owner and cannot cross engine boundaries. persistent reuse therefore needs a durable representation that preserves graph semantics without treating process-local handles as identities.

content and graph metadata have different storage requirements. immutable bytes and trees are naturally addressed by digest and may be large. computation records and dependency edges require transactions, indices, reverse queries, and crash recovery. forcing both into one representation weakens one side or recreates database behavior in the engine.

## proposed decision

immutable content is stored in a filesystem content-addressed store. engine metadata is stored in sqlite through pith-owned interfaces. neither filesystem paths nor sqlite types appear in the kernel's semantic interfaces.

arena `RuleId`, `ComputationId`, `ValueId`, and related handles remain process-local. persistent records use `RuleIdentity`, `RuleRevision`, computation digests, content identities, and other typed durable digests. loading a graph creates fresh arena nodes and maintains an internal mapping from durable digests to local handles.

the initial implementation has one process owning the writable engine database. sqlite provides atomic transactions and concurrent readers; multi-process scheduling and distributed graph mutation are not part of this decision.

## filesystem content store

blobs are stored as their exact bytes under a path derived from `ContentId`. trees are stored as a versioned canonical manifest under their tree content identity. materialized filesystem trees are derived views and are not the canonical stored representation.

writes use a temporary file in the destination filesystem, flush the file, and atomically rename it into place. an existing object with the same identity is reused. reads verify that stored bytes or manifests still produce the requested identity; corruption is an adapter error, never a cache miss.

content written before a metadata transaction commits may become unreachable when the transaction fails. this is safe because content is immutable. later garbage collection may remove objects not reachable from retained graph and cache records.

## sqlite engine state

sqlite stores:

- the storage schema and semantic encoding versions
- stable rule identities and observed revisions
- durable computation keys and evaluation attempts
- ordered dependency edges and their categories
- canonical typed results or content identities
- action plans, authorization decisions, executor reports, and capability use
- reuse, invalidation, failure, and cancellation reasons
- the index from reusable computation keys to completed attempts

an evaluation attempt has one of four states: `Pending`, `Complete`, `Failed`, or `Cancelled`. only `Complete` attempts can enter a reusable-result index. failures and cancellations remain provenance but are never result cache hits.

creating an attempt records `Pending` before effectful execution begins. completion writes the result, final dependency set, provenance, and reusable index entry in one transaction. failure or cancellation writes its final state and available provenance in one transaction. after a crash, remaining `Pending` attempts are marked failed as interrupted work; they are not resumed implicitly.

## dynamic dependencies and revalidation

a durable pure-computation key identifies the rule application and its explicit inputs. because a pure rule may request dynamic dependencies, finding that key is not sufficient for reuse.

the engine loads the previous completed attempt and revalidates its recorded dependency set. a dependency whose durable result identity changed makes the consumer dirty. when reevaluation produces a value equal under the type's canonical equality, downstream propagation stops even though a new attempt records the changed upstream provenance.

the dependency set discovered by a successful reevaluation replaces the prior set atomically with the new completed attempt. a failed or cancelled reevaluation does not publish a partial dependency set as the latest valid graph.

an action dependency edge is recorded once its action attempt exists, before success or failure is propagated to the requesting pure computation. planning failures that create no action attempt create no target edge. this keeps retained policy denials and executor failures reachable from their requesters.

## schema compatibility

the database begins with an explicit schema version and semantic encoding version. before the first release, an incompatible metadata version causes the metadata database to be moved aside and rebuilt from an empty graph and cache. compatible content objects remain usable because their domain-separated content identities include their encoding version.

silent interpretation under a different schema is forbidden. explicit migrations can be added when retaining user-visible history becomes a release requirement.

## adapter boundaries

`pith-store` owns the content-store interface and its memory and filesystem adapters. `pith-engine` owns an engine-state interface expressed in durable pith data types. the sqlite implementation stays behind that interface.

the synchronous pure evaluator does not perform sqlite work during an individual rule step. state lookup and transactional publication happen at engine scheduling boundaries. the first implementation may serialize metadata access through the engine owner rather than adding locks throughout the arena graph.

## alternatives considered

### persist arena indices

write local integer handles directly and reconstruct arenas with the same indices.

this conflicts with per-instance ownership, makes compaction and partial loading fragile, and turns an implementation detail into durable identity.

### sqlite for content and metadata

store blob bytes, tree manifests, and graph records in one database.

transactions are convenient, but large immutable content is better served by digest paths, atomic filesystem operations, and later remote-store adapters. database growth and backup behavior would couple content retention to graph metadata.

### filesystem files for the entire graph

store every computation and edge as a content-addressed manifest.

this makes immutable snapshots straightforward but requires the engine to build transactions, reverse-edge indices, latest-attempt indices, and crash recovery itself. sqlite already provides those mechanisms locally.

### append-only event log

record every state transition and derive the current graph by replay.

this preserves history naturally, but adds replay, compaction, and secondary-index work before the persistent semantics are proven. immutable evaluation-attempt rows retain history without making the event log the primary database.

## consequences

persistent storage has two adapters with distinct failure modes. publication ordering must tolerate unreachable content while never exposing metadata that points at absent content.

canonical typed-value encoding becomes a storage contract. adding value forms requires versioned encoding work, not an ad hoc serde representation.

the graph remains an arena during evaluation. persistence does not replace the representation chosen in 0021; it supplies durable records that can hydrate and validate that graph across engine instances.

initial schema incompatibility trades cache loss for correctness and implementation speed. this is appropriate before release and must be replaced by explicit migration policy before persisted provenance becomes a stable user contract.

## unresolved

retention policy, garbage collection, database compaction, and historical-provenance limits need workload evidence.

remote content stores and remote cache metadata use the same durable identities but need separate trust, transport, and authorization decisions.

the exact canonical encoding of future structural values and module identities belongs to the values and module work. each implemented subset is versioned before it is persisted.

