---
schema: design-doc/v1
id: decision-0068-published-surfaces-are-context-bound
title: a published module surface is a claim about one dependency context, and an added provider is not a minor change
summary: a consumer's ABI and interface surface both move when a dependency's public surface moves, with the consumer's source unchanged, so a registry publishes a surface together with the imported subject/ABI pairs and elaborator revision it was built against; an addition is potentially breaking, the break belongs to a consumer's closure rather than to the edited module, and a publisher's own check cannot detect it
kind: decision
status: proposed
created: 2026-09-06
updated: 2026-09-06
tags:
  - modules
  - registries
  - compatibility
  - identity
relations:
  informed_by:
    - planning-modules-distribution
    - planning-modules-registry
    - research-module-distribution
  depends_on:
    - decision-0015-interface-rule-selection
    - decision-0047-the-declaration-table
    - decision-0061-the-declaration-artifact
    - decision-0067-local-module-workspaces
  supersedes: []
---

# a published module surface is a claim about one dependency context, and an added provider is not a minor change

> settles two assumptions the module-system proposal carried into M-14: that a registry can hold one ABI
> per released version, and that an added declaration or rule is a minor change. both were written before
> 0067 put imported subject/ABI pairs inside the module ABI, and before anything measured what an addition
> does to a consumer. both are false, and the second fails in a way its classification did not anticipate.

## context

the proposal requires a registry's candidate metadata to carry a module's ABI, and classifies `pith diff`'s
verdicts so that "an added declaration or rule is minor". neither claim had an executable counterexample,
and both were about to become file-format promises: the candidate encoding needs to know whether one ABI
per version is meaningful, and the publication gate needs to know what an addition means.

0067 changed the ground underneath the first claim without the proposal being updated. a module's ABI and
its interface surface now encode a sorted, deduplicated set of imported subject/ABI pairs, which makes a
module's published identity depend on what it imported, not only on what it says. this record answers how
far that dependence reaches and what a registry may therefore publish.

0015 is the ground underneath the second: it refuses two rules matching one request rather than ranking
them, which is what makes an added provider dangerous. where that refusal happens decides who can see the
break.

## decision

### a published surface is a claim about one dependency context

measured: with a consumer's source bytes byte-identical across two runs, a public addition to its
dependency moves the dependency's ABI, the consumer's ABI, the consumer's recorded imported ABIs, and the
consumer's *interface surface*. what moved inside that surface is exactly the dependency's ABI digest
(established by redacting the digest from both encodings and finding the remainders equal). a dependency
body edit that changes nothing public moves none of them.

so a registry field holding one ABI or one surface per released version means "under the dependency context
this release recorded", never "under every resolution this release's requirements admit", and it
must not be read as a context-free property of the source.

a release therefore publishes source, plus a surface **together with its context**: the imported
subject/ABI pairs and the elaborator revision it elaborated against. a consumer may reuse the published
surface only when its own resolved context matches both. otherwise it re-elaborates the source, which is
the ordinary case and not an error. a different resolved context is not tampering, and normal local body
edits do not require republishing a surface or rewriting an immutable pin.

### an added provider is potentially breaking, and the publisher cannot see it

measured, in two halves that fall on opposite sides of the frontend boundary.

within one module, two rules providing one interface are refused at elaboration, `E-3012`, naming the
second provider and the first. a publisher cannot build that break, so cannot publish it.

across modules, a closure in which two dependencies each provide one interface **elaborates cleanly**
(`check` reports no errors and produces an ABI) and the consumer fails only when it evaluates, `E-1102`.

four consequences:

- an addition is potentially breaking. the unconditional minor classification is withdrawn.
- the break is a property of a consumer's whole closure, not of the edited module. neither dependency is
  wrong on its own; the pair is.
- a publisher's own `check` cannot detect it, because their module is fine. the party with the power to
  act has no signal.
- no comparison of two versions of one module can prove an addition safe, because the counterexample is
  not in either version.

### what a differ may claim, and what publication may gate on

`pith diff` reports three things and never merges them.

its **structural report** covers the module's own declarations, computed after subtracting the imported-ABI
region (otherwise a dependency's movement is attributed to the author's edit, which the measurement above
shows would happen for byte-identical source). its **context report** names a change in imported
subject/ABI pairs or elaborator revision as its own fact. its **consumer report** is optional evidence that
one named closure still selects and typechecks, and its claim is limited to that closure.

publication gates on a conservative classification. removals, nominal representation moves, constructor-set
changes, and interface changes are breaking. additions are potentially breaking and require the same bump
discipline rather than being waved through. formatting, declaration order, and alias edits are not changes
at all, which 0067 already guaranteed by keeping them out of every digest. an unchanged structural report
is not a compatibility proof, and the gate must not describe it as one.

## alternatives considered

**one context-free ABI per released version**, as the proposal assumed. rejected as measured false: the
registry field's meaning would silently vary with the consumer's resolution while looking authoritative.

**publish source only, no surface.** coherent, simpler, and it removes the context question entirely, but
it gives up the cross-module reuse the frontend graph tier was built for: a consumer can no longer skip
re-elaborating a dependency. retained instead as a cache entry carrying its context, reusable exactly when
the context matches and inert otherwise.

**keep additions minor and let consumers pin.** rejected. the measurement shows the party who could act
(the publisher) has no signal, so the policy would push a cost onto consumers who cannot anticipate it
either. conservative classification is the default the evidence supports when the break is invisible to
both sides until evaluation.

**refuse a two-provider closure at elaboration, moving `E-1102` to check time.** attractive, because it
would give the consumer the error at the boundary where they are already reading diagnostics. not decided
here: it requires the frontend to know the full closure's rule table, which is a larger change to the graph
tier than this record's evidence justifies. named as unresolved below.

## evidence

four tests, run on 2026-09-06. they are measurements first and regressions second; each names the claim it
settles.

| test | shows |
| --- | --- |
| `pith-loader` `probes::an_unchanged_consumer_gets_a_new_abi_when_its_dependency_surface_moves` | with the consumer's source asserted byte-identical, a dependency's public addition moves the dependency ABI, the consumer ABI, the consumer's imported ABIs, and the consumer's interface surface; redacting the dependency ABI digest makes the two surface encodings equal |
| `pith-loader` `probes::a_dependency_body_edit_leaves_the_consumer_abi_alone` | the converse, so the first result is context dependence and not sensitivity to any dependency edit |
| `pith-cli` `end_to_end::two_rules_providing_one_interface_in_one_module_are_refused_at_elaboration` | the intra-module break is `E-3012` at elaboration, so a publisher cannot build it |
| `pith-cli` `end_to_end::a_rule_added_to_one_dependency_breaks_a_consumer_that_imports_another` | the cross-module break elaborates cleanly, `check` passes, and the consumer fails at evaluation with `E-1102` |

what the evidence does not cover: every fixture is path
dependencies inside one workspace, with no registry, no versions, no solver, and no acquired content. the
cross-module fixture has exactly two providers, one interface, and one consumer. the redaction argument
shows the surface difference is confined to the dependency's ABI digest *for these encodings*, not that
every future surface encoding will isolate its imported region as cleanly. the selection probe runs against
cold engine state deliberately, so it measures selection and says nothing about revalidation.

## unresolved

whether a consumer's elaboration should refuse a closure carrying two providers of one interface, moving
`E-1102` to check time. it would put the error where the affected party is looking, at the cost of the
frontend needing the closure's rule table.

whether the rule table participates in an entry computation's key. while the selection probe was being
built, a run made after a dependency gained a competing provider returned a hydrated answer rather than
re-selecting. the probe was given cold state so it would measure one thing, and this observation was
deliberately not turned into a test asserting current behavior, because it is not yet known to be correct.
it belongs to the revalidation witness the local-workspace slice already owes.

how a published surface's context is encoded in a registry entry, and whether the elaborator revision
belongs beside the imported pairs or in the entry's own metadata. this record fixes the meaning; the
encoding waits for the entry format.

the third planned probe (withdraw an admitted, locked selection and replay the lock) remains unrunnable
until registry machinery exists. it is the obligation that decides whether a lock replays a selection
without replaying a judgment about it.
