---
schema: design-doc/v1
id: planning-modules-registry
title: the module registry
summary: a registry is a signed, append-only, metadata-only index in a Git repository — the registry says which bytes, Git says these are those bytes — with eight facts, four mechanisms, consumer-written policy, and an explicit list of what it refuses to become
kind: planning
status: draft
created: 2026-09-06
updated: 2026-09-06
tags:
  - planning
  - modules
  - registries
  - security
relations:
  informed_by:
    - research-module-distribution
    - research-registry-supply-chain-security
    - research-index-formats
    - research-artifacts-and-trust
    - research-sources
  depends_on:
    - planning-modules-distribution
    - planning-modules-system
    - planning-modules-registry-integration
    - requirements-security-and-trust
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
    - decision-0042-binary-reuse-as-admitted-substitution
    - decision-0044-the-first-source-adapter
  supersedes: []
---

# the module registry

**the registry says which content. the content proves it is that content.**

a registry is a signed, append-only index of module releases, every field of which is *derived from the
source it names*. it stores no module content, runs no builds, holds no accounts, and answers no queries.
an entry says: for this subject at this version, the module is the tree measured to this digest, taken
from this subpath of this immutable revision, with these requirements.

verifying a registry is therefore re-deriving it, and everything below is a consequence of that.

a git repository is the first realization of both halves: of the index, because a commit history is a
convenient signed append-only file tree, and of the revision pin, because a commit hash is a content
identity that authenticates itself. neither is part of the meaning. the index is *an append-only signed
file tree*, which a directory or a static HTTP mount serves equally well; the pin is *a self-authenticating
content identity*, of which a git commit is one. keeping that separation is what stops a forge from
becoming part of a public contract.

this is a plan, not an accepted decision, and nothing here is implemented or measured.

## why this shape

three properties of pith, not general truths, make the usual registry unnecessary.

a module is `.pi` **source**. there is nothing to build at publication, so there is no build to compromise,
no artifact to sign as distinct from its source, and no rebuilder needed to check that the two agree.

a source revision is **self-authenticating**. 0044 already settled this: git object names are content
hashes, so a commit witnesses its own content. a registry that names a commit has delegated content
integrity to a mechanism that cannot lie about it.

identity is **domain-bound**. registries are appointed per domain and never searched, so an index has no
namespace to police, no search order to rank, and no confusion attack to prevent at read time.

together these mean the registry's only irreducible job is to be a **trustworthy statement of which
revision is which version**, plus the requirements needed to resolve without fetching everything. that is
a small job, and this document exists to keep it small.

## the index is derived, not authored

the manifest at a pinned revision is the single owner of a release's meaning: its subject, its version, and
its requirements are declared there, in `.pi`, once. the index does not restate them as a second authority.
it **caches** them, signed, so a resolver can read requirements without fetching every candidate.

this matters because the alternative is a standing disagreement. bazel's registry documentation says
plainly that resolution reads the registry's module file, which *may differ* from the source archive's,
a defined authority rule for a fact with two owners. under "define meaning once" that is not a rule pith
can adopt. here there is no winner to pick: the index names an exact revision and subpath, so any party can
re-derive every cached field and compare. a disagreement means the cache is wrong, and the entry is
refused, not reconciled.

everything in an entry except the publisher's signature and the registry's admission time is derivable
from the pinned revision. that is the design's central property, and the executable refusal below that
mutates one cached field and expects a refusal is what keeps it true.

## the facts

a registry provides facts, not judgments. every policy (cooldowns, signature thresholds,
allowed origins) is the consumer's, written in `.pi`, over these facts. that factoring is what keeps the
registry from growing into a policy engine with opinions its users cannot read.

| fact | what it is | owner |
| --- | --- | --- |
| subject | `domain/name`, matching 0067's identity | derived from the manifest |
| version | the release's version in the subject's declared scheme | derived from the manifest |
| requirements | the direct dependency subjects and ranges, sufficient to resolve without fetching | derived from the manifest |
| revision pin | an immutable, self-authenticating content identity, plus the subpath holding the module root | the release |
| tree digest | the measured identity of the normalized module tree at that subpath | derived from the revision |
| publisher signature | over the whole entry, by a key in the subject's domain key set | the publisher |
| admission time | when the registry admitted the entry, asserted by the registry, not the publisher | the registry |
| withdrawal | a later append marking a version unfit, with a reason and an issuer | the issuer |

one field is deliberately not in that table. **where a revision can be fetched from is not a fact about the
release.** a remote URL is realization: it names a host, a protocol, and an organization's current
arrangements, none of which are part of what the module means. baking one into an append-only public
contract would make a forge migration unfixable, because the entry can never be edited.

so a locator is a *hint*, carried beside the entry and overridable by consumer configuration. a consumer
may fetch a revision from the publisher's forge, an internal mirror, or a local cache; the revision's own
content identity and the tree digest make all three indistinguishable when they are honest and refused
when they are not. this is also the entire answer to mirroring: a mirror is a configuration line, never an
index rewrite.

note what is absent: no archive, no checksum of an archive, no yank flag that mutates a line, no ABI field
invented for the index, no popularity, no ownership records beyond the domain key set, no vulnerability
data, no scanner verdict. each of those either belongs to another mechanism or is a judgment a consumer
should make.

the advertised interface surface is deliberately absent too. per
[integration](registry-integration.md)'s seventh claim, a published surface is the frontend's own artifact
and is a cache entry, not an index field, reusable only when its dependency context and elaborator
revision match, and re-derivable from the source when they do not.

## the four mechanisms

four, and they are the entire security design. this replaces an earlier draft that stacked TUF's four
roles, a transparency log, witnesses, staged publishing, cooldowns, and a revocation snapshot. each of
those answered a real attack, but six independent mechanisms that nobody operates protect nothing while
looking like protection.

### 1. a pinned root key

the consumer's configuration names a registry and pins its root key, on exactly the terms 0042 pinned
origins and 0044 pinned a checkpoint. the root key signs domain key sets and admission records. nothing
vouches for the pinned key but the configuration naming it; this is a first-use trust gap, stated rather
than hidden.

### 2. domain key sets

`domains/<domain>` lists the public keys permitted to sign releases for that domain, along with a
threshold and a **monotonic generation number**. it is signed by the root key. a domain a registry does
not carry a key set for cannot receive releases there, which is publishing authority as data rather than
as an account system.

the generation number is a rollback counter. it advances on every change, and a consumer refuses a key set
whose generation went backwards. this is the one place 0048's pin-format-versions-at-1 rule must not
apply, and the distinction (format version versus rollback counter) belongs in the opening record before
any encoder exists to inherit the wrong rule.

### 3. signed release lines

every line in `index/<domain>/<name>` is signed by a key in that domain's set. a consumer verifies the
signature against the key set, and the key set against the root key. the registry operator's own keys do
not appear in this chain: a compromised operator cannot mint a release for a domain whose keys it does not
hold. that is PEP 480's property, obtained with one signature format instead of four metadata roles.

### 4. append-only, verified by the consumer

this mechanism replaces the transparency log.

a consumer records the index state it last admitted, **per subject**. on the next read, every entry it
previously saw for that subject must still be present and byte-identical. new entries may be appended. a
removed or altered entry is a fork, and it is refused by name.

append-only is therefore **checked by the consumer against its own memory, not asserted by the host**.
that is what a transparency log is for, obtained without a log server, checkpoints, inclusion proofs, or a
witness quorum.

recording it per subject rather than over the whole index is what keeps this from becoming a cost that
grows with the registry. a consumer verifies the subjects it actually depends on, so a project with twelve
dependencies pays for twelve subjects no matter how large the registry becomes, and a sparse index (one
subject file fetched at a time) supports exactly the same check as a full clone, so no project has to
clone an ever-growing history to build.

the honest comparison: a real witness network detects a registry serving *different* forks to *different*
consumers, which a single consumer's own record cannot. what replaces it here is that the index is a git
repository anyone can clone, and the sources it names are public commits anyone can check. detection
becomes a social property of a widely mirrored repository rather than a protocol guarantee. that is
weaker, and it is the deliberate trade.

## the index

one directory of subject files, one line per event, append-only.

```
<registry>/
  root                     the root key, self-describing, pinned by consumers
  domains/<domain>         signed key set: keys, threshold, generation
  index/<domain>/<name>    signed release and withdrawal entries, append-only
```

one file per subject is what makes the per-subject check above cheap and a sparse read possible: a
consumer fetches `index/example/greeter` and nothing else.

a release line names the version, source pin, subpath, tree digest, requirements, publisher signature, and
the registry's admission time. a withdrawal line names an existing version, an issuer, and a reason. there
is no third kind of line, and no line is ever edited or removed.

the format stays line-oriented and boring: a static host can serve it, a person can read it, and `git
diff` explains a change. [integration](registry-integration.md)'s first claim keeps `.pi` for what a
person writes and a wire format for what a machine serves; this is that boundary.

## the short path, and the fallback it is not

requiring every project to appoint a registry and pin a root key before it can name a single dependency is
the kind of ceremony that principle warns against. the distinction that resolves it is the one
[the module system](system.md) actually makes: what it refuses is a *silent* fallback to a default
registry, because a silent fallback is the dependency-confusion attack.

an explicit, shipped default is a different thing. the distribution may carry a binding for the first-party
domains with its root key, provided the binding is written in configuration a person can read, appears in
`pith` output like any other route, is overridable, and is refused under frozen operation the same way any
user-level binding is. deterministic, inspectable, fail-closed. what must never exist is a route that
applies without appearing anywhere: a domain no configuration mentions still fails, and it fails naming
what is configured.

## publication uploads nothing

`pith publish` produces a signed release line for a commit that already exists, and proposes it to the
index repository. that is the entire publication protocol, and it is the largest single simplification in
this design: **there is no upload, so there is no
upload credential, so there is nothing for a staged-publishing split to protect.** the 2026 incidents in
the [research](../../research/registry-supply-chain-security.md) turn on a compromised build pipeline
holding a credential that can release; a pipeline that can, at most, open a pull request against a
reviewed index is a much smaller problem.

review of that proposal is the authority gate, and it is two-party review, SLSA's strongest source
control, obtained from the forge that already implements it rather than from a staging service pith would
have to build and operate. a reviewer checks one thing that no automation can check for them: that the
commit named is the commit intended.

merging appends the admission record with the registry's time. a curated registry with reviewed proposals
is the credible first operating model, and it is also the only one that has this property; the automated
alternative is named in the alternatives below.

## acquisition, and the boundary it respects

four steps, and the split between them is 0044's, unchanged.

**read** the index from a local clone. this is a caller-side effect. verify the root key's signature on the
key sets, each key set's generation against the recorded one, each release line's signature against its
key set, and the whole index against the previously admitted state.

**resolve** as a pure rule over the resulting candidate universe, through the engine API peers use.
requirements come from the index, so no candidate is fetched to discover what it needs. the solver
performs no I/O; removing network access changes whether a universe could be read, never a solved answer.

**fetch** each selection's pinned revision, extract the subpath, normalize the tree, and measure it.
compare the measurement against the line's tree digest. normalization executes nothing: no install or
publication hook, no native host code, no executable git filter or clean/smudge program, no submodule
hook, no editor or agent configuration found in a dependency tree. a bounded file-kind allowlist, and an
explicit refusal for everything else.

**admit**, producing the record described in [integration](registry-integration.md)'s fifth claim: content
identity, evidence, policy, time context, and reason; recomputable, never a stored trusted bit. it shares
`crates/phloem/src/substitution/`'s `Admission` and `Refusal` with 0042's binary offers, one clause per
claim, so T-5's separate claims stay separate while the machinery is shared.

the result is not a boolean, and not an admit-or-refuse pair. it is a value in the uncertainty calculus
0026 already settled, because a module's admission is exactly the kind of external fact that principle
warns against dressing up as trustworthy:

| outcome | meaning |
| --- | --- |
| admitted | every clause checked against a current index |
| `Stale<T>` | admitted against withdrawal knowledge older than the consumer's policy allows — the ordinary offline case, and it must say so rather than look clean |
| `Unreachable` | the index could not be refreshed at all; distinct from a registry legitimately having no such entry |
| `Conflicted<T>` | the index disagrees with the consumer's recorded prior state, or two configured routes claim one subject |
| `Unchecked<T>` | a path dependency, deliberately unwitnessed, carried visibly rather than silently |

a build may proceed on `Stale` or `Unchecked`; that is the point of modeling them instead of refusing.
the weaker guarantee stays visible in the value, the lock, and the CLI, and a consumer policy can
refuse any of them. collapsing these into one flag is the defect this table exists to prevent.

withdrawal is read from the same local index, so an ordinary run consults it without network access. the
freshness of a consumer's withdrawal knowledge is the freshness of their index clone, and `update` is what
refreshes it. this is why there is no separate security snapshot service: a withdrawal is an append,
and it reaches a locked consumer by the same mechanism everything else does, without changing the lock,
because the lock records the selection and the index records its fitness.

## the threat model

0044's five adversaries stand. this restates the set against the model above, and the last four rows are
the floor.

| adversary | position | what does it |
| --- | --- | --- |
| dependency confusion | **prevented** | domains bind to registries, never searched, no default fallback |
| code execution during acquisition | **prevented** | source modules, bounded normalization, no hooks of any kind |
| stolen upload credential | **does not exist** | nothing is uploaded; a stolen CI token can at most open a pull request |
| stolen publisher key | **contained** | it can propose; review is the gate, and the threshold in the key set can require more than one |
| registry operator substitutes a release | **prevented for signatures, detected for admission** | the operator holds no domain key, so it cannot forge a line; it can merge a badly-reviewed one, and the source pin makes that checkable by anyone |
| registry rewrites its own history | **detected** | the consumer's own append-only check, against its recorded prior state |
| rollback of publishing authority | **prevented** | monotonic key-set generations, refused when they move backwards |
| a compromised source forge | **integrity yes, availability no** | the revision identity and tree digest are both pinned, so a forge can withhold a module but cannot change it; another locator serves the same content |
| a malicious publisher publishing honest, malicious code | **not addressed** | nothing here helps; withdrawal is a kill switch after detection, not a safety claim |
| a compromised host | **not addressed** | unchanged from 0044 |
| targeted forks served to one victim | **weakly detected** | a widely mirrored public index and publicly re-derivable entries, not a witness protocol |
| an index entry that quietly disagrees with its source | **prevented** | every cached field is derived, so a disagreement is a refusal rather than an authority question |
| first-use trust in a registry's root key | **not addressed** | the configuration naming the key is the only thing vouching for it |

the availability row is the price of choosing metadata-only, and it deserves its name: if an upstream
repository disappears, a module becomes unfetchable. what it cannot become is *different*. mirroring is
therefore a pure transport concern: any organization can clone the sources it depends on and configure
them as an origin, and the tree digest makes the mirror unable to lie. availability is an operational
problem with an operational fix; substitution would have been a security problem with none.

## what it refuses to become

the list is as much a part of the design as the mechanisms, because each entry is a plausible next feature
that would make the registry a second system.

- **not a content host.** it stores no module bytes. archiving may be added later for availability; it is
  never the integrity mechanism, because the tree digest already is.
- **not an identity provider.** keys are data in a signed file. no accounts, no login, no OIDC issuer
  relationship, no forge holding compiler privilege.
- **not a build service.** nothing is compiled, executed, or transformed at publication.
- **not a namespace authority.** a domain key set is the whole ownership model. who may claim a domain in
  a given registry is that registry's administration, not a global service.
- **not a vulnerability database, scanner, or advisory feed.** withdrawal carries a reason; interpreting
  the ecosystem is somebody else's product.
- **not searchable.** no query API, no ranking, no default registry, no discovery. `git grep` on a cloned
  index is the search, and it is enough.
- **not mutable.** no line is ever edited or deleted. yanking is an append.

## alternatives considered

**a registry service with staged upload, TUF metadata, and a witnessed transparency log.** this was the
previous draft. it is stronger on paper against a compromised registry operator serving targeted forks,
and it is the right design for a registry distributing *binaries*, where content must be hosted and builds
must be attested. it was rejected here because metadata-only publication removes the upload the staging
split exists to protect, the pinned source revision removes the artifact the build attestation exists to
cover, and a cloneable index removes the log server the witnesses exist to hold accountable. six
mechanisms answering attacks that three structural choices had already removed is not defense in depth; it
is machinery nobody will operate. the upgrade path is real and additive: witnesses can cosign an index
state later without changing a line's format.

**a git index with automated appends.** same substrate, same append-only property, self-serve from day
one, and a bot signs the merge instead of a person reviewing it. rejected for the first registry because
it gives back the one thing this model buys over a conventional service: a stolen publishing key reaching
the index directly, with no second party. it is the obvious scaling path once the first registry's volume
justifies it, and it should be adopted deliberately, with the loss stated, rather than by drift.

**exact revision dependencies with no index at all.** every dependency names a commit; no versions, no
requirements, no solver. this is the smallest possible system and it is genuinely coherent. rejected
because M-14 promises transitive range resolution, and because a diamond then has no mechanism to agree on
one version of a shared dependency: every consumer resolves that by hand, forever.

**registry-stored content.** what cargo and npm do, and the answer to a deleted upstream. deferred rather
than refused: the format can name a stored copy without one existing, and the tree digest means an
archived copy is a cache, never an authority. build it when an operated registry has an availability
requirement, not before.

## executable refusals

each fails at its own boundary, with a source-bearing diagnostic.

| fixture | required outcome |
| --- | --- |
| two registries claim one domain | refused at configuration load, before any I/O, naming both declarations |
| no registry serves a required domain | refused at resolve, naming the domain and listing configured bindings |
| a dependency manifest declares a registry binding or an admission policy | ignored for authority and diagnosed; both are consumer configuration |
| a release line signed by a key absent from the domain key set | refused, naming the key and the set |
| a domain key set not signed by the pinned root key | refused, distinctly from a bad release signature |
| a key set whose generation number went backwards | refused as rollback, distinctly from a signature failure |
| a previously seen index line that is now absent or altered | refused as a fork, naming the line and both states |
| fetched bytes whose normalized tree disagrees with the entry's digest | refused at acquisition, naming both identities |
| an entry whose cached subject, version, or requirements disagree with the manifest at its pinned revision | refused as a wrong cache, naming the field and both values; never reconciled by preferring one side |
| the same revision fetched through two different locators | identical admission records; a locator never reaches the identity or the lock |
| a module tree with a symlink escape, a traversing path, a special file, a duplicate entry, or an executable Git filter | refused by the normalizer, naming the entry |
| a published module containing a path dependency | refused at publication |
| a withdrawn selection in an existing lock | refused at run with the issuer and reason named, the recorded selection unchanged, no silent substitution |
| the same lock replayed offline | `Stale`, admitted with the withdrawal knowledge's age visible in the value, the lock, and the output |
| a path dependency in the closure | `Unchecked`, visible in every surface that reports admission |
| an index that cannot be refreshed at all | `Unreachable`, distinct from an entry that legitimately does not exist |
| a consumer policy rule refusing a release the registry admitted | refused, pointing at the consumer's own rule |
| an unreachable network during a locked run of already-acquired content | succeeds; only `update` needs the network |

## open questions

1. **signature format and key distribution.** one scheme, chosen once. signed git commits reuse forge
   machinery but bind the model to git and to a keyring; detached per-line signatures work over any
   transport, including a static mirror, and are the recommendation. confirm before an encoder exists.
2. **the pinned root key's first use.** 0044's gap, unchanged and inherited. a registry key distributed in
   the pith release is one answer for the first-party registry and no answer for anyone else's.
3. **threshold semantics.** a key set carries a threshold, but the model above verifies one publisher
   signature. multi-signature releases are the strongest available answer to a stolen publisher key; decide
   whether the first registry requires them or merely permits them.
4. **re-derivation cost.** verification is re-derivation, which means fetching a revision to check an
   entry's cached requirements. a resolver that must fetch every candidate to trust its requirements has
   lost the reason the cache exists. the likely answer is that the signature is what a resolver trusts at
   solve time and re-derivation is what publication, audit, and any third party perform; that makes
   the publisher's signature what resolution actually rests on, and it should be said outright rather
   than discovered.
5. **withdrawal authority.** a publisher withdrawing their own release is obvious. a registry withdrawing
   someone else's, for malware, is a different authority, and both need to be expressible without letting
   the registry silently redefine what a domain's keys mean.
6. **bootstrap.** the compiler's own first-party domains resolve before any registry exists. their trust
   root is the checkout, and saying so explicitly keeps bootstrap from quietly becoming a bypass.
