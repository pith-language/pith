---
schema: design-doc/v1
id: decision-0056-peerhood-is-a-registered-crate
title: peerhood is proven by a domain crate the kernel does not name, not by a plugin interface
summary: requirement U-10's "tests prove that an external library can replace or extend it without hidden hooks" becomes an actual test; a domain crate registers one pure rule and one action rule through `register_rule` and `register_action_rule`, depends on no other domain, is named nowhere in the workspace, and gets reuse, hydration, and contract inspection from that alone; a dynamic plugin ABI is refused with its reason stated
kind: decision
status: proposed
created: 2026-08-17
updated: 2026-08-17
tags:
  - libraries
  - kernel
  - testing
relations:
  informed_by:
    - research-extension-interfaces
  depends_on:
    - decision-0004-first-party-without-privilege
    - decision-0009-peer-first-party-domains
    - decision-0021-arena-graph-engine
    - decision-0047-the-declaration-table
  supersedes: []
---

# peerhood is proven by a domain crate the kernel does not name, not by a plugin interface

> takes the requirement 0009 and 0004 both rest on, which 0021 names as its design criterion ("a new domain implementable without a core patch"), and supplies the evidence U-10 asks for in its own text and the workspace never had. 0009 and 0004 stand. what changes is that their central claim stops being an argument about the interface and becomes a test that fails when the claim stops being true.

## context

U-10 is one sentence and its second half has never been true: "each first-party domain library uses public kernel interfaces. tests prove that an external library can replace or extend it without hidden hooks." the first half is well evidenced. xylem declares its types and registers its rules through the same public surface any crate can reach, and M-5a's statement quotes 0021's design test as the thing the next milestone exists to check. the second half names a test that does not exist.

what stood in for it was two domains. xylem is independent of everything but the kernel; phloem is a package library above it, and `crates/phloem/Cargo.toml` makes xylem a hard dependency, with roughly twenty-five use sites of `xylem::types::`. that is the architecture 0009 predicts, since values compose across domains and a package library consuming a build library is one of its examples. it is still not evidence for the peerhood claim: both crates are first-party, both are in the workspace, and either could have grown a hook without anything failing. the evidence base for "an external library could do this" was one domain and an argument.

the research note beside this record reads five systems at that question and finds the argument form is the weak one. Bazel announced in 2020 that its native rules would move onto the Starlark surface and completed the move in Bazel 8.0 in december 2024: four years in which the extension surface was public, the claim was made, and the built-in rules were still java in the binary. Buck2, PostgreSQL, and Nix all reach parity by construction. there are no rules in the buck2 binary, the built-in types are catalog rows, and the only primitive that reaches the outside is `derivation`. pith cannot make that structural argument. the kernel is rust, the domains are rust crates compiled into the same workspace, and nothing stops a kernel crate from naming a domain. so the argument left is the one none of the five ships: a test.

the note also separates two things this decision has to keep apart. parity is whether an extension reaches what the built-ins reach. isolation is whether an extension can damage what it should not. Terraform answers isolation with a process boundary and a versioned gRPC protocol, and pays by fixing what a provider can be: the resource graph, the plan, and state management stay in core permanently, and a provider defines resources and data sources inside a shape it cannot extend. that trade is the one this record has to decide against explicitly, because "peer domains" and "plugin system" sound alike and are different.

## proposed decision

### peerhood is a claim about the interface, and a crate is the proof of it

a domain is a peer if it can declare its own types, register its own rules through the public engine, and get the engine's properties from that alone. the proof is `crates/example-domain`: one pure rule, one action rule, its own declaration table under the module identity `example`, a dependency list of kernel crates and nothing else, and no mention of its name anywhere else in the workspace.

it renders a text template. the pure entry reads the template's bytes with `PureStep::NeedBlob` and checks that every placeholder the text spells is bound and that no name is bound twice, then requests the action; the action plans a contract that runs a renderer program named by content (0036) over the staged template, with the bindings as arguments. the domain is small and unlike the first-party ones on purpose. nobody should use it as a library. its subject is the registration surface.

it registers with `Engine::register_rule` and `Engine::register_action_rule`, the same two calls `register_xylem` makes. what it gets for that is measured below: an action planned from its own contract, a result served from the reusable index on the second request, the same result hydrated by a later process over the same durable state, a request whose canonical inputs make two orderings one computation, and a planned contract readable through `EngineQuery::plan_action`. none of it required a change to the engine, and the test that says so reads the tree instead of trusting the diff.

### the refusal: no dynamic plugin ABI, and this is why

pith will not grow a plugin loader. no dynamic library ABI, no out-of-process provider protocol, no registry of extension points a domain declares itself into. an extension is a crate that depends on the kernel and calls its public functions, and the workspace boundary is a packaging fact with no semantic weight.

the reason is the note's finding stated as a decision. a wire protocol answers isolation, which is not what 0009 and U-10 ask about, and it answers it by fixing the shapes an extension may take, which is the outcome 0009 exists to prevent: Terraform's providers cannot add a kind of thing to the plan graph, and pith's premise is domains whose values are as first-class as any other. it would also be a second mechanism for a concern that already has one, against the principles' one-mechanism rule, and it would be built with no consumer, the failure mode 0047 diagnosed in `Type::Nominal` and 0055 refused to repeat for division.

the underlying need is deferred, not denied, and the trigger is nameable: a plugin boundary becomes the right answer when a domain must be loaded by a pith binary its author cannot rebuild. that condition is a distribution question, it does not exist while everything is a pre-release cargo workspace, and it arrives no earlier than the surface language or a shipped binary.

### what the test asserts, and what it cannot

the tree-reading test asserts that no crate in the workspace and no file under `xtask` contains this crate's name in either spelling, and that this crate's manifest names no other domain. both sets are derived: the crate's own name from cargo, the domain set from the crates directory minus the `pith-` prefixed kernel crates. a third domain added later is covered without an edit here.

what it cannot assert is that no hook exists under another name. a kernel that special-cased the module identity `example`, or that grew a facility only this domain uses, would pass. the test catches the failure that actually occurs, where a domain gets carried by adding it to something in the kernel. the failure it cannot catch is the one a record has to keep naming.

## alternatives considered

### a dynamic plugin interface, Terraform's or PostgreSQL's shape

let a domain be a shared library the engine loads, either in-process behind a stable C ABI as PostgreSQL loads a type implementation, or out-of-process behind a versioned protocol as Terraform runs a provider.

rejected, and recorded here so the refusal is explicit. the out-of-process form answers isolation and fixes the extension's shape permanently: Terraform's own documentation puts "Construction of the Resource Graph" and "Plan execution" among core's responsibilities and leaves a provider to "Define managed resources and data sources", which contradicts 0009's peer claim at the mechanism level. the in-process ABI form answers nothing pith is asking. PostgreSQL loads shared libraries because its extensions are written outside its build; pith's are cargo dependencies, so an ABI would buy a stability problem in exchange for a distribution property nothing needs yet. both are surface with no consumer.

### keep arguing it from the first-party domains

leave the evidence as it stands, since xylem uses only public interfaces and so an outside crate could, and treat U-10's test sentence as satisfied by the existing suites.

rejected on Bazel's four years. a public surface is compatible with the built-ins not using it, and with a privilege appearing later that nobody notices because nothing fails. this repository already produced the smaller version of that drift: M-4 was declared to land no kernel constructor while `Record` and `Sum` went in on the day it was committed. an argument that cannot fail is not evidence, and U-10 asked for a test in its own text.

### a `DomainLibrary` trait every domain implements

give domains a trait covering declarations, rules, diagnostic range, and registration, so the engine can enumerate them and a conformance suite can hold them to one shape.

rejected as the privilege it would create. the surface under test is `register_rule` and `register_action_rule`. a trait beside them would be a second registration mechanism, and the moment the engine knows what a domain is, being a domain is a status the engine grants. the conformance idea is right and belongs elsewhere: `pith-engine`'s `testing` feature already exports the cross-adapter suite that holds engine-state adapters to each other, which is how an *adapter* gets held to a contract. a domain has no contract beyond the rules it registers, and that is 0009's claim.

### move phloem off xylem so two independent first-party domains exist

break the `xylem` dependency in `crates/phloem/Cargo.toml` and make the package library independent, so the workspace has two unlayered domains.

rejected because it would answer a different question and damage a correct design. phloem builds packages through xylem's compile and link entries, which is 0045's measured result and the composition 0009 predicts; removing it would either duplicate a build library or reintroduce one behind an indirection. a layered domain is evidence that domains compose. no amount of it is evidence that an outside one can register.

### put the test inside pith-engine's `testing` feature

write the fixture domain as an engine test, using the existing feature-gated test surface.

rejected because the crate boundary is the assertion. what a peer must not need is a change to the kernel, and a test inside the kernel cannot fail for the presence of one: an engine-private fixture has whatever access the crate has, `pub(crate)` included. cargo's dependency direction is the check being run, and it exists only between crates.

## consequences

the workspace has a third domain and a second independent one. the evidence base for 0009's peer claim is now two crates that depend on the kernel and on nothing else, plus one layered on xylem, which is the shape 0009 describes.

carrying it cost the workspace two lines: the member entry in `Cargo.toml` and the lockfile entry cargo derives from it. no crate changed. that number is the claim, and the tree-reading test keeps it true, since a later hook for this domain in the engine or the cli or xtask fails a test instead of passing review.

two holes in the peer surface became visible by being walked into, and both are recorded here without being fixed. the first is diagnostic codes. `pith-diag` documents a 1000-based engine namespace and reserves a 2000-based composition namespace, and says nothing about the rest, so xylem stamps 9002, xylem's own fixtures 9003, phloem 9004, and this domain picked 9005 by reading the others. an outside domain has no way to pick a range that will not collide, which leaves K-11's stable codes stable per crate and unallocated across them. 0053 named this in its unresolved section; this round is the second domain to hit it and the first from outside the first-party set.

the second is smaller and affects the first-party domains equally. `Engine` exposes `put_blob` and no way to read a blob back, so a domain that wants the bytes behind its own result opens a second handle on the content store the engine already owns. the tests here do that, as xylem's do. it is a gap in the public surface, not a privilege, and naming it gives the next round that touches the engine's content surface a reason to close it.

the cli is unaffected in a way worth writing down. `crates/pith-cli/Cargo.toml` depends on no domain at all, so "no change to pith-cli was required" holds for this domain and equally for xylem and phloem. there is no domain-facing command surface, so whether the cli privileges a domain cannot be tested yet. that question arrives with the first command that runs one.

### measured

the crate is 620 lines of non-test source across three modules: `types.rs` 238, `rules.rs` 321, `lib.rs` 61, plus 494 lines of tests and 123 of a fixture executor. its `[dependencies]` are `pith-core`, `pith-diag`, `pith-ids`, and `pith-engine`; its dev-dependencies add `pith-store`, `pith-state-sqlite`, `async-trait`, and `tempfile`. it names neither xylem nor phloem, and `this_domain_depends_on_no_other_domain` asserts that against the domain set it derives from the crates directory instead of a written list.

`no_other_crate_in_the_workspace_names_this_domain` reads every rust source and manifest under `crates/` and `xtask/`, excluding this crate, and fails if either spelling of its name appears. it passes against one hand-written line outside the crate — the workspace member entry — plus the lockfile entry cargo derives from it.

the peerhood suite is eight tests, each measuring a property the first-party domains are measured on. a cold render is `Computed` and produces the expected bytes through a filesystem content store and a sqlite engine-state database. the second request for the same document is `Reused` with the executor's execution count still at one, so a peer's completed computation enters the reusable index (0031, 0033) with no registration beyond its two rules. a fresh engine opened over the same root after the first is dropped reports `Hydrated` and executes nothing, so a peer's recorded attempt revalidates and loads across a process boundary (0024). bindings listed in the two possible orders are one computation, because the value constructor sorts and the computation key is over canonical inputs: one request under two spellings, the second served from the index. a request naming a different renderer is `Computed` and not served, because the program's content identity reaches the contract (0036). an unbound placeholder fails with the domain's own diagnostic code and zero action computations in the graph, so a check the pure rule can make is made before anything is planned. a name bound twice is refused with the name in the message. and `EngineQuery::plan_action` returns the planned contract, with the renderer as `ActionProgram::Content` and the bindings as canonical arguments, so K-12's inspectability reaches a domain the engine does not know about.

the suite is host-agnostic. it supplies its own `Executor`, which is a public trait, and never drives `pith-executor-local`, so it compiles and runs on any host. eight suites in the workspace are `cfg(target_os = "linux")` at file scope and compile to nothing elsewhere. the cost of that choice is stated plainly: this round measures the registration surface and does not measure a real program under confinement. xylem's suites already do, and no part of this claim depends on it.

## unresolved

the diagnostic-code allocation the consequences name. two first-party domains and one outside one now stamp codes in an undocumented range by inspection of each other. what the rule should be — a per-module range the registration boundary assigns, a coordinate-derived code, or a documented allocation table — is a K-11 question. it is cheap now and a compatibility break at four domains.

whether an outside domain can supply an executor, a content store, or an engine-state adapter as easily as it supplies rules. all three are pith-owned public traits, and the evidence for all three is first-party: `pith-executor-local`, `pith-store`'s two adapters, and `pith-state-sqlite`. the conformance suite exists for the state store, so that one is closest to answerable. the executor's is hardest, because confinement is the part of the contract an executor claims rather than implements.

whether this crate stays a fixture or acquires a second reader. it is a workspace member and CI builds it, which is what keeps it honest, and nothing depends on it. a later round wanting a domain to demonstrate something else — the merge operator across two domains, a second module's declarations meeting the first's — puts it here. whether a fixture domain that accumulates features is still a fixture is a question to answer then.

the hook the test cannot see: a kernel facility added for one domain and named generally. nothing in this round addresses it, and the mitigation is the one M-5a already states. a kernel change a domain demands gets argued in a record instead of assumed into the diff.
