---
schema: design-doc/v1
id: research-reproducibility
title: the reproducibility lineage
summary: how the Reproducible Builds community operationalized bit-for-bit identity, and why the build system is the verifier not the producer
kind: research
status: researching
evidence: reviewed
created: 2026-03-05
updated: 2026-03-05
tags:
  - research
  - reproducibility
  - trust
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# the reproducibility lineage

reproducibility is older than any of the build systems this project studies, and it has been operationalized most rigorously by a community that does not build build systems.

## the pressure

the original pressure was trust. binary distributions ask users to run code they did not compile. without a way to check that a binary corresponds to its published source, the source is documentation and the binary is what runs.

signing the binary does not solve this. a signature establishes who produced a binary, not that the binary corresponds to any particular source. the signer could have put anything in it.

the Debian reproducibility effort, which became the Reproducible Builds project, started here and arrived at a concrete operational definition:

> a build is reproducible if, given the same source code, build environment, and build instructions, any party can recreate bit-by-bit identical copies of all specified artifacts.

verification is by cryptographic hash comparison of independently-built outputs.

## the mechanism

the community's contribution was less the definition than the enumeration, over roughly a decade, of every way a build can fail to meet it.

builds embed nondeterminism through:

- timestamps
- filesystem readdir order
- locale, timezone, umask
- usernames and hostnames
- uninitialized memory
- address-space layout, including ASLR
- embedded random values, including UUIDs and cryptographic keys
- parallelism races
- CPU-feature detection
- profile-guided optimization

each is a way for two builds under "the same inputs" to produce different bytes. most look innocent in isolation. together they are the reason "two builds produced two different binaries" is the default state of software rather than an anomaly.

the fixes are mechanical where the nondeterminism is removable and conventional where it is not. SOURCE_DATE_EPOCH is the convention for timestamps: one environment variable carrying a unix timestamp, which build tools clamp to instead of reading the wall clock.

```
SOURCE_DATE_EPOCH=1722739200
```

the value is the source's last-modification time. it is more informative than build time, since it reflects how old the software actually is, and it is stable across builds. the specification requires that tools embedding timestamps clamp them to a value no later than SOURCE_DATE_EPOCH, and that human-readable formatting be deferred to runtime.

the deeper fixes are the community's stated principles: do not record the maker or the place of making, do work in a determined order rather than readdir order, keep the workspace clean of locale and timezone, do not embed randomness, treat parallelism and profile-guided optimization as amplifiers of any remaining nondeterminism. the list is the artifact.

## where the invariant lives

reproducibility is a property of the build instructions and the build environment. the build system can verify it and can refuse to assert it when unverified. it cannot produce it.

this is why Debian measures reproducibility as a percentage of packages rather than as a property of apt. the engine is the same across all packages. the variation is in the build scripts. Nix is in the same position. Nix's content-addressed derivations let it detect and substitute verified-reproducible outputs. the nixpkgs reproducibility measurements are a property of the package set, not of the Nix daemon.

the engine is the verifier. the build instructions are the producer. confusing the two is the mistake this design has to avoid.

## what this project inherits

the kernel's identity and storage model provides content-addressed identity by construction, which makes bit-for-bit comparison trivial once two builds exist. the kernel's clean-build-equivalence invariant, from the build-system lineage, ensures the same declared inputs produce the same computation. neither is reproducibility. both are prerequisites for verifying it cheaply.

what the kernel should not do is claim reproducibility as an engine property. the honest framing is that the engine verifies reproducibility by building twice and comparing content identities, records the result in provenance, and surfaces the absence of verification as a weaker guarantee. decision 0014 is about this.

the build library, separately, should adopt the Reproducible Builds determinism rules and SOURCE_DATE_EPOCH as defaults for the actions it defines. this is library policy, not kernel semantics. if reproducibility is a property of the build instructions, the first-party build library should ship instructions that have it.

## questions for the historical pass

- which of the Reproducible Builds determinism rules have proven hardest for real build systems to adopt, and what does that say about defaults?
- how do reproducibility verifications interact with cross-compilation and multi-platform builds, where "the same output" is defined per target?
- what is the trust model when one build is local and one is remote? how does the remote executor's identity affect whether the comparison is evidence?
- how do existing supply-chain frameworks (in-toto, SLSA) represent verified-reproducible status, and is there an existing attestation shape to align with?

## sources

- [Reproducible Builds: definition](https://reproducible-builds.org/docs/definition/)
- [Reproducible Builds: commandments](https://reproducible-builds.org/docs/commandments/)
- [SOURCE_DATE_EPOCH specification](https://reproducible-builds.org/specs/source-date-epoch/)
- [Reproducible Builds: tools](https://reproducible-builds.org/tools/)
- [Reproducible Builds: deterministic build systems](https://reproducible-builds.org/docs/deterministic-build-systems/)
