---
schema: design-doc/v1
id: research-incremental-revalidation
title: how deep a validity check goes
summary: how Salsa, rustc, Bazel, Nix, and the Build Systems à la Carte taxonomy each answer whether validating a cached result inspects one edge or the whole subtree beneath it, and what each answer costs in retention
kind: research
status: researching
evidence: reviewed
created: 2026-07-29
updated: 2026-07-29
tags:
  - research
  - incrementality
  - caching
  - retention
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - research-method
  supersedes: []
---

# how deep a validity check goes

an engine holding a cached result has to decide whether it is still the answer. the recorded dependency edge is where every system starts, and the question this note reads for is what happens below it: does the check stop at the edge, walk the subtree the edge opens, or consult a summary of that subtree recorded when the result was made?

the three answers are all in production, and each one prices differently in a place the incremental literature usually treats separately: what the collector is allowed to delete. a check that reaches into a subtree makes that subtree undeletable while the result above it is worth keeping. that coupling is the substance here, and it is the part the systems document least deliberately — Bazel and rustc pay it as a memory number, Nix pays it as disk, Salsa states it as a rule, and the taxonomy paper prices the alternative in a lost optimization rather than in bytes.

this note is the mechanism-level companion to `build-systems.md`, which compared engines. here the question is one predicate inside them.

## Salsa states the rule the others imply

Salsa's revalidation is `maybe_changed_after`, and it is a demand-driven recursive walk: asking whether a query's memoized value is still good asks the same question of each dependency, which asks it of theirs. rust-analyzer's account of the design is that the work of tracking invalidation happens when a fresh result is requested rather than when an input changes, so nothing is marked dirty eagerly and the walk is the whole mechanism.

what makes Salsa the sharpest source is that it also shipped eviction, and had to say what eviction may touch. RFC0004 adds an LRU bound per query group: "if a new value is computed, and there are already n existing ones in the database, the least recently used one is evicted." then the constraint, stated in one line and italicized in the original:

> Note that information about query dependencies is **not** evicted.

that is the coupling, admitted. a recursive validity check turns the dependency graph into a structure the collector cannot touch, and the only thing left evictable is the values hanging off it. Salsa's LRU is therefore a bound on memoized results and not on the graph, and a Salsa database's floor is the graph of everything it has ever been asked.

## rustc loads the whole previous graph, every session

rustc's incremental compilation is the same algorithm at a process boundary. `try_mark_green` takes a node from the previous session's dependency graph and checks its dependencies in turn; for a dependency whose colour is not yet known it calls itself, and if that fails it re-runs the query and compares fingerprints. the node goes green only if every dependency did.

the retention consequence is not a policy in rustc, it is the file format. the dev guide: "when a compilation session starts, the compiler loads the previous dependency graph into memory as an immutable piece of data," and "it's also cheap to load the entire set of fingerprints together with the dependency graph." there is no partial load and no eviction. rustc's answer to the coupling is to accept it completely and then bound the total by discarding the whole cache on any compiler-identity mismatch, which is the same discard-or-keep-everything shape 0048 records for derived caches.

## Bazel walks the graph and then built a way to stop retaining it

Skyframe inverts the direction — invalidation is bottom-up, from changed files to the nodes that transitively depend on them — but the retained structure is the same one. the graph of dependencies between nodes lives in the Bazel server across builds, and it is what makes the second build fast.

Skyframe's change pruning is early cutoff by another name, and it is stated as a resurrection: "if a node is invalidated, but upon rebuild, it is discovered that its new value is the same as its old value, the nodes that were invalidated due to a change in this node are 'resurrected'." editing a comment in a C++ file recompiles the object file, finds the same bytes, and the link does not run.

what makes Bazel the most useful source is the mechanism it added when the retained graph got too expensive. `--discard_analysis_cache` frees the analysis structures and pays for it by redoing analysis next build. Skyfocus goes further: the developer names a working set of directories, and "Bazel will only keep state needed to correctly incrementally rebuild changes to those files," reported at 45% of heap in the documented example. the price is stated plainly in the same page:

> Changes outside of the working set will cause a build error.

so a system whose validity check walks the graph, having found the graph too large to keep, could not find a way to drop part of it that degrades to *recompute*. it drops to a hard failure at the boundary instead. that is worth carrying: the failure mode of a pruned closure is a design choice, and Bazel's evidence is that the safe one is not free to get.

## Nix folds the subtree into the identity instead

Nix does not revalidate. a derivation's output path is computed from the derivation graph beneath it, so a change anywhere in the closure produces a different path, and the question "is this still valid" is answered by whether the path exists. there is no walk because there is no check.

the retention consequence follows from the same construction. the collector deletes "any package not used (directly or indirectly) by any generation of any profile," and the manual is explicit that indirection means the whole closure: "all derivations that are build-time dependencies of garbage collector roots will be kept and that all output paths that are runtime dependencies will be kept as well." a root retains a closure. Nix's floor is the transitive closure of its roots by construction, and nobody experiences this as a surprise, because the closure is also the unit users name, copy, and substitute.

what buys Nix this is that a derivation's dependencies are known before it is built. nothing is discovered while a builder runs, so the closure can enter the identity. an engine whose dependencies are discovered by running the computation cannot construct that identity: it would need the answer to build the key that looks the answer up.

## the taxonomy prices the third answer

*Build Systems à la Carte* separates the rebuilder from the scheduler and names four rebuilders. a **dirty bit** marks what changed (Excel, Make). a **verifying trace** records the hashes of a target's dependencies and rebuilds when they move (Ninja, Shake). a **constructive trace** additionally stores the resulting value so other users can fetch it (Bazel). a **deep constructive trace** stores only the terminal input keys, ignoring the intermediate dependencies (Buck, Nix).

the paper's result on the fourth is the one that matters here:

> All traces except for deep traces support the early cutoff optimisation.

and the second: deep traces "may generate frankenbuilds if the tasks are not deterministic," because a value keyed on terminal inputs alone can be combined with values from a build that took a different route.

the taxonomy makes the summary-of-the-subtree answer legible as a category rather than an implementation trick. summarizing a subtree onto the result buys a check that needs no walk and no retained subtree, and what it spends is early cutoff: a terminal input that moves invalidates every result above it, whether or not anything the results are made of actually changed. Nix, which is in this category, does not feel the loss, because it has no early cutoff to lose — a changed input changes the output path regardless.

## what the five agree and disagree about

they agree on what a shallow check is worth: nothing on its own. no system checks one edge and stops. Salsa, rustc, and Bazel all reach the whole subtree, through a recursive walk or a reverse-edge sweep, and Nix reaches it at identity-construction time. an engine that checks its immediate edges and trusts what is under them has no analogue among these.

they disagree about where the transitive cost is paid, and the disagreement is stable. paid at check time, it is a walk and a retained closure: Salsa, rustc, Bazel. paid at record time, it is a summary and a lost cutoff: the deep-trace category, Nix's closure hashing, Buck's terminal keys. no system in this reading pays it in both places, and no system avoids paying it.

the third thing they agree on, having each arrived at it separately, is that the retained structure and the retained values are different classes. Salsa evicts values and keeps dependencies. Bazel's `--discard_analysis_cache` drops analysis structures and keeps the on-disk action cache. rustc keeps fingerprints beside the graph and nothing else from the last session's outputs. the walk needs edges and the fingerprints the comparisons run over; it does not need the artifacts.

## what this leaves for pith

the engine's recorded pure edge already carries what a verifying trace carries — the dependency's computation key and the attempt it named — and 0049 added the check that the rule behind that key is still registered at that revision. so the edge-level check is a verifying trace in this taxonomy, and the missing piece is the recursion every system in this note has and this engine does not: hydration serves a recorded result without ever requesting the dependency, so the scheduler that would have re-asked the question never runs.

the two mechanisms that could close it are the two categories above, and the note's reading says what each costs. a walk buys the property and raises the retention floor to the closure, which Salsa states as a rule, rustc as a file format, and Bazel as a memory problem it shipped a working-set feature to relieve. a per-attempt summary of the rule revisions a subtree used is a deep trace over the rule set, and the taxonomy's result says what it spends: early cutoff, which is the property 0033 exists to preserve.

one thing this reading does not settle is the failure mode of a pruned closure. Bazel's Skyfocus errors; Nix's collector refuses to break a closure in the first place; Salsa's graph is never pruned. an engine whose missing record degrades to recompute rather than to an error would be doing something none of the five do, and whether that is an improvement or an unpriced cost depends on whether the collector can be trusted to prune the right things — which is 0027's open question, now with a floor under it.

## sources

- [Salsa RFC0004: LRU](https://github.com/salsa-rs/salsa-rfcs/blob/master/RFC0004-LRU.md)
- [Salsa tuning: LRU and durability](https://salsa-rs.netlify.app/tuning)
- [rust-analyzer: durable incrementality](https://rust-analyzer.github.io/blog/2023/07/24/durable-incrementality.html)
- [rustc dev guide: incremental compilation in detail](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html)
- [Bazel Skyframe](https://bazel.build/reference/skyframe)
- [Bazel: optimize memory, `--discard_analysis_cache` and Skyfocus](https://bazel.build/advanced/performance/memory)
- [Nix manual: garbage collection](https://nix.dev/manual/nix/2.24/package-management/garbage-collection.html)
- [Build Systems à la Carte](https://www.microsoft.com/en-us/research/publication/build-systems-a-la-carte/)
- [Build systems à la carte: theory and practice](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/097CE52C750E69BD16B78C318754C7A4/S0956796820000088a.pdf/build_systems_a_la_carte_theory_and_practice.pdf)
