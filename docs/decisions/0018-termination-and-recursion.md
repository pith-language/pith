---
schema: design-doc/v1
id: decision-0018-termination-and-recursion
title: total pure evaluation by construction, with cycle detection and a backstop limit
summary: the pure language fragment has no general recursion; repetition goes through the graph, which the engine drives and can check for cycles
kind: decision
status: proposed
created: 2026-04-22
updated: 2026-04-22
tags:
  - language
  - evaluation
  - termination
relations:
  informed_by:
    - research-configuration
  depends_on:
    - decision-0010-typed-pure-language
    - decision-0007-tracked-dynamic-dependencies
    - foundation-principles
  supersedes: []
---

# total pure evaluation by construction, with cycle detection and a backstop limit

## context

decision 0010 says ordinary declaration evaluation is terminating. the rules-and-graph design doc says incremental execution must remain equivalent to an empty-cache evaluation under the same declared inputs. these two commitments interact. if a rule could recurse without bound, evaluation might never return, and the equivalence to a clean build stops being a property the engine can claim.

the open questions list names this directly: should evaluation be total by construction, or can termination be checked with an explicit unsafe boundary. a recursion limit alone does not answer it, because a limit makes the same rule terminate on one input and fail on another, and the failure is arbitrary rather than semantic.

## proposed decision

the pure language fragment has no general recursion. repetition over finite data is expressed through structural recursion, folds, and similar total constructs, and through the dependency graph itself.

the graph is where recursion lives. a rule requests other rules, the engine drives the evaluation, and because every request passes through the engine, the engine sees the full dependency chain. a request that would depend on itself is a cycle the engine can detect and report as a structured diagnostic. the cycle is the real bound, and it is semantic: it names the actual loop, not a counter that happened to run out.

this makes the empty-cache equivalence hold by construction for the pure fragment. the structure of evaluation guarantees termination, because there is no language-level recursion that could escape the graph and the graph cannot be cyclic without the engine noticing.

a recursion or step limit exists only as a backstop on the impure paths, the bounded actions, observations, and mutations where work cannot be statically bounded. it is generous, it points at the runaway when it triggers, and it is never the primary defense against non-termination in pure code.

the escape hatch for cases that genuinely cannot be expressed is a visibly distinct construct, not a hidden flag that relaxes totality elsewhere. it stays marked at the call site, and its presence in provenance is obvious.

## alternatives considered

### recursion limit as the primary defense

allow general recursion and impose a step or depth limit.

simple to implement. the same rule terminates or fails depending on input size and the chosen limit, so correctness becomes contingent on a number. the failure is arbitrary and gives no useful diagnostic. this is the model this decision exists to reject.

### total by construction with no escape hatch

the pure fragment is total and there is no way to express anything that is not obviously terminating.

the strongest safety story. some real computations are total but not obviously so, and forcing them through a total-by-construction language can make them impractical to write. the escape hatch exists for these cases, marked so the weaker guarantee stays visible, consistent with how gradual adoption is handled elsewhere in the design.

### explicit unsafe termination boundary

allow general recursion behind a boundary the author asserts is safe, checked loosely or not at all.

flexible. it moves the termination burden onto the author and makes the engine's termination claim conditional on an assertion it cannot verify. this is the model decision 0010 already rejects for ordinary evaluation. the escape hatch here is narrower: a distinct construct for cases that need it, not a general relaxation.

## consequences

pure evaluation terminates by construction. the empty-cache invariant is a property of the structure, not a hope about input size. the engine can claim clean-build equivalence for the pure fragment without qualification.

recursion is a graph property. cycles are reported as structured diagnostics that name the requests in the loop, which is more useful than a depth-limit error that says nothing about which dependency closed the cycle.

some things that are easy to write as general recursion become verbose. a generator that derives many values, a search over a finite space, or a fixed-point computation has to go through the graph machinery rather than a loop. most of what a build, package, and configuration kernel does is finite-foldable over known data, so the cost is smaller than it sounds, but it is real.

the impure paths carry a backstop limit. actions, observations, and mutations can do work the engine cannot bound, and a generous limit there prevents a runaway from wedging the graph. the limit points at the offender when it triggers.

## unresolved

which total constructs the pure fragment supports is open. folds and structural recursion are the baseline. whether fixed points, generators, or bounded search are expressed as language constructs or as graph patterns needs design alongside the values-and-types work.

the cycle diagnostic's exact shape, and how it presents a long or dense cycle readably, needs a prototype.

how the impure backstop limit is configured, whether per action, per capability, or globally, and how it interacts with cancellation, is part of the action prototype milestone.
