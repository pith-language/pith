---
schema: design-doc/v1
id: decision-0057-the-rule-index
title: selection is a lookup on the interface, not a scan of the arena
summary: the rule arena and an index from interface to candidates become one structure, so selecting a rule is one map lookup under the same equality the scan evaluated per rule; candidates are ordered only on the ambiguous path, which is a failure path, and the measurement is a per-request cost that stops depending on the rule count
kind: decision
status: proposed
created: 2026-08-17
updated: 2026-08-17
tags:
  - rules
  - engine
  - performance
relations:
  informed_by:
    - research-dispatch
  depends_on:
    - decision-0015-interface-rule-selection
    - decision-0021-arena-graph-engine
    - decision-0047-the-declaration-table
    - decision-0050-cycle-detection-over-the-computation-key
  supersedes: []
---

# selection is a lookup on the interface, not a scan of the arena

> closes the item 0050 left in its unresolved section — "the predicate is now cheap and rule selection is not" — and takes it on the benchmark 0050 built for exactly this. 0015 stands and is unchanged; what this record shows is that 0015's refusal to rank is what makes an exact index possible, which is an argument for 0015 that 0015 did not make.

## context

0050 removed the per-request cost that grew with a build's depth and named what was left. `select_rule` walked the whole rule arena on every request, compared each rule's interface with the request's structurally, cloned the interface and the label of every match into a `Vec`, sorted it, and collected — including in the ordinary case where exactly one rule matched and there was nothing to sort.

that cost grows with the domain model rather than with the graph. under 0015 the only way to make two rules distinguishable is to give them different interfaces, and the only way to distinguish two rules over the same shape is to mint a nominal type; 0050 recorded this as an ergonomic fact after the benchmark's two-rule shapes had to give a root an input it never reads. so a domain's rule count is a function of how many distinct things it can be asked for, and every request pays a comparison against all of them.

0047 raised what each of those comparisons costs. a nominal type carries its declaration body instead of a coordinate, so comparing two interfaces that name declared types walks those bodies, and the arena walk multiplies that by the population.

there is a measurement now, which is the reason this is a record and not a cleanup. the crowded-arena shape holds the requests fixed at two thousand and grows the registered population: 0.002 ms per request against no extra rules, 0.052 ms against sixteen thousand — twenty-six times, on the cheapest population the scan can be given, one whose members fail the comparison on the output constructor after a single equal input.

## proposed decision

the rules of one effect category and an index from interface to candidates become one structure, `RuleTable<K>`. it owns the arena, `push` maintains both halves, and `select` is one lookup into an `IndexMap<Interface, SmallVec<[RuleId; 2]>>`.

### the key is the interface itself, not a digest of it

0050's unresolved section proposed "the same computation digest the engine already derives," and that is the one key this cannot use. a computation digest covers the rule identity and the rule revision along with the interface and the inputs, and both are properties of the rule the lookup is trying to find. deriving it requires having already selected.

a digest over the interface *alone* would work, and is still the wrong key. it would mint a digest domain under 0048 for a structure that never leaves the process, introduce a collision case that has to be either argued away or checked with the comparison it was meant to replace, and derive from the canonical encoding — which is a second notion of interface equality to keep in agreement with `Eq`. `Interface` already derives `Hash` beside the `Eq` that selection is defined by, and the hash is over the same fields at the same cost as encoding them.

### the equivalence needs no argument in two directions

0050 had to prove its new predicate agreed with the old one, because a digest over four fields replaced a comparison of three. here the index's key equality *is* the predicate the scan evaluated: the scan kept rules where `rule.interface == request.interface`, and the map returns the bucket under that same `Eq`.

what the equivalence does require is that the index contain every registered rule and that no rule's interface change after registration. neither is maintained; both are structural. the table owns the arena, `push` is the only way in, and there is no accessor handing out a mutable rule. an interface editable in place would leave the index naming a bucket the rule no longer belongs to, and the way to make that unreachable is to not offer it.

`crates/pith-core/tests/fuzz_selection.rs` holds the agreement to a generated population anyway, with the pre-0057 scan kept in the test as the reference model. that is the shape the sqlite adapter's conformance suite uses, and it is what makes the equivalence executable instead of asserted here.

### candidates are ordered on the failure path

0015 requires that the candidates a diagnostic names not depend on registration order, and the old implementation got that from sorting every selection. sorting belongs where the order is read: the ambiguous branch, which produces `E-1102` and ends the run. a bucket holding one rule returns it with no clone, no allocation, and no comparison — this is 0050's "the walk survives for the diagnostic" applied to the other predicate.

the tie-break is unchanged. the sort is by label and stable over registration order, which is what the old stable sort by interface-then-label degraded to inside a set of rules that all share one interface.

### the map is an `IndexMap` and its order is not observable

nothing iterates the map: a lookup returns one bucket and the bucket is a sequence. so this is not a determinism requirement the way 0050's per-chain set was, where the set was iterated to build a diagnostic. it is an `IndexMap` because 0021's guard forbids the alternative in crate source, and taking the ordered map costs less than arguing an exception for a structure whose cost is one hash either way.

## alternatives considered

### keep the scan and memoize its outcome per request shape

the CLOS position, and what production implementations of it do: cache the effective method against the argument classes that selected it, so a repeated call shape skips the search. the dispatch research note reads it from the standard's own procedure — select the applicable methods, then sort them — which is a search that must run at least once per distinct shape.

rejected because there is nothing dynamic here to memoize. CLOS caches because the applicable set depends on argument classes that vary per call and on a class hierarchy that can change; pith's population is registered before evaluation starts — `register_rule` takes `&mut self`, and so does every entry point, so no rule is registered while a scheduler is live, which 0050 already relies on. a cache over an unchanging population is an index built lazily, with an invalidation story it does not need and a first-call cost it cannot avoid.

### an approximate index, then a scan of what survives

GHC's rough map keys on the class name and the argument type constructor names and deliberately does not look further — "without poking inside the DFunId" — because resolving the full type would force interface files to load. SWI-Prolog hashes one argument, deepens to at most seven levels, and still linearly scans a small predicate. Julia's method table "splits up on the structure based on a left-to-right decision tree."

rejected because all three approximate for a reason pith does not have. each of those systems ranks candidates by specificity, so a narrowed set still has to be searched, and the index's only job is to make the search shorter. 0015 refuses to rank: a match is exact or it is not a match, so the bucket an exact key reaches is the entire outcome, including the ambiguity. taking a coarse key here would be the same code doing strictly less.

### sort the arena and binary search it

O(log n) comparisons instead of n. each of those comparisons is a full structural comparison of two interfaces, which 0047 made unbounded in size, against one hash of one interface for the map. it also gives the arena an ordering it does not have — ids are allocation order, the brand is what makes them safe, and re-sorting on insert would either move ids or need a second index, which is the structure this record already has.

### an index in the engine, beside `pure_rule_revisions`

0049 added exactly this shape: a map the engine maintains in `register_rule`, "indexed off `rules` so revalidating a recorded pure edge does not scan the arena."

rejected because selection lives in `pith-core` and the engine is not its only caller. an index in the engine would leave `pith-core` holding the population and the engine holding the view of it, with the invariant that binds them spread across two crates and maintained by hand in two registration paths. the table makes the drift unconstructible instead, which is the same reasoning 0049's map does not get to use because a revision map is not derivable from the arena's own contents.

## consequences

`RuleTable<K>` replaces `RuleArena<Rule<K>>` in the engine's `rules` and `action_rules` fields, and the free function `select_rule` is gone: the spelling is `table.select(request)`, and `SelectOutcome::into_result` takes the table so it can name candidates. `RuleArena` stays as the arena inside the table, so `RuleId` and its brand are unchanged and nothing downstream of an id moves.

registration clones the interface once, as the index's key. the arena remains the single source of a rule's contents, and the index holds ids into it and no copies.

the pure and action categories get this from one generic structure. selection was already generic over the effect category, and an action request pays the same lookup a pure request does.

`crates/pith-engine/benches/scale.rs` gains the crowded-arena shape, which is the first benchmark to vary the rule population instead of the graph. the other four shapes are the control: they register two rules and their numbers should not move.

### measured

`cargo bench -p pith-engine` on this host, release build, before and after, with the benchmark identical across both. two thousand requests at each population; `ms/request` is the column the claim is about.

| registered rules | scan, total | scan, ms/request | index, total | index, ms/request |
| --- | --- | --- | --- | --- |
| 0 | 3.9 ms | 0.002 | 4.0 ms | 0.002 |
| 256 | 5.2 ms | 0.003 | 3.9 ms | 0.002 |
| 1,024 | 9.0 ms | 0.005 | 4.1 ms | 0.002 |
| 4,096 | 27.3 ms | 0.014 | 4.1 ms | 0.002 |
| 16,384 | 104.8 ms | 0.052 | 4.5 ms | 0.002 |

the scan's per-request cost is linear in the population and its total is 23x the index's at sixteen thousand rules. the index's is flat: the residual rise from 4.0 to 4.5 ms across a population sixteen thousand times larger is not a per-request term, and at two thousand requests it is 0.25 microseconds each.

the four graph shapes are unchanged, which is what says the win is selection and not something the change happened to make cheaper elsewhere: deep-chain 45.6 to 45.3 ms at 16,000, wide-sequence 40.2 to 40.5 ms at 16,000, wide-fanout 18.2 to 18.3 ms at 4,000, reused-chain 14.5 to 15.4 ms at 4,000.

## unresolved

the population is append-only and this record assumes it. nothing removes a rule, so the index has no deletion path and the table offers no way to ask for one. a registry that could unregister — a language server reloading a domain, or the represented bodies of 0038 arriving from a file that changed — has to say what happens to the bucket and to the ids in it, and that is a record, not a method.

selection is now cheap and key derivation is not. the request path still encodes the interface and every input into a manifest and hashes it once per request, which 0050's second unresolved paragraph predicted 0047 would make more expensive. the crowded-arena shape at zero extra rules is where that constant lives, and nothing has attributed it yet: 0.002 ms per request is now mostly key derivation, arena allocation, and publication, and which of the three dominates is unmeasured.

interning the interface would remove the per-request hash of a structure 0047 allows to be large, replacing it with an id comparison, and would also give a request a cheaper thing to carry. it is not taken here because the measurement does not ask for it, and because an interner over interfaces is the kind of structure that wants a reason beyond one benchmark column.
