---
schema: design-doc/v1
id: decision-0042-binary-reuse-as-admitted-substitution
title: binary reuse is an admitted substitution over a lock binding, decided by policy over measured evidence
summary: a prebuilt binary is offered against a lock binding, never against a computation key; the admission test is four legs — the offer's binding matches the lock's, the offer's realization coordinates match this run's environment, the offered bytes measure to the identity the offer claimed, and a declared policy authorizes the substitution — checked clause by clause in a fixed order with the failing clause named, and with the policy leg shipping at M-4 as local admission of named origins and no keys; the lock records source only, a served substitution is a distinct provenance value, a rejected offer is an explained miss that builds from source and is not remembered, and resolution stays blind to offers
kind: decision
status: proposed
created: 2026-07-06
updated: 2026-07-27
tags:
  - packages
  - caching
  - artifacts
  - trust
  - provenance
relations:
  informed_by:
    - research-binary-reuse
    - research-artifacts-and-trust
    - research-reproducibility
  depends_on:
    - decision-0009-peer-first-party-domains
    - decision-0014-reproducibility-properties
    - decision-0023-rule-and-cache-identity
    - decision-0024-persistent-engine-state
    - decision-0031-action-cache-identity
    - decision-0033-consumer-of-action-reuse
    - decision-0038-represented-rule-bodies
    - decision-0039-package-identity
    - decision-0040-declared-constraints-and-resolution
    - decision-0041-the-written-lock
  supersedes: []
---

# binary reuse is an admitted substitution over a lock binding, decided by policy over measured evidence

> closes the binary-reuse line M-4 has carried since 0039 opened the milestone, in the shape 0039 fixed ("an admitted substitution over that binding rather than a 0031 cache hit") and with the witness 0041 predicted ("a subject digest against a builder's signed claim, which is the right shape for 0039's binary-reuse admission test"). also answers the reuse preference 0040 named and left undesigned, by refusing it.

## context

the milestone's first three records built the ladder this one stands on. 0039 fixed what a package, a package version, and a realization are, and drew the line this record crosses: binary reuse is an admitted substitution over a lock binding rather than a cache hit. 0040 fixed resolution as a host-rule computation whose answer is a pure function of four declared inputs, and named a reuse preference over prebuilt binaries as a policy question left to this record. 0041 fixed the lock as a text projection of a document that binds coordinates to source content, verified locally, with no witness. what remains of M-4's binary-reuse line is the question the research note frames: when a binary arrives from elsewhere, what may it stand in for, and who says so.

the precedents disagree about what a binary cache is, and the disagreement is not incidental. Nix frames substitution as an optimization with an explicit trust gate: every substituted path is re-hashed at import, but acceptance requires a signature by a key in `trusted-public-keys`, a substituter configured `trusted=true`, or self-authenticating content addressing — and content-addressed realisations still carry signatures because the derivation-to-output mapping is not client-derivable. Bazel's action cache is the opposite position: results are keyed by the digest of the serialized action, downloads are digest-verified for integrity, nothing authenticates an `ActionResult`, and the docs treat the cache as infrastructure to keep secure rather than a party to verify — the community analysis of a shared cache puts it as Bazel trusting the ActionResult blindly. ccache and sccache are the purest form of that position: correctness rests entirely on key completeness, with `sloppiness` as an explicit safety-for-hit-rate dial and `SCCACHE_RECACHE` as the knob that admits a shared cache can hold bad artifacts. Debian and Arch invert the perimeter: mirrors are untrusted because every artifact hash chains to a signed index (Debian's Release file) or travels with a per-package signature (Arch), so a compromised mirror fails verification. and the reproducible-builds line treats the binary itself as a claim: rebuilderd rebuilds packages and reports GOOD, BAD, or UNKNWN, signing an in-toto link for every GOOD rebuild, with quorum across independent rebuilders as the acknowledged-but-unfinished layer — no surveyed system verifies by rebuild at consumption time.

so the disagreement is about where the perimeter sits, not about mechanism. every system digests the bytes. a design that names its perimeter explicitly can express all three positions; one that inherits a cache silently has chosen the trusted-infrastructure position without saying so. pith's answer follows from the machinery it already has: the perimeter is named in one place, an admission policy, and the rest of the test is facts.

two internal facts bound the answer. M-4 ships no key infrastructure, so any authorization leg degrades to something local, and the record has to say what it degrades to and whether the remainder is worth having. and 0031's admission test — recorded executor identity, platform, access verification — asks whether *this* environment matches the one an attempt was recorded in, which a binary this graph never computed cannot answer, because there is no recorded attempt.

## decision

### what is admitted, and what the admission is about

a binary is offered against one lock binding, and the substitution it is admitted to replaces that binding's *realization* — the build computations that would turn the bound source into artifacts under this run's request inputs. it replaces neither the resolution nor the lock: the selection stands, the file stands, and what changes is how the selected source becomes artifacts. 0039's sentence is enforced by construction here.

the offer is a claim: this binary content is a realization of that source binding, built under these realization coordinates, served from this origin. the admission test never verifies the claim's truth. whether building the bound source actually produces these bytes is the bit-for-bit question, a property of the build instructions (0014), and only a rebuild measures it. what admission verifies is the claim's consistency with facts this run holds — the lock's binding, this run's environment, the measured digest of the offered bytes — and then authorizes the claim by policy. 0039's separation survives intact: the binary is a claim, the substitution is authorized, and the rebuild remains the way to turn the claim into a measurement.

what the distinction from a cache hit buys is where the trust lands. a cache hit answers a question about this graph's own past — did this very request already run in this environment — and its admission test checks recorded facts against present ones. a substitution answers a question about someone else's past, and its admission test is policy over evidence. collapsing them breaks three things. the reusable index's induction — an indexed attempt was reusable when it was published, because the engine published it (0024) — becomes ungrounded the moment the index holds an attempt nobody computed; provenance would report `Reused` over an execution that never happened, which is the Guix-graft failure 0039 warns about wearing a different spelling; and 0031's admission test, asked of a foreign binary, can never pass, because there is no recorded executor or platform to test — so the collapse would need either a forged execution record or a bypass around the test with no policy surface. a fourth break is quieter: an action key inherits invalidation semantics — a moved toolchain revision invalidates the key — and a binary indexed under a build's key would inherit semantics its own provenance cannot honor.

### the admission test: four legs

the test is a conjunction over four legs, checked clause by clause in a fixed order — coordinates, features, source, platform, toolchain, bytes, authorization — and the first failing clause is the one a refusal names.

the binding leg: the offer's binding — package identity, version, features, source content identity — equals the lock entry's binding, and a refusal on this leg names which of the three moved. the lock entry is the record of what this run would resolve (0041 checks it against read bytes per entry), so equality against the entry is the test. an offer for the same coordinates over different source is not a substitution candidate; it is drift-shaped, the failure a lock binding exists to catch.

the environment leg: the realization coordinates the offer claims — platform and toolchain, the request inputs a realization's identity already covers (0039) — equal this run's. a binary for another platform or another toolchain is a different realization and fails before authorization is even asked. this leg is what makes the substitution a realization-level fact rather than a package-level one, and it is where a cross-compiling environment says which binaries are its own.

the content leg: the digest of the bytes actually offered, measured locally from what was read, equals the content identity the offer claims. this is the one measurement, and every surveyed system performs its twin: Nix re-hashes at import, Bazel digest-verifies downloads, apt checks package checksums against the signed index. it is also the leg that stays a fact no matter what the authorization leg becomes.

the authorization leg: a declared policy authorizes the substitution. this is the perimeter, in one place.

the three fact legs are checked from values a real run holds — the lock entry, the caller's environment declaration, the measured digest — so no leg can pass vacuously: each one is a comparison whose two sides are separately observable in the outcome, and each failure names both sides.

### the authorization leg at M-4: local policy, no keys

M-4 ships no key infrastructure, and the record refuses to pretend otherwise. the policy is a declared set of origins this environment admits, in the typed evidence shape the lock entry already carries — where a binary is served from, stated as a decision rather than left implicit in a URL — on the shape of Nix's `trusted=true` substituter with the signature gate's absence named rather than hidden, and of ccache's trusted cache with the trust made a value instead of an ambient assumption. an offer from an origin the policy does not name is refused on the policy leg and the build runs.

this is still worth having, for two reasons. the three fact legs do not degrade — binding, environment, and digest are as strong with no keys as with them, and they are what a tampered or mistaken offer trips over first, which is exactly Debian's and Arch's arrangement where the supplier is never trusted and integrity is anchored elsewhere. and naming the perimeter in one leg means the upgrade replaces one leg and touches none of the others: when key infrastructure arrives, the policy leg becomes attestation verification — check the subject digest against the artifact in hand and the builder identity against a configured root of trust, the SLSA verifying workflow, which is 0041's "subject digest against a builder's signed claim" — and the offer grows the attestation beside its origin without the test changing shape.

the honesty statement is 0041's, repeated for this boundary: at M-4 the authorization is a person's configuration, and no evidence vouches for an origin beyond the configuration naming it. what settles the leg is the first remote source adapter and the threat model it forces, the same event 0041 named for its witness.

### not the reusable index

the substitution path shares nothing with 0031 and 0033 but the word admission. it reads no attempt, no computation key, no executor identity; it produces no attempt. the engine learns nothing about offers or substitutions, on the terms 0024 keeps it from learning about packages, and the whole test runs in the package library over values. this is deliberate rather than convenient: the reusable index answers questions about this graph's recorded past, and a foreign binary has no recorded past. a separate path with a separate outcome type is what makes the two answers distinguishable in provenance — `Reused` names an attempt this engine published, a substitution names an origin and an authorized claim — and the distinguishability is what keeps the Guix-graft failure out: a build from source and a substituted binary are different provenance claims about the same package version, recorded as different values.

### the lock records source only

a binary is a different content identity over the same coordinates, and the lock does not record it. 0039 already argued this: one lock answers several platforms by binding source only, because a realization is derived per environment and locking realizations would duplicate that derivation as data with nothing to check it against. a second entry kind for binaries would reopen the question 0039 closed, and it would reopen 0041's merge story: entry lines are a set over package identities precisely so a union merge is safe, and binary records would either multiply entries per package or live in a second section with its own conflict shape.

the cost is named rather than hidden: substitutions have no cross-process witness in the lock. each environment admits its own binaries under its own policy, no union merge ever composes two environments' substitutions, and a machine that wants to know which binaries another machine admitted cannot learn it from the lock. the witness of a served substitution is its provenance record, below.

### rejection and fallback

a binary that fails any clause is a miss, and the build runs from source. none of the failures is a fault — the shape is 0031's, where a failed admission is a cache miss and not an error — and the rejection is explained: it names the clause that failed and both sides of the comparison, so a person can tell a wrong binding from a wrong platform or toolchain from a tampered artifact from an unauthorized origin.

a rejected binary is not remembered. the next resolution re-attempts it, because the admission test is deterministic, cheap beside the build it guards, and a negative entry keyed by nothing — the offer is not a request input and belongs to no revision — would be the third class of state 0038 named, state neither the request nor the revision covers. if a rejected offer keeps arriving, the explanation arrives with it every time, which is the honest behavior for a fact that keeps being false. remembering rejections is a real optimization once offers are fetched at cost, and it is refused until the first remote fetching exists to make that cost measurable.

### resolution stays blind to offers, and the reuse preference is refused

0040 named a reuse preference over prebuilt binaries and left its composition with the newest-version preference to this record. it is refused, on 0041's ground. a selection is reproducible under the same candidate universe because it is a pure function of the four protocol inputs; offers are environment facts, and a resolution ordered by which candidates have admitted substitutes would move the lock when a mirror gained a binary, with no input diff naming the cause — the failure mode 0040's own refusal of solver policy exists to prevent. substitution composes after resolution, over whatever was selected, which also keeps 0039's substitution-over-a-binding shape: there is no binding to substitute over until the answer exists.

if a domain ever wants the preference, the offers must become request inputs of the resolution — a fifth input the lock would then have to record beside the universe digest, on the same terms the scheme is recorded — and that is a protocol change to argue in a record of its own, measured against a real registry.

### provenance records a substitution as a substitution

a served substitution is a value: the binding, the realization coordinates, the measured content identity of the binary, and the origin whose claim the policy admitted — every input the test consulted, carried out of it. it is a record over existing constructors, carried in the realization's outcome, and it is the witness a caller keeps — the piece the lock's refusal of binaries leaves unwitnessed across processes. it is deliberately not an engine attempt: nothing was computed, and the record says whose claim was trusted rather than implying the graph produced the bytes.

## alternatives considered

### point 0033's machinery at binaries

the strongest collapse: publish the binary as an attempt under a computation key derived from the binding, let the reusable index serve it, inherit revalidation and invalidation for free.

rejected on the four breaks named in the first section, any one of which is disqualifying. the index's induction becomes ungrounded, provenance reports an execution that never happened, 0031's admission test has nothing to test, and the inherited invalidation semantics describe a build nobody ran. the free machinery is the point: it answers questions about the wrong past.

### binaries in the lock, as a second entry kind

record beside each binding the binary identity this environment admitted, so the file witnesses substitutions too.

rejected on 0039's argument and 0041's shape, with the cost named in the decision section. the one thing a lock-recorded binary would buy — a cross-process, mergeable record of what was admitted — is bought at the price of making a realization-level, environment-specific fact masquerade as a selection, and the merge story for two environments admitting different binaries over one binding has no union answer.

### attestation verification now

ship the SLSA-shaped leg immediately: offers carry attestations, the test verifies subject digest and builder identity.

rejected because there is nothing to verify against. no root of trust exists at M-4, so verification would either trust every key it sees — which is the trusted-infrastructure position adopted silently, the one thing this record exists not to do — or fail every offer and make the machinery dead code. the offer's shape leaves room for the attestation beside the origin, and the upgrade touches one leg.

### rebuild as the admission

rebuilderd's position as a gate: admit a binary only once a rebuild has confirmed it reproduces.

rejected as the default because it costs what it saves — a rebuild per binary is a build per binary, and 0014 already keeps verification available without making it the local default. kept as the backstop, exactly as 0039 said: the rebuild is how a claim becomes a measurement, and a policy of the future can demand the measurement, at which point rebuilderd's quorum of independent rebuilders is the strongest witness the research surveyed.

### trust the configured source, digest-only

the Bazel and ccache arrangement, stated as policy: a configured offer source is inside the perimeter, digests guard integrity, nothing else is asked.

absorbed, not rejected: this is precisely what the M-4 policy leg *is* when it names an origin — the difference is that pith records the admission as a provenance value naming the origin and the policy, rather than leaving the trust in the fact that someone configured a cache. the position is expressible; it is just not silent.

## consequences

phloem gains the substitution slice: the offer, the admission the run brings to the test — the entry, the platform, the toolchain, the admitted origins — the clause-ordered test, the refusal with its named clause and both sides of the disagreement, the substitution record as a value, the realize function answering with the substituted record or the build, and the build's requests derived from the description against xylem's existing compile interface. no kernel constructor lands, which makes this the third M-4 record to need none.

the engine is unchanged, and xylem is unchanged. the substitution belongs in phloem because every input to it is package-domain data — the binding is the lock's value, the offer is a claim about package coordinates — and the build it replaces is reached through the same peer boundary 0039 fixed, the request constructors phloem already holds. a substitution in xylem would make the build library learn packages, which 0009 forbids in this direction.

### measured

the prototype checks the record's claims directly. a binary offered for a binding the lock produced through a real resolution is admitted when every clause holds, and the outcome substitutes the binary for the build, which issues no request at all. the admission test cannot pass vacuously: a perturbation fixture moves one input at a time — the version, the feature set, the source, the platform, the toolchain, the bytes, the origin — and each move is refused by the clause that reads that input, with both sides of the disagreement carried in the refusal; the admitted record in turn carries every input the test consulted, its measured digest computed from the bytes rather than copied from the claim. a refused offer leaves the build running in place of the binary, one request per prescribed input. the substitution record round-trips through its value. the rendered lock is byte-identical before and after a served substitution. and the two reuse paths are distinguishable in one test: the engine's second resolution reports `Reused` under its computation key while the substitution reports its record naming the origin and the measured content — different answers to different questions, visible as different values.

## unresolved

the attestation shape is open in its details: which predicate a verified offer carries, whether pith's own builds emit attestations so a substitution can one day be authorized by a first-party builder, and whether a quorum of rebuilders is ever demanded as the strongest leg. rebuilderd's open issues — attestations fetched by artifact digest, transparency logs, a standard multi-rebuilder predicate — mark how unfinished the ecosystem's answer is, and the first remote adapter decides how much of it pith needs.

where substitution records persist is open. M-4 returns the record to the caller and keeps no cross-process trace, which matches the lock's refusal to witness binaries but leaves a long build's substitutions unrecorded once the process ends. whether they enter an engine store, a caller-side file, or the development environment's own artifact is the neighboring record's question.

remote fetching and materialization are out of scope with the first source adapter: an admitted binary's bytes are measured where they were read, and nothing here says how they travel or where they land in a content store.

an offer names one content identity, and a package whose realization is many artifacts — a library plus its headers, a runtime plus its tools — needs either a tree identity or a list of content identities. the first real binary offer will measure which, and the record does not guess.
