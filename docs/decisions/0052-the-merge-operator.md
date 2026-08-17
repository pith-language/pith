---
schema: design-doc/v1
id: decision-0052-the-merge-operator
title: an explicit merge operator in the system library, with declared policy, no priority numbers, and a named escalation path
summary: record-shaped values compose through one operator in the library M-5a opens; conflict policy is declared at the merge site, disagreement fails, deliberate replacement is the only way a value wins, and the operator moves to the kernel when a second domain needs it
kind: decision
status: proposed
created: 2026-08-07
updated: 2026-08-07
tags:
  - composition
  - libraries
  - values
relations:
  informed_by:
    - research-system-composition
    - research-configuration
    - foundation-principles
  depends_on:
    - decision-0026-generic-typed-calculus
    - decision-0009-peer-first-party-domains
    - decision-0015-interface-rule-selection
  supersedes: []
---

# an explicit merge operator in the system library, with declared policy, no priority numbers, and a named escalation path

> takes the item [0026](0026-generic-typed-calculus.md)'s unresolved section defers — "the merge operator's signature, priority system, and conflict-to-`Conflicted<T>` promotion rule need design alongside the first configuration library prototype" — and the milestone framing that names requirements C-2, C-3, C-4 and C-7 as resting on one operator no milestone creates. 0026 stands; its calculus is unchanged. this record decides where the operator lives and what its conflict posture is, and leaves the exact signature to the prototype round M-5a opens.

## context

M-5a composes files, users, a service, and boot configuration into one immutable artifact. every one of those is assembled from several declared contributions: a unit from the fragments that declare pieces of it, an /etc from the parts that each own files under it, a user table from the modules that add accounts. the composition requirements name what the assembly has to guarantee: C-2, deterministic composition that fails on conflict unless an explicit operation handles it; C-3, replacement that identifies the target and its expected owner; C-4, composition that preserves types, provenance, and constraints; C-7, precedence and merge behavior that are deterministic and queryable.

0026 reserved exactly this mechanism in its composition section — "a merge takes two records and produces either a merged record or a `Conflicted<T>` that the engine surfaces; it does not silently pick a winner from import order. priorities, overrides, and conflict policy are declared at the merge site and recorded in provenance" — and deferred its design to the first configuration library prototype. no milestone creates a configuration library. the system library M-5a opens is the first place a domain needs one, which makes it that library.

the [system-composition research note](../research/system-composition.md) reads five systems that compose a filesystem image from declared parts and finds that the artifact's shape and the composition's conflict rule are independent choices. on the rule, the precedents split: NixOS discards definitions by numeric priority before a per-type merge; Guix refuses statically and pushes resolution to redeclaration outside the fold; CUE makes disagreement a lattice bottom and has no override; BuildStream fails on overlap unless the depending element whitelists the path; OCI lets extraction order decide silently. the operator has to take a position among these, and the position is what this record writes down.

one merge-shaped function exists in the tree today: phloem's `merge_provided`, over header sets. agreeing duplicates collapse; one spelling naming two contents refuses with a diagnostic naming both. that is the posture below, already running in one domain.

## proposed decision

### one operator, in the system library M-5a opens

the merge operator lands in the system library as an ordinary pure library function over `Value::Record`. it adds no kernel constructor and no engine hook, on 0009's ground: a domain carrying its own composition algebra privileges nothing and forces nothing on a peer that has no use for it.

the operator takes the set of contributions and a declared merge policy. the policy is a value — a closed set of declared behaviors, canonically encoded, entering the computation key as a declared input on 0023's terms, so two merges run under different policies are different computations. policy is declared at the merge site, as 0026's paragraph requires, and recorded in provenance.

the result is a merged record, or a diagnostic refusing the merge that names the field, both values, and both owners. at M-5a a conflict is a diagnostic; the promotion of a conflict to a `Conflicted<T>` value stays gated, per the consequences below.

### conflict posture: fail closed, order-independent

agreeing contributions collapse. disagreement over one field is a diagnostic refusing the merge. the merge is insensitive to assembly order: inputs are canonically sorted and the result is a function of the set of contributions plus the policy, which is C-2's determinism and C-7's queryability in one property. `merge_provided` already holds both halves — the agreeing collapse that the two-package fixture exercises, and the refusal it spells "one spelling cannot name two headers."

the precedents for refusing are Guix's fold and CUE's unification, read from their primary documents. Guix's fold raises "service '~a' provided more than once" and "duplicate '~a' entry for /etc" statically, before any artifact exists, and resolves nothing by ordering. CUE's specification defines unification as "the greatest lower bound of a and b," states it is "commutative, associative, and idempotent," and the project's own account of the design is that "The lack of override support is a language feature, enforced by the core evaluator." BuildStream's overlap rule is the same posture at path granularity: staging overlapping files "is normally an error," escaped only by an explicit, per-element whitelist.

### replacement is the only way a value wins

C-3's deliberate replacement is the explicit operation for "this contribution overrides that one": a replace names the field, the expected owner, and the new value, and fails when the field's ownership has changed underneath it. it is visibly distinct at the merge site, in the sense the principles' escape-hatch paragraph requires, and it is recorded in provenance like the policy.

there is no numeric priority, no force annotation, and no last-writer-wins. the arguments against each are in the alternatives below; the shared reason is that ownership is the fact a composition system must preserve, and every mechanism that resolves conflicts by number or by position spends it.

### the escalation path

one mechanism per concern sets both halves. the operator starts in the system library because that is where the first full consumer is. it moves to the kernel's value layer when a second domain needs record-shaped composition — phloem's header sets, or the deployment library M-6 opens — because at that point letting each domain implement its own merge is exactly the case scope.md names for kernel ownership: cross-domain composition that becomes impossible to explain. the move gets its own record, the domains adopt the one operator, and the convergence tracking M-4 began for constructors tracks operators the same way.

until the trigger fires, phloem's `merge_provided` stands as a domain-local function. the adoption is part of the escalation, named here so the deferral is explicit.

## alternatives considered

### numeric priorities: NixOS's mkOverride ladder, Nickel's priority annotations

NixOS attaches a number to each option definition; the manual states the rule: "A module can override the definitions of an option in other modules by setting an *override priority*," and "All option definitions that do not have the lowest priority value are discarded." the ladder in `lib/modules.nix` runs 10 (`mkVMOverride`), 50 (`mkForce`), 60 (`mkImageMediaOverride`), 100 (plain), 1000 (`mkDefault`), 1500 (option defaults), and the number itself is load-bearing: a module's definition can be erased by an integer in an unrelated module, with nothing in the result saying so. equal-priority scalar conflicts still error — "All definitions must have the same value, after priorities" — so the ladder is a second resolution mechanism beside the merge it modifies, which is the one-mechanism-per-concern defect. and the error text for a conflict suggests reaching for `mkForce` or `mkDefault`, which is the ladder reproducing its own pressure: where numbers resolve conflicts, the numbers accrete.

Nickel ships the same resolution with a different surface: priorities are annotations on values, defaulting to 0, and "If the priorities differ, the value with the highest priority simply erases the other." any Nickel number is a valid priority, including fractions. erasure with a cleaner spelling is still erasure.

the posture has an in-repo ancestor. 0015 rejects ranking ambiguous rule candidates on the ground that "priority numbers and scores rot," and 0019 cites that refusal again when it declines a category field on one effect type, calling it the same failure. adopting a priority ladder for composition would reintroduce, at the merge boundary, the mechanism 0015 refused at the selection boundary.

rejected because C-3 already names the honest form of a value winning: replacement that names the target and checks its owner. a priority number is replacement without the check, and the discarded definition leaves no trace for C-7 to query.

### unification as the whole merge: CUE

CUE's merge is unification, and its specification grounds it: "All possible values are ordered in a lattice," and unification is the greatest lower bound in that order, commutative, associative, and idempotent, with conflicts arriving as the bottom value.

the posture is adopted wholesale: disagreement is an error, the operation is order-independent, and no override exists at any price. the mechanism is not adopted, for one reason. unification's reach comes from subsumption — a concrete value refines a constraint because `2 ⊑ int`, defaults are disjunctions whose starred branch is an instance of the whole — and that requires a value model in which types and values share one partial order. 0026's calculus is exact by construction: `is_type` accepts with no width or depth subtyping, records are closed, and the record's own argument keeps matching exact because computation keys and rule selection digest types as they are spelled. a library operator over closed records keeps everything this record wants from CUE and builds none of the lattice. partial adoption, stated as such.

### order-based merge: systemd drop-ins

systemd composes a unit from its main file plus drop-ins, "merged in the alphanumeric order and parsed after the main unit file itself has been parsed," with equally named files resolved by directory hierarchy: "equally named drop-in files further down the prefix hierarchy override those further up."

the precedent documents its own costs. list directives like `After=` "cannot be reset to an empty list, so dependencies can only be added in drop-ins. If you want to remove dependencies, you have to override the entire unit," and the reset idiom for resettable lists is assigning the empty string. ordering decides conflicts silently, so removing what an earlier fragment added costs a workaround with its own semantics.

rejected on the principles' own sentence: "registration order is not an acceptable conflict-resolution rule." filename order is registration order after a sort, and the merge site learns nothing about why a value won.

### the operator in the kernel now

landing the merge in pith-core immediately would make it available to every domain on day one, and the kernel already carries the value algebra it operates on.

rejected on 0047's gating discipline: landing a mechanism without the subsystem that reads it is what produced the `Type::Nominal` history, where a constructor existed for two milestones with nothing able to inhabit it. the operator's first full consumer is the system library M-5a opens; phloem's header merge is a narrower, domain-local shape today. the escalation trigger is named in this record rather than left implicit, so the deferral is a decision with a condition, and the condition is checkable: a second domain, or the adoption of phloem's merge, is the event that reopens placement.

## consequences

M-5a composes through one mechanism at both granularities it needs. units are records merged under a declared policy. files and directories are the same algebra at path granularity: a file set is the sorted-list spelling 0040 already fixed for keyed values, and an overlay disagreement refuses the way `merge_provided` refuses and BuildStream's overlap errors — one path naming two contents is a conflict, whitelisted replacement aside.

C-2 is discharged by the posture, C-3 by replacement, C-7 by the policy value and the order-independence property. C-4 is discharged in the half provenance carries: the policy and the per-field ownership record are what composition preserves; the exact provenance shape is prototype work named in unresolved.

`Conflicted<T>` stays unbuilt. at M-5a a conflict is a diagnostic refusing the merge — the Guix-fold posture, and the shape `merge_provided` already has. promoting a conflict to a value the engine could dispatch on remains gated on the subsystem that would read it structurally, which is 0047's gate applied to 0026's deferred promotion rule: no reader, no constructor.

no encoding version moves, on 0048's rule: the policy is a new declared input shape, pre-release incompatibility is a rebuild, and no retained byte sequence changes meaning.

0026's composition paragraph is instantiated. the global magic merge stays rejected; the explicit, provenance-carrying operator that paragraph reserved now has a placement, a posture, and a condition for moving.

### measured

this round lands documents; the plan that scoped it excluded code. the merge-shaped code in the tree is phloem's `merge_provided` (`crates/phloem/src/build/rule.rs`): its agreeing path is what carries a dependency's headers into the dependent's compile in `crates/phloem/tests/two_package_build.rs`, and its refusal — the posture this record generalizes — has no test driving it today. the prototype round that lands the operator owes three tests this record names so its claim is checkable against this file: a conflict test (two contributions, one field, the diagnostic naming both owners), a replacement test (a replaced field whose ownership changed fails), and an order-insensitivity test (a permutation of the contributions yields the same canonical result).

## unresolved

the policy constructor set and the operator's exact signature: the closed set of merge behaviors the library ships — equality-merge for scalars, concatenation for lists, recursive record merge, the file-set overlay spelling — belongs to M-5a's prototype round, with one-mechanism-per-concern as the tiebreaker if file composition and record composition turn out to want different operators.

the conflict-to-`Conflicted<T>` promotion rule, in 0026's words, stays open behind 0047's gate: which subsystem would read a conflicted value structurally, and what it would do differently, is the question that decides when the constructor lands.

the provenance shape for per-field ownership: what a merge records about where each field came from, so C-4's preservation and C-7's queryability have a concrete answer.

the operational criterion for escalation: whether phloem's header merge counts as the second consumer at its current shape, or only once restated over records; and whether the escalation target is pith-core's value layer or a first-party foundation crate the domains share — decided by where the second consumer lives, tested against scope.md's criterion.
