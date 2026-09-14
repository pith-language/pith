---
schema: design-doc/v1
id: decision-0071-host-bodies-bind-through-host-adapters
title: a host body binds through a host adapter named in the module's manifest, and its revision is derived from the artifact
summary: pith's step protocol, action contract and value encoding are the host contract; a host adapter turns an artifact and a module's declarations into engine rules; the manifest names the adapter and artifact; a host declaration lists the interfaces its body may request; and a rule served by an artifact takes its revision from the artifact's content
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - extensibility
  - identity
  - libraries
relations:
  informed_by:
    - research-wasm-components
    - research-extension-interfaces
  depends_on:
    - foundation-principles
    - requirements-usability
    - decision-0004-first-party-without-privilege
    - decision-0023-rule-and-cache-identity
    - decision-0038-represented-rule-bodies
    - decision-0061-the-declaration-artifact
    - decision-0065-entry-evaluation-and-the-cli-query-surface
  supersedes: []
---

# a host body binds through a host adapter named in the module's manifest, and its revision is derived from the artifact

## context

a module written outside this repository can declare types and rules in `.pi`, and its represented pure
bodies run. its `= host` declarations do not. `bind_module` in `crates/pith-query/src/program.rs` binds
each of them to a diagnostic reading "the CLI links no domain crate", and 0065 lists host implementation
binding as unresolved. the only host code pith can run is rust compiled into the binary, so only domains
in this repository can have host bodies. 0004 and U-10 require that first-party code has no path that
third-party code lacks, and this is one.

host revisions are also maintained by hand. a host rule carries a `BodyRevision(u32)` that its author has
to bump when the body's meaning changes. if the author forgets, caches keep serving results computed by
the old body, and nothing detects it.

this record defines how any host body is bound. [0073](0073-the-component-interface-is-generated-from-declarations.md)
through [0076](0076-component-execution-is-deterministic-and-bounded.md) define the first adapter, for
WebAssembly components.

## decision

### the host contract is pith's

the host contract is what the engine already has: the step vocabulary of `PureStep` (`Need`, `NeedAll`,
`NeedBlob`, `NeedAction`, `NeedObservation`, `Complete`) and `Resumption`, the `plan` and `complete` pair
of `ActionRule`, the declared interfaces of 0061, and the canonical value encoding. none of these refers
to a particular host technology.

### a host adapter produces engine rules

a host adapter is the part of pith that takes an artifact and a module's declarations and produces the
engine's `PureRule` and `ActionRule` implementations. the WebAssembly component adapter is the first.
linked rust crates become the second, bound through the same path, so the in-tree domains are no longer
loaded differently from other modules. the rust adapter binds only crates compiled into the binary and
cannot be named from a manifest.

each adapter lives in its own crate. the engine sees the rules it produces, the run bound it already has
from 0059, and a class of host faults. a new adapter is a new crate and does not change `.pi` files or
engine types.

### the manifest names the adapter and the artifact

a module's `module.pi` says which adapter serves its host declarations and where the artifact is:

```
module acme/render 0.1.0

host wasm-component from path "host/render.wasm"
```

the adapter is named explicitly and not inferred from a file extension. a module has at most one `host`
clause.

the path is relative to the module root, and the file is part of the module's source content, admitted
and identified the same way as its `.pi` files. the module's content identity covers it, so the manifest
does not repeat a digest. locators outside the module, such as a URL or an OCI reference, come with the
transport that fetches them, and the lock pins their digest.

each `= host` declaration binds to the artifact's export for its coordinate. an artifact missing an export
for a host declaration, or with an export no declaration names, is refused at load, before any rule is
registered.

### a host declaration lists what it may request

the engine can see every request a represented body makes, because the body is data. a host body is
opaque, and nothing currently says what it may ask for. a host declaration therefore lists the interfaces
its body may request. the list is part of the rule's revision and not of the module's ABI, since callers
depend on a rule's answer and not on how it is computed. the notation decides how the list is spelled;
this record only requires that it exists and is complete.

an adapter must make an undeclared request impossible or refuse it. the component adapter does it at
link time (0073); the rust adapter checks each request against the list at run time.

### the revision is derived from the artifact

the revision of a rule served by an artifact is a digest over the adapter's name and protocol version,
the artifact's content identity, the export name, the declared request list and the interface. a rule
cannot change meaning without changing revision, because the revision is computed from the bytes that
implement it. `BodyRevision` remains only for the rust adapter.

this is coarse. any change to an artifact changes the revision of every rule it serves, and their results
are recomputed. the engine's existing equality pruning limits how far that goes, since a dependent whose
inputs come back equal is reused.

### domains meet through requests

an artifact cannot link against another domain's artifact. domains reach each other through requests,
which the engine records, caches and can explain; linking artifacts directly would bypass all three.

## alternatives considered

keeping `BodyRevision` for artifacts would let an author ship a changed component under an old revision.
the digest is available, so there is no reason to trust a hand-maintained number instead.

inferring the adapter from the artifact's file extension or magic bytes would work for one adapter and
turn a second one into a guess.

letting each host declaration name its own artifact would allow one module to mix adapters. nothing needs
that yet, and one `host` clause per module keeps the revision rule and the load-time checks simple.

## evidence

the component witness in [0073](0073-the-component-interface-is-generated-from-declarations.md) covers
this record. for binding specifically:

- rebuilding the component changes the revisions of its own rules and no others, and an unchanged
  dependent is reused
- a host declaration without an export, and an export without a declaration, are each refused at load
  with a diagnostic naming both sides

## unresolved

the spelling of the request list belongs to the notation.

whether the in-tree domains move to components, stay on the rust adapter, or become `.pi` with
represented bodies is open. with the rust adapter, none of them has to move for this record to hold.
