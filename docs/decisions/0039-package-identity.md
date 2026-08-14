---
schema: design-doc/v1
id: decision-0039-package-identity
title: separate package identity from version and realization identity
summary: a package is named by declaration, a name in a domain, which fills 0005's semantic-identity slot; a package version adds the coordinates constraints and locks range over; a realization is identified by the computation and content identity the kernel already has; a lock entry binds coordinates to source content identity, and binary reuse is an admitted substitution rather than a cache hit
kind: decision
status: proposed
created: 2026-06-19
updated: 2026-06-19
tags:
  - identity
  - packages
  - caching
relations:
  informed_by:
    - research-dependency-resolution
    - research-artifacts-and-trust
    - research-nix
    - research-sources
  depends_on:
    - decision-0005-separate-identities
    - decision-0009-peer-first-party-domains
    - decision-0013-managed-object-identity
    - decision-0015-interface-rule-selection
    - decision-0023-rule-and-cache-identity
    - decision-0026-generic-typed-calculus
    - decision-0031-action-cache-identity
    - foundation-scope
  supersedes: []
---

# separate package identity from version and realization identity

> opens milestone M-4. the constraints, lock format, and environment records that follow it all presuppose an answer to the one question here: what gives a package its identity, and how does that identity relate to the content identity the kernel already has.

## context

M-4 asks for package identity, basic constraints, lock data, binary reuse, and a reproducible development environment. every one of those names packages: constraints range over them, a lock pins them, binary reuse substitutes for building them, an environment installs them. none can be designed until the record says what a package is.

the kernel has three identity answers already, and none of them is a package. content identity says whether two immutable byte sets are equal. rule identity and revision (0023) name a semantic declaration and one revision of its executable semantics. 0005 separates semantic, computation, content, and external identity, five with 0013's managed-object, and its own context already says "a package has semantic identity" without saying what constructs it. that construction is this record's work.

a package is not content, in both directions. the same source content builds different packages: Debian's `gcc-defaults` source package builds the unversioned `gcc`, `g++`, and `cpp` wrappers, whose versions track the defaults rather than the compiler, while the compiler itself comes from a different source package entirely; one source name maps to many binary names and a binary's `Source` field is provenance rather than identity. and the same package resolves to different content per platform and per toolchain: Spack's dag hash covers name, version, variants, compiler, architecture, and dependency hashes precisely because a concretized spec is not a package; conda hashes the "used" variant variables into the build string for the same reason; Debian's `Multi-Arch: same` lets `libc6:amd64` and `libc6:i386` share one name and version as distinct instances distinguished by an architecture qualifier. a version can even name more than one build of itself: a Debian binNMU rebuilds a binary from an unchanged source and publishes it under a `+bN` suffix, so even (name, version, architecture) does not pin the bytes.

the precedents disagree about where identity lives, and the disagreement is structural rather than incidental. Nix has no identity above the store path: an input-addressed path is derived from the derivation, a content-addressed path from the output, and package names live in the expression layer (nixpkgs attribute paths) that the store model never sees. Cargo, npm, and Debian name packages by declared coordinates, name plus version plus a source for Cargo, with bytes pinned separately by a checksum of the `.crate` tarball, an SRI integrity value, or archive signatures; npm's registry made name-and-version immutable by policy after the 2016 incidents, which is a promise about behavior rather than a property of the identifier. Go names a module by path and version, where a pseudo-version embeds a commit hash and timestamp, and go.sum binds those coordinates to an `h1:` hash of the module's canonical file tree, witnessed by sum.golang.org, a signed transparency log. purl is pure coordinates and carries no digest at all; Software Heritage is the opposite position, identifiers computed intrinsically from content, with origin and path as qualifiers that are explicitly not part of identity. TUF and in-toto split the question three ways: names locate, hashes authenticate, keys authorize.

the two families answer questions that arise at different times, which is why neither wins. declared coordinates are speakable before anything exists: a constraint `openssl >= 1.1.1` names a package no digest could name yet, and a lock can be compared, diffed, and reasoned about as data. computed identity is verifiable after content exists: a hash is checked against bytes, while a name is trusted against a registry's discipline. the SBOM standards carry the scar of conflating the two: SPDX's package verification code and CycloneDX's `evidence.identity` with confidence scores both exist because name-and-version alone does not determine bytes, and the systems that work all keep coordinates and digests as separate fields bound by a record rather than folding one into the other.

## proposed decision

identity is drawn at three levels. a package is named by declaration; a package version adds coordinates; a realization is identified by the machinery the kernel already has.

### a package is a declaration named within a domain

a package's identity is an author-declared name inside a declared domain: the pair (domain identity, package name). the domain is a namespace authority, either a first-party library's namespace or a remote source identity such as a registry or a forge. the construction parallels 0023's `RuleIdentity`, a module identity plus a declaration name, because the two questions are the same shape: a stable coordinate of a declaration site that survives changes to what is declared there.

the identity is declared, not computed, and it fills 0005's semantic-identity slot for this domain. what it is stable across: version bumps, constraint and metadata changes, platform and toolchain changes (which change realizations, not the package), maintainer changes, and source moves that the domain's resolution survives. Go is the precedent for the last of these: a module's identity is its path, and a moved repository keeps the old path resolving through a `go-import` redirect while go.sum keeps pinning the bytes behind it, so the name is stable and the resolution is a lookup that may move. what breaks the identity: a rename. a renamed package is a new identity, and continuity across a rename is an explicit aliasing operation recorded in provenance, never something the system infers; Cargo's dependency rename is instructive as the opposite, a purely local mapping that never enters the lockfile, which is exactly why it cannot carry continuity. and what is forbidden: a name changing its meaning inside a domain, which is domain policy to enforce (npm's immutability rule, a registry's publish discipline) and pith's policy to check when it adopts one.

two identity types are nearby and are not this one. the domain's own identifier for a package, a registry entry or a forge path, is 0005's external identity: recorded in provenance as where the package is known, never the package's identity, for the reason 0005 gives, that semantic identity must not depend on a selected realization. and 0013's managed-object identity does not apply, because a package version is immutable and unowned in 0013's sense; there is no continuity-of-ownership question to answer, and a registry entry that changes underneath a package name is drift for policy to detect, not an identity transition.

### a package version is coordinates, not content

a package version is the package identity plus a version, in the format and comparison the domain declares (semver for most domains, Debian's epoch-and-tilde ordering for a deb domain). this is the thing constraints range over and the thing a lock names, and it is deliberately a level below the package rather than the identity itself. Cargo's `PackageId` and purl both fold version into the identifier, and both ecosystems still need the lineage above it: an upgrade path is a relation between two versions of one package, a lock diff reads as "the same package moved," and a vulnerability statement `openssl < 1.1.1k` ranges over versions of the package, not over a list of unrelated identifiers. two levels rather than one is the cheaper model of those sentences.

a package version's description is a value: a record carrying the source binding, the build inputs it prescribes, and the options it declares. the description's own content identity is a digest of that value, and it is not the package's identity either; it is one revision of the description, on the same terms a rule revision is one revision of a rule. the record shape is what this record needs from the 0026 calculus, and it has its own section below.

### a realization is identified by what the kernel already has

building a package version is a request against the interfaces the build library already declares, selected by 0015 on the same terms as any other request. the realization's identity is the existing computation machinery: the selected rule's identity and revision, the request inputs, the planned contract digest (0031), and the content identity of the outputs. platform and toolchain enter as request inputs, which M-3 already measured rather than assumed: a toolchain is a request input, and the same source under gcc and clang plans different actions and produces different objects. variants, when they arrive, enter the same way.

this is where Spack's dag hash and conda's build hash land in pith: not as a new package-side identifier but as the observation that a realization's identity must cover everything that influenced it, which is what a computation key already is. no kernel machinery is added for realization identity, and the package library computes none of it; it asks the engine.

the package library stays a peer of the build library, on the terms 0009 and the scope statement set. a package version's description names build interfaces and carries inputs; it never wraps "build" as a sub-concept of "package." scope's base case holds unchanged: someone builds one executable without defining a package, and the same build output later becomes package content through an explicit conversion that provenance records, which is the cross-domain conversion 0009 says cross-domain operations need. the dependency runs package-to-build, never the reverse.

### a lock entry binds coordinates to content

a lock entry is a pair plus evidence: the package version (what was chosen), the content identity of the source it resolved to (what the choice means), and the origin it was resolved from (where that happened). the binding is the entry; the origin is not part of either identity.

the precedents all converge on this shape despite disagreeing about everything above it. flake.lock records the unlocked reference, the locked reference, the `narHash` of the content, and the `rev` it came from. go.sum records module, version, and the `h1:` tree hash, with the origin not even present because the sum database makes it irrelevant to verification. Cargo.lock records name, version, source, and the checksum of the `.crate`. TUF's targets metadata is the same entry wearing signatures: a path, hashes, and the keys that authorized them. in every case the coordinates name and the digest authenticates, and the entry is the record that binds them.

the content side of the binding is intrinsic, in Software Heritage's sense: computed from the bytes pith reads, valid wherever those bytes are found again, independent of which origin served them. the 0020 discipline applies unchanged, since reading a registry's tarball is reading bytes from a declared source and assigning identity from what was read.

what makes the binding trustworthy is not settled here. a lock is data, not authority; go.sum is trustworthy because a transparency log witnessed the hash line and a client checks the inclusion proof, TUF metadata because a key threshold signed it, Cargo.lock because the registry index vouches for the checksum. which witness M-4 ships with, or whether the first cut is local-verification-only, is the artifacts-and-trust question, named in unresolved below.

one lock answers several platforms by binding source only. a realization is derived per environment, and its identity already encodes the platform through the request and the contract; locking realizations would duplicate that derivation as data and give it nothing to check against. the open question about multi-platform locks in the planning record is answered to exactly this extent: the lock pins what was chosen and what it resolves to, and the per-platform facts recompute.

### binary reuse is an admitted substitution, not a cache hit

0031's admission test answers whether a recorded attempt of this very request may be served: the key derives from the request, and the environment an attempt was recorded in is tested when reuse is considered. a prebuilt binary is not that. it is a realization this graph did not compute, produced by a different build graph under different rules; serving it answers a different question, whether a realization someone else computed may stand in for one pith would compute.

so package-level binary reuse is 0031's shape over different material, and the difference is the trust. the request side is the lock binding rather than a computation key: this package version, this source content. the admission test is policy over evidence: the source binding matches what this run would resolve, the content offered matches what the publisher bound it to, and some authorization covers the substitution, whether an attestation, a signature, or an explicit local policy decision. the strongest alternative witness is recomputation, Nix's floating content-addressed derivations rebuilding to verify determinism, which is 0014's rebuild check and costs what it always costs. attestation-first with recompute as the backstop keeps 0014's separation between a claim and a measured fact: the binary is a claim, the substitution is authorized, and the rebuild remains the way to turn the claim into a measurement.

a served substitution is recorded in provenance as a substitution. Guix's grafts are the caution here: a grafted `bash` has a different store path than an ungrafted one, so "the package that was installed" depends on how resolution ran, and the effective identity is context-dependent in a way the model has to work to explain. a build from source and a substituted binary are different provenance claims about the same package version, and the record keeps them distinct rather than making the package's identity absorb the difference.

### what this needs from the 0026 calculus

a package is the first record-shaped thing in the system, and the package description measures the need the way 0015 measured `Nominal` and 0034 measured `List`.

records are required, with high confidence: a description is nothing but named fields (name, version, source binding, options), a lock entry is named fields, and there is no honest way to spell either in the six scalars plus `Nominal` plus `List` that `pith-core` carries today. the shape is what 0026 already specifies: closed records with fixed named field sets, canonical encoding with fields sorted by name, no row variables. landing it means `Type::Record` and `Value::Record` and the next `RECORD_ENCODING_VERSION` bump on the moved-aside-and-rebuilt terms of the last two.

declared sums are required by the source binding, with slightly less confidence. a source is a fixed set of constructors carrying different payloads (a registry archive with its digest, a git revision with its tree hash, a local path with its content identity), which is a declared sum with typed payloads, not a record with a tag field; the tag spelling re-creates the flat-namespace ambiguity 0026 rejected polymorphic variants for, at a smaller scale. constraints, in the record that follows this one, want sums for the same reason (a range, a pin, an any). the honest hedge: if M-4's first cut shipped one source kind, sums could wait, but nothing in the researched precedent and nothing in this milestone's own statement suggests one source kind, since lock data and binary reuse both presuppose a remote source beside the local one.

`Map` is not required by this record. dependency sets are `List`s of records until the constraints record shows a lookup that needs keys, and reserving the constructor now would be the accretion 0026's closure rule exists to prevent.

### what is deliberately not in this record

constraints, version-range semantics, preferences versus hard requirements, the resolution algorithm, and the explanation model are their own record against the dependency-resolution research, which already separates the data model from the solver for exactly this reason. the line this record draws: resolution chooses among package versions, which presupposes what a package version is and adds nothing to identity. the lock's file format, its retention, and the development environment are likewise later records in the same milestone.

## alternatives considered

### content identity as package identity

Nix's position, in its strongest form: the store path is the only identity, and it works at the scale of the largest deployment of functional package management in existence.

rejected on the two directions the context measured. the same content builds different packages (one Debian source, many binaries whose names carry meaning the source does not), and the same package resolves to different content per platform and toolchain, so a digest is neither necessary nor sufficient for package equality. the decisive argument is timing: constraints, locks, and vulnerability statements all speak about packages before content exists, and a content digest is unspeakable then. Nix itself does not escape this; it keeps the naming layer in the expression language and nixpkgs attribute paths, outside the store model, which is precisely the layer this record has to design. computed identity is kept where it is verifiable: the realization and the lock binding.

### a package is a rule

identity would ride 0023, selection would ride 0015, and the package library would sit directly on the kernel the way xylem does.

rejected on 0009 and scope, and on three concrete failures. the package library would become the parent abstraction of the build library, since a build would be "how a package is realized," which 0009 forbids and scope's build-without-package base case contradicts. a package must survive rule revisions and library rewrites without a cache-invalidating meaning: bumping a compile rule's revision must not change what the package is, and under rule identity it would, since revision is derived from the executable semantics that realize the package. and many rules can build one package version (two toolchains are two rule applications of the same package), which is a request-to-rule relation, not an identity.

### a package version is the request that builds it

identity as request identity, which the kernel also already has, and which looks closest to "a package is what you ask for."

rejected as circular, for the reason 0031 rejected keying its index on execution facts. request identity needs the resolved inputs the request's own evaluation produces: the discovered header set, the planned contract, the toolchain resolution. a package version must be nameable before resolution, because the lock is written before the build and the constraint before the resolution; and one package version is realized by many requests (per platform, per toolchain), which makes the relation many-to-one in the wrong direction for identity.

### one identifier with the version inside

Cargo's `PackageId` (name, version, source) or a purl as the single identity, dropping the lineage level.

absorbed rather than rejected. the coordinates-with-version are exactly what a lock line names, and this record keeps them as the version level. what the single-identifier shape loses is the sentences above it: "the same package, new version" as an upgrade, as a lock diff, as a vulnerability range. those are relations over package identities parameterized by version, and an identifier that moves with every version cannot carry them. Cargo's own documentation speaks of packages this way informally, which is the evidence that the level exists even where the identifier does not.

### one hash over the concretized package

Spack's dag hash as the identity: hash name, version, variants, compiler, architecture, and dependencies, and let that be the package.

rejected as identity, absorbed as realization identity. the dag hash is computed after concretization, so nothing can range over it before a solver has run; Spack keeps name and version above it for the same reason pith does. in pith the same content is already covered by computation keys, which are derivable per environment rather than baked into one global string, and which the engine, not the package library, computes.

### managed-object identity for packages

give packages 0013's fifth type, since a registry entry for a package looks like an external object that a deployment-like domain owns.

rejected because none of 0013's defining pressure is present. a package version is immutable once declared, so there is no mutation across observations; no platform re-creates it under a new identifier, so there is no continuity-of-ownership question; and the registry entry is 0005's external identity, a name an external system assigns, recorded in provenance rather than promoted to the package's own identity. stretching the managed-object type here would erode the mutable-immutable distinction it exists to carry, which is the same collapse 0013 rejected when it declined to fold managed objects into external identity.

## consequences

0005's semantic-identity slot gains its first concrete construction for a declared domain object: a name in a domain, stable across version bumps, metadata changes, platform and toolchain changes, and domain-surviving source moves, broken only by rename. the open planning question about what gives a source-level semantic object its identity is answered for packages and stays open for modules and other declarations.

`pith-core` lands records and declared sums, driven by a domain the way `Nominal` and `List` were. `RECORD_ENCODING_VERSION` moves again on the moved-aside-and-rebuilt terms, and the conformance suite extends to the new constructors.

the package library is a consumer of xylem and the kernel, never a wrapper of them. it declares package descriptions as values, resolves them into lock entries, and produces build requests against interfaces xylem already declares. the lock is domain data in the package library, not kernel engine state; nothing in 0024's store model changes, and the engine learns nothing about packages.

binary reuse adds an admission path keyed on lock bindings rather than computation keys, with policy deciding whether an offered realization may substitute for a built one, and provenance recording a substitution as a substitution. 0031's recorded-attempt path is unchanged and remains what serves a build pith itself has run.

xylem is unchanged. a toolchain, a source, and an object keep the identities they have, and the package library composes requests out of them.

## unresolved

the witness for a lock binding is open. a local-only first cut verifies digests against bytes and trusts nothing else, which is honest and weak. the candidates for strength are go's transparency log (witnessed hash lines with inclusion proofs), TUF-style signed metadata (a key threshold over the binding), and in-toto-style attestations binding the build that produced the content. what would settle it is a threat model for M-4's actual sources, which does not exist yet.

variant dimensions are open. whether optional features enter a package version's coordinates (purl qualifiers, conda's used-variable hashes) or stay request inputs that realization identity already covers (Spack variants feed the dag hash, not the name) is a question the constraints record has to answer, because a constraint over a feature is a constraint over coordinates if and only if features are coordinates.

the aliasing mechanism for renames is open. this record says continuity across a rename is explicit and recorded; it does not say where the alias lives, who maintains it, or how a lock migrates across it. Pulumi's aliasing and Cargo's local-only rename bracket the design space, and choosing inside it needs a first rename to study.

whether a lock should ever pin realizations, rather than deriving them, is left with the multi-platform lock question. this record's answer (source only) is argued from the fact that realization identity recomputes, but a cross-compiling environment that wants to ship identical lock data to heterogeneous machines may measure a need this argument does not see.

`Map` waits on the constraints record, and this record claims no evidence for it.
