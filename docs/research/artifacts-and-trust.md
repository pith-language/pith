---
schema: design-doc/v1
id: research-artifacts-and-trust
title: artifacts, identity, and trust
summary: early research into content addressing, provenance, supply-chain evidence, and secret identity
kind: research
status: researching
evidence: preliminary
created: 2026-03-06
updated: 2026-07-27
tags:
  - research
  - artifacts
  - security
relations:
  informed_by: []
  depends_on:
    - research-method
    - research-nix
  supersedes: []
---

# artifacts, identity, and trust

Nix made the dependency closure and store path part of everyday package management. newer build systems, OCI tooling, and supply-chain systems use content digests too, but they attach different meaning to those digests.

the current design separates semantic identity from content identity. two artifacts can implement the same semantic component, and a single semantic component can have different contents for different platforms. a remote API object needs a third identity that survives observation and mutation.

## immutable data

the kernel should store blobs, directory trees, and serializable values by digest. an artifact is a domain interpretation of those objects.

archive files, filesystem layers, package formats, VM images, and container manifests should remain derived representations. choosing one of them as the universal model would leak its ordering, metadata, and platform assumptions into unrelated domains.

materialization is separate. remote workers and consumers should be able to pass digests without downloading every intermediate object.

## action identity

remote build APIs typically identify an action from its command, declared inputs, and execution properties. the action cache maps that identity to output digests.

this works only when the declaration includes every influence. network access, undeclared tools, host paths, mutable clocks, and secrets can make identical action keys produce different results.

the capability model therefore needs to affect cache and trust semantics. an action with unrestricted network access should not receive the same reproducibility claim as a sandboxed compiler invocation.

## provenance

provenance should be emitted from the graph while artifacts are created. dependency manifests, software bills of materials, and attestations become views of recorded evidence.

signing a digest proves who endorsed those bytes. it does not prove that the rule was hermetic, that the dependencies were trustworthy, or that a deployment used the signed object. those are separate claims and need separate evidence.

## secrets and workload identity

secret plaintext should not enter ordinary immutable values or artifact identities. configuration contains a typed reference describing the secret, intended consumer, scope, and provider requirements.

the target resolves the reference late through workload identity where possible. this avoids copying long-lived credentials through evaluation, build workers, state files, and deployment logs.

the project needs to research SPIFFE-style workload identity, secret-store leases, key rotation, and offline targets before choosing a concrete protocol.

## questions

- which tree representation supports filesystems without making every artifact Unix-specific?
- should digests include a schema and hash algorithm identifier?
- how are nondeterministic but valid outputs represented and compared?
- which provenance claims are measured by the engine and which are declarations by an adapter?
- how are signatures and attestations preserved when an artifact is converted to another format?
- how does a secret influence action identity without revealing its contents or destroying useful caching?
- what trust policy is evaluated before reuse from a remote cache?

## sources

- [Nix store](https://nix.dev/manual/nix/latest/store/)
- [Bazel Remote Execution API](https://github.com/bazelbuild/remote-apis)
- [OCI image specification](https://github.com/opencontainers/image-spec)
- [OSTree documentation](https://ostreedev.github.io/ostree/)
- [BuildStream documentation](https://docs.buildstream.build/)
- [The Update Framework](https://theupdateframework.io/)
- [in-toto](https://in-toto.io/)
- [SLSA specification](https://slsa.dev/spec/)
- [Sigstore documentation](https://docs.sigstore.dev/)
- [SPIFFE specifications](https://spiffe.io/docs/latest/spiffe-specs/)
