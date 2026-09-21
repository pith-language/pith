---
schema: design-doc/v1
id: decision-0015-interface-rule-selection
title: select rules by interface match and refuse ambiguity
summary: rule selection matches typed requests against declared interfaces; more than one match is an error, never a silent ranking
kind: decision
status: accepted
created: 2026-04-17
updated: 2026-09-21
tags:
  - rules
  - selection
  - capabilities
relations:
  informed_by:
    - research-build-systems
  depends_on:
    - decision-0007-tracked-dynamic-dependencies
    - decision-0003-explicit-effects
    - foundation-principles
  supersedes: []
---

# select rules by interface match and refuse ambiguity

> edit, 2026-09-21: superseded by [0082](0082-rules-are-called-by-name-and-functions-are-values.md).
> rules are now called by name, and behaviour reaches a library by being passed. selection by interface, `ask`
> and the ambiguity refusal are removed. the record below is kept as the history of the decision.

## context

the rules-and-graph design doc says rule selection returns one explained implementation or an ambiguity error, and that registration order has no semantic meaning. it does not say how the match is made or what happens when several rules could satisfy a request.

the kernel cannot select by name, because names live outside the typed model and would let first-party rules hide behind identifiers third parties cannot match. it cannot select by registration order, because that is already rejected as a conflict-resolution rule. what is left is matching against the typed interface a rule declares.

the harder question is the common case: two rules both satisfy `Source, Compiler -> Artifact`, both are valid, and neither is wrong. systems that tolerate this pick one silently through priority, score, or order. that is the failure mode this decision rejects.

## proposed decision

a rule declares the typed request it satisfies. selection matches a request against the declared interfaces of available rules.

a request matches at most one rule. if more than one rule matches, evaluation stops with an ambiguity error that names every candidate and the interface each one declared.

the caller resolves ambiguity by being more specific, not by the engine ranking candidates. narrowing the request, adding a constraint only one rule satisfies, or selecting a rule explicitly at the call site are the disambiguation mechanisms. each is a deliberate act visible in the source.

an interface is more than an output type: it includes required capabilities, input constraints, target platform, and the identity of any domain noun the rule operates on. that precision keeps accidental matches rare, and a rare accidental match is an error rather than a guess.

this is the rule-selection question named as the gate on milestone M-1 in the open questions list. the semantic prototype depends on it.

## alternatives considered

### select by output type only

a request carries an output type and the engine finds any rule that produces it.

simple to specify. too coarse to disambiguate, because everything that produces an artifact competes for the same request. the interface would have to grow back the precision this removes.

### select by explicit name

rules are named and requests name the rule they want.

deterministic and unambiguous. it moves selection out of the typed model and into identifiers, which lets first-party rules claim names third parties cannot reach. it also forces every caller to know implementation names rather than request shapes, which breaks composition across repositories.

### rank candidates by score or priority

multiple matches are normal and the engine picks the highest-priority one.

this is the model to reject. priority numbers and scores go stale: a rule gets a number once, the world changes, nobody updates the number, and the engine silently picks the wrong rule forever. the failure is invisible and untested.

### capability interface match

rules declare the capabilities they require and provide, and selection matches on those.

this is part of the selected direction. capabilities are one component of the interface. on their own they are not enough, because two rules can require and provide the same capabilities and still be different rules.

## consequences

every selection has one candidate or a reported ambiguity; the engine never silently chooses among valid rules. queries can explain why a rule was selected, and where ambiguity was resolved by the caller, the query points at the narrowing that did it.

interface quality is the main ongoing cost. a loose interface makes everything ambiguous and the system refuses too much; an over-specific one makes nothing compose.

most requests match one rule and evaluation proceeds without ceremony. ambiguity errors concentrate in the cases that actually need a decision, which is where the friction belongs.

first-party and third-party rules compete on the same interface. nothing in the kernel breaks the tie in favor of the project's own libraries. if a first-party rule and a third-party rule both match, the caller decides, or the request is ambiguous.

## measured

this record moves to `accepted`: the selection rule is measured in practice, and the prototype its
unresolved section asked for exists four times over.

the interface is the requested input types and the output type, matched by derived equality, and
[0057](0057-the-rule-index.md) made that a lookup into `IndexMap<Interface, …>` rather than a scan. four
domains and the frontend design have since put pressure on it, and every collision it predicted was
resolved by making the types distinguish the rules; none needed a rule that picks. xylem's two
content-producing rules collapsed to `() -> Blob` and collided as `E-1102` until `CSource`, `Object` and
`Executable` became distinct nominals. its generate and test rules share their input types and differ only
in output. stele's three text renders needed the same treatment.
[the frontend architecture](../planning/frontend/architecture.md) reports the fourth independent instance
before writing a line: `interface-of`, `bodies-of` and `index-of` over identical input lists, needing
three distinct nominal outputs.

no case has emerged where refusing was the wrong answer. every `E-1102` so far has been a design signal,
which is exactly what priority numbers and scores would have hidden.

## unresolved

two items stay open and neither blocks acceptance, because both are spellings rather than semantics and
both wait on a surface language that milestone M-13 delivers.

how explicit selection at a call site is expressed, and how it stays visible in provenance rather than
becoming a hidden preference, needs that notation; the design happens alongside the values-and-types work.
so does whether a request can carry a preferred interface to narrow the match, and how that differs from
naming a rule: the escape hatch must not become selection-by-name in disguise. until then there is no
call-site syntax to make a preference invisible in, and `EngineQuery::select` already answers which rule
serves a request and whether it is ambiguous.

what the unresolved section asked for and got: the exact fields of a rule interface needed a prototype,
and four domains supplied one.
