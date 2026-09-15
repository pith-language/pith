---
schema: design-doc/v1
id: research-registry-supply-chain-security
title: registry supply-chain security
summary: recent package compromise evidence and the distinct roles of staged releases, authenticated metadata, provenance, transparency, and execution confinement
kind: research
status: researching
evidence: preliminary
created: 2026-09-06
updated: 2026-09-06
tags:
  - security
  - registries
  - modules
  - supply-chain
relations:
  informed_by:
    - research-module-distribution
    - research-artifacts-and-trust
    - research-binary-reuse
    - research-reproducibility
  depends_on:
    - research-method
  supersedes: []
---

# registry supply-chain security

this follow-up answers the request to design supply-chain mitigation into M-14 from the beginning.
sources were checked on 2026-09-06; the dates below distinguish incidents, announcements, and
specifications. the result is a proposed [registry design](../planning/modules/registry.md), not an
assertion that pith implements these controls. the existing M-4 witnesses remain evidence at their
original scopes.

## the pressure moved beyond stolen passwords

recent incidents do not support treating a familiar package name or a successful signature check as
sufficient admission. the attacker often controls an existing release path.

| event or publication | primary evidence | implication for pith |
| --- | --- | --- |
| Trivy attack, March 19, 2026; vendor update April 1 | Aqua describes compromised automation credentials, moved action tags, and incomplete initial credential rotation | pin immutable source revisions; recover every authority path, not only the first stolen token |
| npm staged publishing, May 22, 2026 | npm supports uploads to a staging queue, with separate human approval; an OIDC workflow can be limited to staging | an upload credential should not authorize release |
| npm install-default announcement, June 9, 2026 | npm announced disabling dependency scripts and restricting Git/remote sources by default in v12 | acquisition should execute no module code, including indirect setup hooks |
| npm/GitHub incidents, May–July 2026; Unit 42 report updated July 15 | researchers describe cache poisoning, theft of live OIDC credentials, and malicious packages with valid provenance | short-lived credentials and provenance need workflow isolation and independent approval |
| Dependabot cooldown, July 14, 2026 | version-update PRs gain a default three-day delay; security-update PRs are exempt | give newly released malware time to be detected, but state the emergency-update policy |
| npm publication scanning, July 28, 2026 | packages can be admitted, held for review, or blocked before becoming installable | staging is also where detection can act before distribution |

sources: [Aqua's incident report](https://www.aquasec.com/blog/trivy-supply-chain-attack-what-you-need-to-know/),
[npm staging](https://github.blog/changelog/2026-05-22-staged-publishing-and-new-install-time-controls-for-npm/),
[npm's announced defaults](https://github.blog/changelog/2026-06-09-upcoming-breaking-changes-for-npm-v12/),
[Unit 42's investigation](https://unit42.paloaltonetworks.com/monitoring-npm-supply-chain-attacks/),
[Dependabot cooldown](https://github.blog/changelog/2026-07-14-dependabot-version-updates-introduce-default-package-cooldown/),
and [npm publication scanning](https://github.blog/changelog/2026-07-28-npm-publish-time-malware-scanning-and-dual-use-metadata/).
the June announcement alone is not evidence that every npm v12 change is deployed everywhere.

Unit 42's most relevant observation is that a poisoned build cache let attacker code obtain OIDC
credentials from a legitimate release runner. a skipped publish step did not prevent unauthorized
publishes using those credentials. its June investigation also describes malicious output carrying
accurate provenance from the compromised pipeline. these are the researcher's findings, not independent
measurements by pith. the conclusion is narrower than abandoning OIDC: isolate build execution from
publication capability, and give the latter less authority.

[GitHub's July 28 summary](https://github.blog/security/supply-chain-security/disrupting-supply-chain-attacks-on-npm-and-github-actions/)
describes its responses across account recovery, untrusted workflow triggers, cache writes, credential
revocation, and publication. some controls are deployed while network restrictions were still future
work behind a monitoring preview. this is evidence for breaking several links in the attack chain,
not for treating one registry feature as a complete defense.

## trusted publishing authenticates a job, not its output's intent

the invariant worth adopting is removal of reusable publishing secrets from build environments.
[OpenSSF's implementation guidance](https://repos.openssf.org/trusted-publishers-for-all-package-repositories)
describes issuer selection from a supported set, signature and claim verification, registry-specific
audience checks, and comparison to a configured publisher policy. the token cannot choose its own
trusted issuer or authorization policy.

[PyPI's security model](https://docs.pypi.org/trusted-publishers/security-model/) explains why the workflow
boundary still matters: repository writers may change the authorized workflow; dedicated environments
and approvers can narrow that authority. removing a maintainer does not automatically remove trusted
publishers they configured. recovery and offboarding must inspect both relationships.

long-lived bearer tokens are cheap to integrate but give stolen credentials a reusable path.
maintainer-held signing keys outside CI remain useful for release approval even when OIDC authenticates
staging. short lifetime limits exposure; a live bearer credential can still be stolen and used before
expiry. result for pith: subject-scoped stage-only capabilities plus separate approval over the exact
release digest. protect authority changes more strongly than uploads. support a signing adapter for
non-OIDC and self-hosted workflows without assigning compiler privilege to any forge.

## authenticated metadata needs compromise recovery

[TUF 1.0.36](https://theupdateframework.github.io/specification/latest/), updated August 5, 2026, separates
root, targets, snapshot, and timestamp responsibilities. its client workflow checks signatures,
delegations, versions, expiry, lengths, and hashes. thresholds and rotation limit particular key
compromises; consistent snapshots address metadata combinations that never existed together. the trust
root and a sufficiently trustworthy clock remain assumptions. TUF cannot make arbitrary first-time
software selection safe or force an attacker to provide availability.

the lineage predates the recent campaigns: [PEP 458](https://peps.python.org/pep-0458/) distinguishes
the practical minimum, where registry online keys sign uploaded packages, from the stronger model in
[PEP 480](https://peps.python.org/pep-0480/). the latter adds developer-controlled signing so compromise
of registry online keys alone does not authorize substituted releases. PEP 480 is a draft proposal,
not a claim about deployed pip behavior. keys stored together share a compromise boundary even if
their role names differ.

result for pith: use a standard TUF client and metadata layout, with bootstrap and recovery fixtures
from the first registry slice. require a separate maintainer release endorsement checked by the client.
a registry database flag saying approval passed is insufficient against compromise of that registry.
root and domain-policy replacement need protected authorization; otherwise an attacker can replace the
keys that authorize endorsements.

this supplements the transparency choice in 0044. metadata authorization/freshness and comparable
release history are distinct questions. choosing a log did not discharge key recovery or current
metadata admission.

## provenance and source review are separate evidence

[PyPI's attestation model](https://docs.pypi.org/attestations/security-model/) explicitly separates origin
evidence from whether a package deserves trust. a signature relates content to an identity; it does
not establish that malicious code was absent before or during the build.

[SLSA 1.2's build requirements](https://slsa.dev/spec/v1.2/build-requirements) distinguish existing
provenance from authentic provenance and resistance to forgery by build tenants, alongside isolation.
its [source requirements](https://slsa.dev/spec/v1.2/source-requirements) address source changes, with
two-party review at the strongest source level. build provenance cannot substitute for source controls.

result for pith: distribute source modules and reconstruct the normalized tree from the approved
commit/subpath without running project commands. this is a pith-specific simplification enabled by
M-14's source format. generated publication content needs an attested transformation or must be
committed first. when M-15 produces binaries, builder identity, inputs, transformation, and isolation
become additional admission facts. a JSON statement using a SLSA predicate does not establish a level.

independent rebuilding detects disagreement between source and an offered artifact. it cannot prove
deliberately malicious source benign. M-14 source reconstruction buys the relevant comparison without
operating a general-purpose build farm.

## transparency needs consistency, witnesses, and observers

an inclusion proof answers whether a leaf appears under one checkpoint. it does not alone establish
that two consumers saw the same history. the [C2SP witness protocol](https://c2sp.org/tlog-witness@v1.0.0)
submits a checkpoint and consistency proof for cosigning. a deployment still needs a policy specifying
which independent witnesses count. quorums must intersect in an honest, stateful witness under the
claimed compromise bound; requiring any one of several witnesses does not establish that property.

[Rekor v2's client notes](https://github.com/sigstore/rekor-tiles/blob/main/CLIENTS.md) describe witnessed
checkpoints but also say its initial launch used unwitnessed checkpoints. protocol support is not
evidence of a public service's operating guarantee. pin and test actual client/service versions.
[Sigstore's timestamp documentation](https://docs.sigstore.dev/cosign/verifying/timestamps/) distinguishes
signature-time evidence from append-only history; v2 clients use a separate timestamp authority.
a signing time does not establish when a release first became publicly available.

result for pith: implement signed checkpoints, inclusion, consistency, and configured witness verification
with persistent last-seen state. log release and authority-change events. a monitor checks duplicate
subject/version bindings and ownership changes. witnesses check consistency, not package safety or
the meaning of every leaf. private registries need private logs/witnesses to avoid publishing names
and identities to an unrelated public service.

## delays and detection should have explicit inputs

[uv's resolution documentation](https://docs.astral.sh/uv/concepts/resolution/) supports fixed upload-time
cutoffs and duration-based cooldowns. it records the resolved timestamp in the lock and does not
continuously update it as wall time advances. pith can similarly resolve time into a declared input
before solving.

recommend an initial 72-hour public-release cooldown, measured from admitted availability evidence,
and record its fixed cutoff and policy. this duration is a starting policy, not an experimentally
optimal value. malware may wait out the delay; a dishonest time authority may backdate metadata.
security fixes need a narrow authorized exception, not a package field claiming urgency.

scanning, anomaly detection, and vulnerability feeds fit beside the delay. record scanner version,
evidence snapshot, and findings. no findings is not proof of safety and cannot bypass failed
authorization. the stage/hold protocol should precede choosing a scanner.

## consequences for the existing Pith boundaries

every source, lock, and reusable artifact needs an admission result naming content, evidence, policy,
time context, and reason. it cannot be an everlasting trusted bit on a content-store object: revocation
or security-snapshot expiry changes whether execution is permitted without changing bytes.

0048's version pinning concerns pith format compatibility. security metadata versions, root generations,
revocation sequences, and log sizes are rollback counters; pinning them at 1 or clearing them during
engine-state rebuilding destroys protection. clarify the distinction in the implementation record.

acquisition must not execute install scripts, load native host code, honor executable Git filters, or
activate dependency editor/agent configuration. later build confinement remains essential: a dependency
cannot grant itself network, secrets, filesystem authority, or a weaker admission policy.

independent approval adds friction; expiry and witness failure can block availability; domain enrollment
and recovery need administration. a small curated registry is a credible first operating model.
checksum-only publication with security fields reserved for later leaves the bootstrap,
recovery, and bypass questions that the recent incidents make urgent.
