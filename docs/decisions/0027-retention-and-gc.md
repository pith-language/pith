---
schema: design-doc/v1
id: decision-0027-retention-and-gc
title: frame retention and garbage collection as roots, two stores, and composable policy axes
summary: define the GC problem for pith's two-store model (content CAS and relational engine state), name the root-set options and policy axes with their evidence from mature systems, and leave the final default combination to workload evidence
kind: decision
status: proposed
created: 2026-05-15
updated: 2026-05-15
tags:
  - persistence
  - caching
  - retention
  - provenance
relations:
  informed_by:
    - research-build-systems
    - research-nix
  depends_on:
    - decision-0013-managed-object-identity
    - decision-0014-reproducibility-properties
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0025-relational-engine-state
  supersedes: []
---

# frame retention and garbage collection as roots, two stores, and composable policy axes

> complements [0024: persist content in a filesystem cas and engine state in sqlite](0024-persistent-engine-state.md), which names retention, garbage collection, compaction, and historical-provenance limits as unresolved and in need of workload evidence, and [0025: relational engine state](0025-relational-engine-state.md), which removed the representation blocker by making reverse-edge reachability a query. this record frames the problem and the option space so that a later workload-evidence record lands in a defined design space, with no pretense of settling the numbers.

## context

0024 left retention and GC open. 0025 did the prerequisite that made those questions answerable: by storing engine state as normalized relations, reverse-edge traversal ("which attempts depend on this one") and reachability ("is this attempt reachable from a retained root") became ordinary indexed queries instead of full-table decodes. 0025 explicitly said "this decision does not implement invalidation explanations or garbage collection; it removes the representation that blocked them." this record picks up where 0025 left off.

the absence of a GC model is itself a policy. today nothing is collected: every attempt, every edge, every diagnostic, and every content blob accumulates without bound. that is the right default for a prototype and the wrong default for a long-lived engine. the question is what to replace it with.

two properties of pith make the GC problem different from a generic build cache, and both come from earlier decisions.

there are two stores with different shapes. the content store (`pith-store`) is immutable and identity-addressed: blobs and trees, never mutated, only added or removed, cheap to keep and expensive to refetch. the engine-state store (sqlite) is mutable history: multiple attempts per `PureComputationKey`, failed and cancelled attempts that are "never cache hits but remain provenance" (0024:62), a reusable index that points at the newest completed attempt per key. content and engine state have different failure modes and want different retention policies.

provenance is a first-class contract. decision 0014 commits to reproducibility verification (building twice and comparing), which presupposes that the first build's provenance is still available. decision 0013 commits to managed-object continuity across observations, mutations, and platform re-creation, which presupposes that an observed managed object's history is retained. aggressive GC that deletes provenance breaks both contracts.

the tension is direct. "preserve information" (principles) says keep things; "optimize for projects that live" (principles) says history grows without bound on a long-lived project. the honest position is that retention is bounded by a visible, inspectable policy, with no hope that nothing ever needs deleting.

## the two stores and what is in each

GC is "delete what is not reachable from the roots." naming the roots requires being clear about what the two stores hold.

the content store holds immutable blobs and trees by digest. its only internal structure is trees referencing blobs and subtrees. a content object is live if and only if some retained engine-state record references it; content has no independent root set.

the engine-state store holds, per the schema in `pith-state-sqlite`, attempts (each in one of `Pending`, `Complete`, `Failed`, `Cancelled`, each belonging to a computation key that may have many attempts over time), the reusable index (the current cache surface, mapping a computation key to its newest completed reusable attempt), and ordered dependency edges with categories, capability-use edges, diagnostics and their notes, executor reports and their produced outputs.

the reusable index is the cache. everything else is, or participates in, provenance. 0024 already encodes this split for the failed/cancelled case ("never cache hits but remain provenance"). the GC question generalizes it: every attempt is provenance; only some attempts are additionally cache hits.

## the roots question (the load-bearing decision)

the entire policy turns on what counts as a root. four candidate root sets each preserve different things.

R1 is the reusable index only. roots are the current cache surface. everything not reachable from "the answer to the most recent build" is collectable. this is the most aggressive option and the closest to a classic build-cache GC. it loses all history, all failed attempts, all stale observations, and all superseded provenance.

no mature system does this. Nix's `--check` and Debian's `.buildinfo` are existence proofs that reproducibility verification requires the original build's provenance to be retained even after the build is "done." Terraform's whole problem is that mutable external state needs a stable continuity reference; deleting observation history because no current computation depends on it is exactly the failure 0013 was written to prevent. R1 violates 0014 (reproducibility) and 0013 (managed-object continuity). rejected.

R2 is R1 plus explicitly pinned roots. the reusable index plus whatever the system or the user names as retained: revision-pinned plans (0012), managed-object identities and their observation history (0013), in-flight `Pending` attempts (which should not exist after reopen per 0024's recovery rule, but defensively include any), and user-designated revisions.

this is Terraform's model (state is the root) and Nix's gcroots model (anything you care about, you pin via a symlink). it is necessary. it is not sufficient, because it does not bound the history of unpinned computations. Pulumi's DIY backend defaulted to this and users reported stacks exceeding 200 GB, mostly history.

R3 is R1 plus bounded history. for each computation key, retain the latest N terminal attempts, plus any attempt within a time window. history is retained up to a budget; the cache stays precise.

this is the consensus design among systems with history. Git runs two retention policies at once: 90 days for reachable reflog entries, 30 days for unreachable ones, and a 2-week mtime grace before pruning loose objects. etcd retains every revision within a configurable window (time- or count-based). Gradle's local cache cleans unused entries after 7 to 30 days. Kubernetes events carry a 1-hour TTL by default. the asymmetry across these systems encodes the same hypothesis: recent history is worth keeping for recovery; unreachable history is worth keeping briefly; nothing is worth keeping forever by default.

R3 has one documented failure mode: if the history window is shorter than the interval between engine invocations on a long-lived graph, the engine hits "required attempt has been GC'd" errors that mirror etcd's compaction-boundary failures. the window must be sized generously relative to any revalidation or resync cadence.

R4 is everything reachable from any known identity. as long as a `RuleIdentity` is known, all its attempts are roots. this is effectively no GC of engine state; only content is ever collected.

no system adopts this as a policy. Pulumi's DIY backend fell into it accidentally and produced the 200 GB stacks. Salsa explicitly evicts query results. Git explicitly prunes. etcd explicitly compacts. R4 is today's de-facto state (nothing is collected), which is appropriate before release and wrong as the target.

## the recommendation in this record's scope

this record frames the problem; it does not fix the final default combination, because 0024 explicitly asked for workload evidence and that evidence does not exist yet. the design space the workload evidence will land in is narrow enough to name.

roots are the reusable index (R1), plus explicitly pinned managed objects, plans, and revisions (R2), plus a bounded history window per computation key (R3). the relational store is governed by the history window because it is cheap to keep and is the provenance contract. the content store is governed by reachability from the retained relational set, with size-budget LRU as secondary pressure relief, which is the universal CAS pattern (Bazel, Gradle, Nix's store).

the reason this is the design space, with the question reduced to numbers, is that every mature system with a history component converges on a policy expressible in these terms. R1 is too aggressive (breaks 0014 and 0013). R4 is too permissive (Pulumi's failure mode at scale). R2 alone does not bound growth. R3 alone loses pinned continuity. the combination is the only position consistent with both the provenance contracts and the unbounded-growth avoidance the principles demand.

the values of N (attempts per key), the time window, the TTLs per category, and the size budget are the workload-evidence questions a later record settles. this record names the axes.

## the policy axes

four independent axes compose on top of the root model. a final policy is a combination of these; the combination is what workload evidence informs.

time and TTL retain within a window. this is the time component of R3 and the natural axis for observations (it composes with 0019's staleness semantics, since an `Observation` category is already partly TTL-shaped) and for diagnostics (debugging material, with no claim on verification; a shorter TTL than provenance is defensible, on the Kubernetes-events precedent). TTL is not appropriate as the primary axis for content blobs: Bazel's `experimental_remote_cache_ttl` is a documented antipattern, where a client-side TTL assumption becomes decoupled from the server's actual eviction and produces either unnecessary rebuilds or unrecoverable "blob missing" errors.

generation and count retain N attempts per key, or N builds or plan applications. useful for the cache-hit tier; for provenance the time window is the better axis. Nix's `nix-collect-garbage --delete-older-than` is generation-flavored; Nix's well-documented gap is the lack of "keep at least N generations," which community tools fill.

size-budget LRU evicts least-recently-used non-pinned objects once the store exceeds a budget. this is the universal CAS pattern (bazel-remote, Gradle's local cache, Nix's `--max-free`). it is right for the content store and wrong as the primary axis for engine state, because provenance is ordered by relevance to current and verifiable results, with no ordering by recency of access.

explicit pinning names user- or system-designated roots retained regardless of other policy. mandatory for managed objects (they are always roots, never reachable-from-cache), plans, and user-designated revisions. this is the escape valve that makes any of the above acceptable.

a plausible pith policy is R2 plus R3 roots, with TTL on diagnostics, generation-count on the cache-hit tier, size-budget LRU on the content store, and explicit pinning for managed-object history. whether that exact combination is the default is a workload-evidence question; that it is expressible in these axes is a property of the design space.

## the cross-store ordering problem

the content store and the engine-state store cannot share a transaction. a GC that deletes engine state pointing at content, then deletes the content, has a window where the two are inconsistent. 0024's invariant is strict on this: "publication ordering must tolerate unreachable content while never exposing metadata that points at absent content."

the ordering that preserves the invariant is content-first, metadata-last. first, compute the retained engine-state set (roots plus bounded history plus pinned) in one transaction; this produces a stable set of content IDs that remain referenced. second, compute the unreachable content set (all content in the CAS minus the referenced content IDs). third, delete unreachable content from the CAS; a crash here is safe because content is immutable and retained metadata still references only retained content. fourth, delete unreachable engine-state rows (attempts, edges, diagnostics, reports outside the history window) in a second transaction, after content deletion has committed or after a grace period like Git's 2-week mtime window.

Nix's two-phase delete (move to `/nix/store/trash` atomically, empty trash later) and Git's mtime-grace defense are the field-tested implementations of this ordering. the shared principle is to never delete immediately; always defer, so a crash leaves a recoverable state.

the Salsa lesson composes here. Salsa drops query values (evictable) while retaining dependency info (not evictable) so it can still compute whether values may have changed. the pith analog is that an attempt's result blob and produced content are evictable independently of its edges, capability records, and diagnostic trail. the two stores have independent retention, and the relational schema already separates these cleanly (`attempts.result` and `produced_outputs.content` are cache-hit material; `dependencies`, `report_capabilities`, `diagnostics` are provenance material).

## provenance versus cache: two retention tiers

the cleanest framing the research surfaces is to treat provenance and cache as two evictability classes within the engine-state store, on the Git reflog/prune model.

the cache tier is an attempt in the reusable index, which is a cache hit. evictable under the history window when superseded by a newer reusable attempt for the same key. its produced content is evictable on the same schedule.

the provenance tier is the dependency edges, capability records, diagnostic trail, and the attempt row itself (even when its result is evicted), which are verification material. retained on a longer window than the cache tier, or indefinitely if pinned.

this generalizes 0024's existing "failed and cancelled attempts remain provenance but are never cache hits" to every attempt. the schema in `pith-state-sqlite` already separates these into different tables and columns; a future GC treats them as two evictability classes.

the reproducibility angle (0014) makes this split load-bearing. Debian's `.buildinfo` is signed metadata recording exact dependency versions, architecture, and build flags, retained essentially forever in the archive, used by `reproduce.debian.net` for bit-for-bit rebuilds. pith's per-attempt provenance is its `.buildinfo`. a GC that aggressively deleted provenance would remove the ability to verify, which is the whole point of retaining it.

## couplings to flag, not solve

three earlier decisions create retention requirements the GC design must respect but does not re-litigate.

managed-object identity (0013). a managed object is the durable thing a deployment owns and mutates across observations. its observation history is not a cache; it is the user's contract with reality, the equivalent of Terraform's state file. managed objects are always roots, never reachable-from-cache. a GC that deleted a managed object's observation history because no current computation depends on it would reproduce Terraform's documented drift failures. pinning is mandatory for managed objects; reachability from the reusable index is not enough.

revision-pinned plans (0012). a mutation in a plan pins the revision of a specific managed object. a GC that collected the pinned revision while the plan still referenced it would break the plan. pinned plans are roots.

remote caches (future). 0024 unresolved: remote content stores and remote cache metadata use the same durable identities but need separate trust, transport, and authorization decisions. local GC is local policy; it does not propagate to a remote cache, and the contract between them is not "GC intent propagates" (Bazel's mistake) but "content-addressed identity makes any copy valid" (Nix's substituter model). a remote cache hit must still be revalidated against local provenance and the current reusable index before use, per 0024's revalidation contract.

## sqlite-specific operational discipline

the engine-state store is sqlite. unbounded growth has documented pathologies and documented fixes; the GC design should adopt the fixes from the start.

enable `PRAGMA auto_vacuum = INCREMENTAL` at creation time. pith's GC will delete whole attempts and their edge/diagnostic cascades at a time, which means large contiguous frees. incremental vacuum reclaims those without a full rewrite. this must be set on the very first database creation; 0024's move-aside-and-rebuild on incompatible versions means it cannot be retrofitted later without a migration.

use `wal_autocheckpoint` plus an explicit `wal_checkpoint` at scheduling boundaries. 0024 already serializes metadata through the engine owner; an explicit checkpoint at those boundaries bounds WAL growth.

run `incremental_vacuum` after GC passes, with no use of the full `VACUUM`. the full VACUUM cost (a complete rewrite, temporary disk doubling) is unjustifiable for a long-lived engine that GCs incrementally.

do not partition by time. partitioning answers log-shaped stores; pith's engine state is graph-shaped, with attempts referencing each other across time via dependency edges. time-partitioning would turn reverse-edge queries (the `dependencies_by_target` index that 0025 made load-bearing) into cross-partition joins. keep one database; bound growth with R3.

## alternatives considered

### keep the status quo (no GC, everything retained forever)

R4 as policy. internally consistent and cheapest. rejected because the foreclosed cost is not optional: Pulumi's DIY backend hit 200 GB stacks; etcd bloat has taken down kubernetes control planes; Salsa and Git both explicitly evict. the principles say "optimize for projects that live," and unbounded growth is the opposite.

### event-log compaction

store the graph as an append-only event log and derive the current state by replay, compacting the log periodically. considered and rejected by 0024: "this preserves history naturally, but adds replay, compaction, and secondary-index work before the persistent semantics are proven." the relational store already retains history (multiple attempts per key) without making the event log the primary database; this record does not revisit that.

### copying GC via snapshot

retain a named snapshot and drop everything not reachable from it. close to R1 with an explicit snapshot root. rejected on the same grounds as R1: it loses provenance and managed-object history that the contracts require. the snapshot idea is partially absorbed into R2's explicit pinning, where a user can pin a snapshot-equivalent without making it the only root.

### client-side TTL as the primary axis

Bazel's model: the client assumes a TTL and pre-emptively treats cached state as stale. rejected because the client's TTL assumption becomes decoupled from the server's actual eviction, producing either unnecessary rebuilds (TTL too short) or unrecoverable "blob missing" errors (TTL too long). TTL is retained as an axis for diagnostics and as the time-window component of R3, with no claim on being the primary content-retention policy.

## consequences

the decision does not implement GC. it defines the design space a future implementation lands in: roots (R2 plus R3), axes (TTL, generation, size-budget LRU, explicit pinning), cross-store ordering (content-first, metadata-last), and two retention tiers (cache, provenance).

the reusable index is confirmed as the cache root set, structurally analogous to Nix's gcroots. managed objects, plans, and user-designated revisions are confirmed as always-roots, never reachable-from-cache, which closes the Terraform-state-analog question for 0013. the relational schema in `pith-state-sqlite` already has the indices GC needs (`attempts_by_computation`, `attempts_pending`, `dependencies_by_target` for reverse edges); no schema change is required to implement the framed policy.

the sqlite operational discipline (incremental vacuum from creation, checkpoint at scheduling boundaries, no time-partitioning) is adoptable now and is not contingent on the workload-evidence question.

the workload-evidence question is narrowed to the numeric parameters (how many attempts per key, how long a window, what size budget, what diagnostic TTL), with the shape of the policy already settled. a later record, informed by the M-2 and M-3 prototypes running against real graphs, picks those.

## unresolved

the default values of N (attempts retained per key), the reachable and unreachable history windows (the Git-analog 90/30 split or different), the diagnostic TTL, and the content-store size budget all need workload evidence from the M-2 and M-3 prototypes. the shape of the policy does not; the numbers do.

whether dependency edges want an index in both directions (so reverse traversal for GC is an index walk, with no scan of the forward index) is the open indexing question 0025 already named. the current `dependencies_by_target` reverse index may suffice; workload evidence settles it.

the exact API surface for explicit pinning (how a user pins a plan, a managed object, or a revision; how a pin is inspected, transferred, and released; how pins survive a database reopen) is part of the deployment-library work this record enables, with no claim on the GC frame.

remote-cache coherence is a separate trust-and-transport decision (0024 unresolved). this record's local-GC frame does not assume or imply a remote policy; it only commits to the local contract that content-addressed identity makes any copy valid and that local GC does not propagate.
