---
schema: design-doc/v1
id: decision-0069-module-authority-is-consumer-configuration
title: a domain is routed to a registry by consumer configuration, a registry entry is a derived cache of the manifest it pins, and a locator is a hint outside the identity
summary: registry bindings and domain routing are written by the consumer and never by a dependency, an index entry restates nothing it cannot re-derive from its pinned revision so a disagreement is a refusal rather than an authority rule, where a revision may be fetched from is not part of what it is, and the key-set generation number is a rollback counter exempt from 0048's pin-at-1 rule
kind: decision
status: proposed
created: 2026-09-06
updated: 2026-09-21
tags:
  - modules
  - registries
  - security
  - identity
  - configuration
relations:
  informed_by:
    - planning-modules-registry
    - planning-modules-registry-integration
    - planning-modules-distribution
    - research-registry-supply-chain-security
  depends_on:
    - decision-0042-binary-reuse-as-admitted-substitution
    - decision-0044-the-first-source-adapter
    - decision-0048-pre-release-version-pinning
    - decision-0067-local-module-workspaces
    - decision-0068-published-surfaces-are-context-bound
    - requirements-security-and-trust
  amends:
    - decision-0048-pre-release-version-pinning
  supersedes: []
---

# a domain is routed to a registry by consumer configuration, a registry entry is a derived cache of the manifest it pins, and a locator is a hint outside the identity

> edit, 2026-09-21: under [0080](0080-inputs-are-parameters.md) locators are written in the project
> file's `inputs` block. the rules here, that routing is consumer configuration and never inherited, apply to
> that block unchanged.

> amends [0048](0048-pre-release-version-pinning.md): the rule that every pre-release format version stays
> pinned at 1 applies to *format versions*. a monotonic rollback counter is not a format version, and the
> domain key set's generation number is exempt. the distinction is recorded here, before an encoder exists
> to inherit the wrong rule: a format version says how to read bytes, and freezing it during pre-release
> costs nothing; a rollback counter says which of two readable states is later, and freezing it disables
> the mechanism it exists to be.

## context

[0067](0067-local-module-workspaces.md) settled what a module *is* (a declared `domain/name` subject with
a source tree) and deliberately left who may declare a domain unanswered, because a local declaration is
a claim inside one loaded program and nothing more. that was safe while every dependency was a path. it
stops being safe the moment a subject can be satisfied by content that arrives from somewhere.

[The registry](../planning/modules/registry.md) argues the destination, on the incident evidence in
[the supply-chain research](../research/registry-supply-chain-security.md): a signed, append-only,
metadata-only index whose every cached field derives from the revision it names. this record settles the
contracts that acquisition, publication, and the lock encoders inherit from it. it fixes meanings, not
implementations; [module distribution](../planning/modules/distribution.md) sequences the code.

three of these have to be settled *before* an encoder exists, because each is a decision that a format
silently makes if nobody makes it deliberately: whether an index may disagree with its source, whether a
fetch location is part of an identity, and whether a rollback counter is a format version.

## decision

### routing is consumer configuration, and it is never inherited

a domain is bound to a registry by the consumer's configuration. there is no search order, no default
fallback, and no discovery. a subject whose domain no configuration mentions does not resolve, and the
diagnostic lists the bindings that are configured rather than reporting a missing package.

configuration lives in two places with one precedence rule: the project's root manifest, and the user's
own configuration. a project binding wins over a user binding for the same domain, and the collision names
both declarations rather than being silently resolved. two bindings for one domain *at the same level* are
refused at configuration load, before any dependency I/O.

registry names are local to each configuration layer. a domain route is bound to that layer's registry
before precedence is applied; reusing a registry name in the project cannot retarget an otherwise
unchanged user domain route. a project domain therefore names a registry declared by the project. explicit
`from registry name` requirements consult the merged names, with project precedence and the same collision
report. the selected registry and domain declaration each retain their owner and source location.

a **dependency's manifest can never supply, replace, or extend** a registry binding, a domain route, or an
admission policy. a dependency's `registry` and `domain` clauses parse (they are grammatical, and a module
that is a root in its own checkout will have them) but they are ignored for authority and diagnosed when
that module is loaded as a dependency. this is the same authority boundary
[integration](../planning/modules/registry-integration.md)'s eighth claim draws around policy, and the
reason is identical: authority a dependency can extend is authority the consumer cannot audit.

the distribution may ship an explicit binding for the first-party domains with its root key. it is
configuration, appears in `pith` output like any other route, is overridable, and is refused under `strict`
operation like any other user-level binding. what must never exist is a route that applies without
appearing anywhere.

### an index entry is a derived cache, so a disagreement is a refusal

the manifest at a release's pinned revision is the single owner of that release's subject, version, and
requirements. the index **caches** those fields, signed, so a resolver can read requirements without
fetching every candidate. it does not restate them as a second authority.

therefore there is no authority rule to write. when an entry's cached subject, version, or requirements
disagree with the manifest at the revision the entry itself pins, the entry is **refused**, naming the
field and both values. it is never reconciled by preferring one side.

this is the point at which pith declines a rule other systems have needed. Bazel's registry documentation
states that resolution reads the registry's module file, which may differ from the source archive's (a
defined winner for a fact with two owners). that rule is only necessary when the index is authored. because
every field here is derivable from a self-authenticating revision, any party can re-derive and compare, so
a disagreement means the cache is wrong.

the cost is stated rather than hidden: a resolver that re-derived every candidate's requirements before
trusting them would lose the reason the cache exists. so **the publisher's signature is what a resolver
trusts at solve time, and re-derivation is what publication, audit, and any third party perform**: the
signature is what resolution itself depends on.

### a locator is a hint, outside the identity

where a revision may be fetched from is not a fact about the release. a remote URL names a host, a
protocol, and an organization's current arrangements (realization, not meaning). an append-only public
contract that baked one in could never fix a forge migration.

so a locator is carried beside an entry, overridable by consumer configuration, and it reaches neither the
release's identity nor the lock. the revision's own content identity and the measured tree digest are what
a consumer checks, which makes the publisher's forge, an internal mirror, and a local cache
indistinguishable when honest and refused when not.

two consequences follow. mirroring is a configuration line rather than an index
rewrite. and a fast path (a forge's archive endpoint against a clone of the same revision) is
interchangeable rather than a second lock shape, which is how the `github:` wart, where a shorthand and a
git URL fetch and lock differently, is unreachable from here.

### the trust chain is four checks, each refused distinctly

a consumer's configuration pins a registry's **root key**. nothing vouches for that key but the
configuration naming it; this is 0044's first-use gap, inherited unchanged and stated rather than hidden.

the root key signs **domain key sets**: for each domain the registry carries, the public keys permitted to
sign its releases, a threshold, and a monotonic generation number. a domain with no key set at a registry
cannot receive releases there, which makes publishing authority data rather than an account system.

every **release line** is signed by a key in its domain's set. the registry operator's own keys do not
appear in this chain, so a compromised operator cannot mint a release for a domain whose keys it does not
hold.

the consumer records the index state it admitted, **per subject**, and on the next read every entry it
previously saw for that subject must still be present and byte-identical. new entries may be appended. a
removed or altered entry is a fork. recording it per subject is what keeps the cost proportional to a
project's dependencies rather than to the registry's size, and what lets a sparse read support the same
check as a full clone.

these four are distinct refusals, not one failure. a key absent from a key set, a key set not signed by the
pinned root, a generation that moved backwards, and a previously seen line that changed are four different
facts about the world, and collapsing them hides whether an incident is an attack or a misconfiguration.

### the generation number is a rollback counter, not a format version

a key set's generation advances on every change, and a consumer refuses a key set whose generation moved
backwards. this is the mechanism that stops an adversary from replaying an older key set to reinstate a
retired key.

0048 pins pre-release format versions at 1 so that nothing spends effort on migrations before a release
exists. applied to a generation number that rule would pin the counter, which does not defer a migration:
it deletes the mechanism. the amendment above states the general distinction so the next monotonic counter
inherits the right rule: 0048 governs how bytes are read, not which of two readable states is later.

withdrawal is an append, never an edit, for the same reason. a yank that mutated a line would be
indistinguishable from a fork under the append-only check.

### publication proposes; it uploads nothing

`pith publish` derives an entry from the manifest at an existing, immutable revision, measures the
normalized tree at its subpath, signs the result, and proposes it to the index. merging appends the
registry's admission time.

nothing is uploaded, so there is no upload credential, so there is nothing for a staged-publishing split to
protect. a compromised publishing pipeline can at most open a proposal against a reviewed index.

two refusals belong to publication itself. a module containing a **path dependency** cannot be published:
a path is a live local input that makes no witnessed-content claim, so a release containing one would name
requirements no consumer can satisfy. and an entry whose derived fields disagree with the manifest at its
pinned revision is refused at proposal as well as at read, so the wrong-cache refusal is not something only
consumers discover.

### admission is a value, not a boolean

the result of admitting a source is the uncertainty calculus 0026 already settled, shared with 0042's
binary offers through one refusal type with a clause per claim:

| outcome | meaning |
| --- | --- |
| admitted | every clause checked against a current index |
| `Stale<T>` | admitted against withdrawal knowledge older than policy allows (the ordinary offline case) |
| `Unreachable` | the index could not be refreshed at all, distinct from a registry legitimately having no such entry |
| `Conflicted<T>` | the index disagrees with the consumer's recorded prior state, or two routes claim one subject |
| `Unchecked<T>` | a path dependency, deliberately unwitnessed, carried visibly rather than silently |

a run may proceed on `Stale` or `Unchecked` (that is why they are modelled instead of refused) but the
weaker guarantee stays visible in the value, in the lock, and in the CLI, and a consumer policy may refuse
any of them. T-5 requires that sharing the machinery not merge the claims: one `Refusal` type with a clause
per claim, never one boolean five checks can set.

## the refusal table

each refusal is one specific, source-bearing diagnostic at its own boundary. "its own boundary" is the
point of the table: folding any of these into a later generic check is what makes an attack and an
unreachable mirror look the same.

| refusal | where | code |
| --- | --- | --- |
| two registry bindings for one domain at one level | configuration load, before I/O | E-3044 |
| a registry binding whose root key is malformed | parse | E-3045 |
| a dependency manifest declaring a registry, domain route, or admission policy | load, as the dependency is bound | E-3046 |
| no registry configured for a required domain | resolve, listing configured bindings | E-3047 |
| a release line signed by a key absent from the domain key set | acquisition | E-3048 |
| a domain key set not signed by the pinned root key | acquisition, distinctly from E-3048 | E-3049 |
| a key set whose generation moved backwards | acquisition, distinctly from a signature failure | E-3050 |
| a previously admitted index line now absent or altered | acquisition | E-3051 |
| an entry whose cached subject, version, or requirements disagree with the manifest at its pinned revision | acquisition and publication | E-3052 |
| fetched bytes whose normalized tree disagrees with the entry's digest | acquisition | E-3053 |
| an index that cannot be refreshed at all | acquisition, as `Unreachable` | E-3054 |
| a withdrawn selection reached through an existing lock | run, naming issuer and reason | E-3055 |
| a consumer policy rule refusing a release the registry admitted | admission, pointing at the consumer's rule | E-3056 |
| publishing a module containing a path dependency | publication | E-3057 |

## alternatives considered

**a default registry with a search order.** every ecosystem that has one has had dependency confusion.
domain binding with no fallback removes the attack structurally rather than defending against it, and the
cost (a project must name a registry before it names a dependency from one) is paid once per project in
configuration a person can read.

**an authority rule for index-versus-source disagreement.** rejected above. it is the correct design when
an index is authored and the wrong one when every field is derivable, and adopting it would make a
disagreement a supported state rather than a bug.

**a locator inside the entry.** simpler for a first implementation, and it removes a configuration step.
rejected because an append-only entry can never be edited, so a forge migration would be unfixable for
every release already published, and because it makes a mirror a second lock shape instead of a transport.

**TUF's four metadata roles, a transparency log, and a witness quorum.** stronger on paper against a
registry operator serving targeted forks. rejected as machinery nobody will operate: metadata-only
publication removes the upload the staging split protects, a pinned self-authenticating revision removes
the artifact a build attestation covers, and a cloneable index removes the log server witnesses hold
accountable. the upgrade path stays additive: witnesses can cosign an index state later without changing a
line's format. what is lost is stated in [the registry](../planning/modules/registry.md)'s threat model:
targeted forks are weakly detected, by a mirrored public index rather than by protocol.

**automated appends instead of reviewed proposals.** self-serve from day one, and the obvious scaling path.
rejected for the first registry because it returns the one property this model buys over a conventional
service: a stolen publisher key reaching the index with no second party. it should be adopted
deliberately, with the loss stated, rather than by drift.

## evidence

the routing portion has executable evidence in
[measured](../planning/measured.md#m-14-consumer-configuration-and-route-agreement), and the trust
chain now does too: [the signed-index slice](../planning/measured.md#m-14-the-module-registry--the-signed-index-slice)
implements the root key over domain key sets, key-set generations, per-line signatures, and the
consumer's per-subject append-only record as four distinct refusals, with key rotation and
generation-rollback fixtures and the wrong-cache and content-identity refusals at acquisition. the
signature scheme this record left open is settled by that encoder: ed25519, detached, over each
line's canonical rendering, spelled `ed25519:<hex>` (the recommendation above, confirmed before
any index bytes existed outside fixtures). the record stays proposed until the lock round trip runs
its claims; the two probes that *have* run belong to
[0068](0068-published-surfaces-are-context-bound.md) and constrain what a registry may publish about a
released version rather than how it is authorized.

the third planned probe (withdraw an admitted, locked selection and replay the lock) remains unrunnable
until the lock document exists. it is the one that decides whether a lock replays a selection without
replaying a judgment about that selection's fitness, and it belongs to the slice that builds the
round trip.

## unresolved

**signature format and key distribution.** the scheme is settled (ed25519, detached, per line, per
the signed-index slice above) but key distribution is not: how a publisher's key reaches a person,
and what the first-party registry's enrollment story is, remain open alongside bootstrap below.

**threshold semantics.** a key set carries a threshold and the client verifies one publisher
signature. whether the first registry requires multi-signature releases or merely permits them is
undecided, and it is the strongest available answer to a stolen publisher key.

**withdrawal authority.** a publisher withdrawing their own release is obvious. a registry withdrawing
someone else's, for malware, is a different authority, and both must be expressible without letting a
registry silently redefine what a domain's keys mean.

**bootstrap.** the first-party domains resolve before any registry exists; their trust root is the
checkout. saying so keeps bootstrap from quietly becoming a bypass, but the boundary between "shipped in
the checkout" and "routed like anything else" is not yet drawn.
