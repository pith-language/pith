---
schema: design-doc/v1
id: decision-0072-action-plans-are-authorized-against-consumer-grants
title: an action plan is authorized against grants written by the consuming project, and no module has authority without one
summary: the root project's manifest grants each module the host programs, toolchain paths, capabilities and network policy its planned actions may use; there is no default grant for any module, including the root and the first-party domains; a dependency cannot grant; grants replace AllowAllActions and apply to every plan regardless of what produced it
kind: decision
status: proposed
created: 2026-09-29
updated: 2026-09-29
tags:
  - security
  - capabilities
  - configuration
relations:
  informed_by:
    - research-wasm-components
  depends_on:
    - foundation-principles
    - requirements-security-and-trust
    - decision-0028-sandboxed-local-executor
    - decision-0069-module-authority-is-consumer-configuration
  supersedes: []
---

# an action plan is authorized against grants written by the consuming project, and no module has authority without one

> edit, 2026-09-29: amended by [0080](0080-inputs-are-parameters.md). grants are written on the input
> they apply to, in the project file. the rules below stand: only the root project grants, no module has a
> default grant, and a dependency cannot grant.

## context

the engine has an authorization step. `ActionPolicy::authorize` in `crates/pith-engine/src/policy.rs`
sees every `ActionPlan` before an executor runs it, and can return `Allowed` or `Denied`. the cli passes
`AllowAllActions` (`crates/pith-query/src/entry.rs`), so every plan is allowed.

that has had no visible effect while every rule came from this repository. once a module from elsewhere
can plan actions, whether through a represented body or a host adapter (0071), it conflicts with the
principle that authority is explicit and with T-3, which asks that rules and adapters receive only the
capabilities the current request needs.

two systems in [the research note](../research/wasm-components.md) got this wrong in different ways. Zed
shipped extensions without grants, reverted its first design the day after it landed, and added user-side
grants about eighteen months after launch, with a warning that restricting them "will likely make many
extensions non-functional". Spin denies by default, but a dependency either gets no resources or, with
`dependencies_inherit_configuration`, gets all of them.

## decision

### grants are per module and written by the root

the root project's manifest holds grants. a grant names a module and what the actions planned by that
module's rules may use:

- host programs and toolchain paths an `ActionSpec` may name
- capabilities, by name and scope
- network policy

a plan is authorized when every host path, capability and network requirement in it is covered by the
grant for the module whose rule planned it. a plan from a module with no grant is refused.

### there is no default

no module has authority without a grant. that includes the root project's own rules and the first-party
domains. the first-party domains get no implicit grant because 0004 gives them no privilege, and the
root gets none so that everything a project allows is written in one place.

### a dependency cannot grant

a dependency's manifest cannot grant anything, to itself or to another module. 0069 draws the same line
for registry routing, because the consumer cannot review authority that a dependency extends. a grant
clause in a dependency's manifest is diagnosed when the dependency is loaded.

### grants replace AllowAllActions

the cli authorizes plans against the root's grants. `AllowAllActions` remains for tests of the engine
itself.

grants apply to a plan whether a represented body, a component or the rust adapter produced it. the
engine authorizes the plan itself, and how the plan was computed does not change what it may use.

### refusals say what is missing

a refused plan reports the module, each requirement not covered, and the grant that would cover it, which
can be copied into the manifest. the authorization, allowed or denied, is recorded in
the action's provenance, as T-3 requires of capability scopes.

### grants that become imports

a grant that a host body uses directly, such as an observer's access to one path, would reach a component
as an import. when that happens, each such grant is a separate named import using WASI 0.3.1's
`implements` annotation, so the component's type lists what it was given. no grant takes this form in the
first version; components import only `pith:core` and their declared requests (0073).

## alternatives considered

keeping `AllowAllActions` until a registry exists would leave path dependencies, the only kind that works
today, with full authority, including any module copied into a project from elsewhere.

a grant set declared by the module itself and approved by the user is Zed's current model. the dependency
writes the first draft of its own authority, and approval prompts tend to be accepted without reading. a
grant in the consumer's manifest is committed text that goes through the consumer's review.

implicit grants for the first-party domains would make the first run of `xylem` simpler, but they are the
privilege 0004 rules out.

## evidence

- the component witness plans its render action only after the root grants it the renderer program; with
  the grant removed, the plan is refused and the diagnostic names the missing program
- a grant clause in a dependency's manifest is diagnosed at load and has no effect
- a plan's provenance records the grant it was authorized under

## unresolved

the manifest spelling of grants belongs to the manifest grammar.

whether a grant can be narrower than a host path, for example a pattern over arguments as Zed allows, is
left until a domain needs it.

grants for observations and mutations wait on those effects having host bodies.
