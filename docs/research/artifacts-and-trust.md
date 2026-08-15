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

## witnessing a source binding

a fetched source forces one question a digest alone cannot answer: once bytes have been measured, who witnesses the binding of coordinates to digest — the first fact that a lock entry records. the systems that shipped this problem disagree about the answer, and the disagreement is about detection versus prevention.

Go's checksum database is the detection position. it is an append-only merkle tree whose leaves are the same lines a lock entry is — module, version, hash — and a client verifies an inclusion proof by recomputing the root from the leaf and the path, against a signed tree head it pins or gossips about. its design doc states the position plainly: the focus turned to detection of compromise. a compromised registry can feed one victim a forked log only by serving that fork forever, and gossip and independent proxies exist to raise the cost of that lie. it detects a registry rewriting history. it detects nothing about a module that is malicious and honestly hashed.

TUF is the prevention position. four signing roles — root, targets, snapshot, timestamp — with thresholds, key expiry, rollback and freeze protection, all inside a closed key set whose root keys are kept offline. an attacker below threshold fails, and the spec is explicit that protection means the attack fails, not that the update succeeds. what it costs is a key story: distributed keys, an expiry discipline, a root of trust someone operates. that fits a repository one party operates and does not fit witnessing other people's registries.

Debian's repository format is TUF's ancestor at one archive key. a signed Release file hashes every package index, every index entry hashes its package, and a mirror without the key can withhold but not forge. apt verifies the chain automatically and aborts on mismatch, so the hashes come from the signed index, never from the mirror. what remains trusted is the archive key itself, and the design says so.

crates.io is the trust-the-registry position: the registry computes and publishes each version's checksum, versions are immutable by policy with `yanked` the one mutable field, TLS carries the transport, and no independent witness exists. the registry is inside the perimeter by construction.

Nix makes the fetch itself safe from below rather than witnessing the binding afterward. a fixed-output derivation — `fetchurl`, `fetchgit` — declares its output hash in advance, which is why the manual lets these derivations, and only these, out of the network namespace: the declared hash turns any network activity into a verifiable claim, so the network cannot influence the result undetected no matter what it serves.

git is the one entry where the content address and the authentication are the same fact. an object's name is the hash of its bytes, a tree's hash covers everything beneath it, a commit's hash covers its tree and parents, and a receiving side verifies every object against its name. the ref is the part git does not authenticate — a mutable pointer no commit records — which is why a source pin over git binds the revision, and the ref survives only as provenance.

0044 takes a position among these rather than averaging them: pith detects tampering after a binding exists and detects a source that disagrees with the witness, prevents nothing, and vouches for no content. the witness is the log, go's shape down to the leaf spelling — a lock's binding lines are the log's leaves — with the checkpoint pinned by configuration on the same terms 0042 pinned origins. git needs no log because the revision authenticates its content intrinsically, and the fixed-output derivation is the shape the fetch takes when it moves into the graph as an action.


## secrets and workload identity

secret plaintext should not enter ordinary immutable values or artifact identities. configuration contains a typed reference describing the secret, intended consumer, scope, and provider requirements.

the target resolves the reference late through workload identity where possible. this avoids copying long-lived credentials through evaluation, build workers, state files, and deployment logs.

the project needs to research SPIFFE-style workload identity, secret-store leases, key rotation, and offline targets before choosing a concrete protocol.

## questions

- which tree representation supports filesystems without making every artifact Unix-specific?
- should digests include a schema and hash algorithm identifier? (the lock file and the log both spell an algorithm prefix per digest; whether a schema identifier belongs beside it stays open)
- how are nondeterministic but valid outputs represented and compared?
- which provenance claims are measured by the engine and which are declarations by an adapter? ([0044](../decisions/0044-the-first-source-adapter.md): the adapter measures, the engine never does, and the boundary is 0003's — a fetch, a digest, and a witness check are caller-side effects producing declared inputs)
- how are signatures and attestations preserved when an artifact is converted to another format?
- how does a secret influence action identity without revealing its contents or destroying useful caching?
- what trust policy is evaluated before reuse from a remote cache? ([0042](../decisions/0042-binary-reuse-as-admitted-substitution.md)'s admission test, which [0044](../decisions/0044-the-first-source-adapter.md) keeps unchanged, now over origins that name real sources; the witness for the source binding itself is the locally checked transparency log, with checkpoint authenticity still open there)

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
- [Go checksum database design](https://go.googlesource.com/proposal/+/master/design/25530-sumdb.md)
- [transparent-log design](https://research.swtch.com/tlog)
- [The Update Framework specification](https://theupdateframework.github.io/specification/latest/)
- [cargo registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- [Nix advanced attributes: fixed-output derivations](https://nix.dev/manual/nix/stable/language/advanced-attributes)
- [Debian repository format](https://wiki.debian.org/DebianRepository/Format)
- [git objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)
