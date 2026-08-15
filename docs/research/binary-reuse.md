---
schema: design-doc/v1
id: research-binary-reuse
title: binary reuse and binary caches
summary: how Nix, Bazel, ccache, the distribution repositories, and the attestation and rebuilder systems each decide whether a prebuilt binary may stand in for a build
kind: research
status: researching
evidence: preliminary
created: 2026-07-03
updated: 2026-07-03
tags:
  - research
  - caching
  - artifacts
  - trust
relations:
  informed_by: []
  depends_on:
    - research-method
    - research-nix
    - research-artifacts-and-trust
  supersedes: []
---

# binary reuse and binary caches

every system that serves prebuilt artifacts divides the same question the same way: what is verified against the bytes (a digest, always), what is trusted from a signature or a policy, and whether anyone ever checks that the binary is what building the source would produce. the systems disagree about the second question and almost none exercise the third at consumption time. the disagreement is structural: it tracks whether the artifact supplier sits inside the system's trust perimeter.

## Nix: substitution as an optimization gated by signatures

a substituter is an additional store Nix fetches store objects from instead of building. substitution is framed as an optimization — `substitute = false` forces building from source, and a cache miss "will fall back to building from source" — but it carries an explicit trust gate. a store object is accepted from a substituter only if it carries a signature by a key in `trusted-public-keys`, or the substituter is configured `trusted=true`, or the object is content-addressed, or `require-sigs` is off. the narinfo signature covers the store path fingerprint; independently, every substituted path is re-hashed at import ("hash mismatch importing path") so the signature authenticates who may supply the path and the NAR hash verifies the bytes that arrived. which users may configure substituters at all is itself policy: unprivileged users may use only `trusted-substituters`, and `trusted-users` rights are documented as equivalent to root access.

content-addressed derivations (RFC 0062) sharpen the split: CA paths "don't need a signature" because their hash is checked against the content itself, while the realisation — the mapping from derivation to output path — is signed, because no deterministic relation lets a client derive it. the manual notes that for floating CA outputs "multiple builds may be performed and compared" to establish determinism, but Nix does not rebuild by default: the first build is accepted as truthful. for input-addressed paths the RFC is blunt that verification short of rebuilding does not exist.

## Bazel: the action cache as trusted infrastructure

the remote execution API's action cache maps a digest of the serialized `Action` — command digest plus every input digest — to an `ActionResult` whose outputs live in the CAS under their own digests. downloads are digest-verified by default (`--remote_verify_downloads`), which is integrity, not authenticity: nothing signs an ActionResult, so anyone with write access can store an arbitrary result under a valid key and all its blobs will verify. the official docs treat the cache operationally ("the remote cache will have your binaries and so needs to be secure"; write-restrict to CI; recreate a cache after poisoning), and the community analysis of a shared cache says plainly that Bazel "will blindly trust the ActionResult messages". `--guard_against_concurrent_changes` (ctime checks before upload) guards against self-inflicted pollution from concurrent local edits, not against a malicious cache. so Bazel's position: the cache is inside the perimeter, protected by access control.

## ccache and sccache: correctness by key completeness

ccache has two key strategies. preprocessor mode hashes the preprocessor output; direct mode hashes the source and compiler options and looks up a manifest recording which include files were read and their hash sums at storage time, checking current contents against the manifest on a hit. the key hashes the compiler identity (mtime and size by default, content optionally), and the docs state the design goal as "as few false cache hits as possible" with `sloppiness` as an explicit safety/hit-rate dial. stored cache data is checksummed against corruption only; there are no signatures, and a miss simply runs the compiler. sccache is the same wrapper shape over more backends, with `SCCACHE_RECACHE` to purge a polluted shared cache — the knob that admits the cache can hold bad artifacts. both trust the cache entirely and put all correctness in the key.

## Debian and Arch: untrusted suppliers, anchored indexes

Debian strips maintainer signatures at archive intake, puts per-package checksums in the Packages indices, hashes those into the Release file, and signs Release with the archive key (`InRelease`). apt verifies the chain automatically and aborts on any mismatch, so a compromised mirror cannot serve modified packages: the hashes come from the signed index, not from the mirror. what remains trusted is the archive key, and apt-secure says so — it does not defend against compromise of the master server, and trusting the archive maintainer is not trusting the code. Arch signs each package individually (packager keys under master keys under the locally populated keyring) with `SigLevel = Required DatabaseOptional` by default, so the signature travels with the artifact rather than with an index. both models make the artifact supplier untrusted; they differ in what the signature anchors.

## SLSA and in-toto: the binary as a signed claim

SLSA provenance is a signed claim — in-toto statement, subject digest, builder identity, external parameters, resolved dependencies — whose verification workflow is: check the envelope signature against a configured root of trust, check the subject digest matches the artifact in hand, then check the builder and parameters against expectations. the build levels grade the platform, not the artifact: L1 provenance exists, L2 hosted platform signs it, L3 hardened builds. the framing is "trust platforms, verify artifacts," and provenance authenticates the build record — source trust and artifact safety are explicitly out of scope. in-toto generalizes the shape: links signed by functionaries record materials, command, and products, and a signed layout is the policy a verifier walks, with artifact rules and thresholds. the subject-digest-against-builder-claim shape is exactly an admission test's authorization leg.

## rebuilderd: the claim converted to a measurement

rebuilderd rebuilds distribution packages from source and reports GOOD, BAD, or UNKNWN per package. a GOOD rebuild produces an in-toto link signed by the worker key recording the source as material and the rebuilt binary as product; the daemon co-signs. the trust story is quorum-shaped: multiple rebuilders you choose report reproducible, and there is "no definite truth" — you query instances, including your own. converting these attestations into a consumption-time gate is unfinished: fetching attestations by artifact digest, transparency logs, and a standard multi-rebuilder predicate are open issues. no surveyed system verifies by rebuild at consumption time; Debian and Arch verify digests of the exact distributed bytes, and the compiler caches never re-verify at all.

## the disagreement, stated

- Nix, Debian, Arch: the supplier is untrusted; authenticity comes from signatures or policy anchored outside the supplier, digest checks anchor the bytes. the cache is an optimization *and* a trust boundary at once.
- Bazel, ccache, sccache: the cache is inside the perimeter; digests give integrity, access control gives authenticity, and correctness rests on key completeness.
- reproducible-builds, rebuilderd: the binary is a claim; rebuild is the only thing that turns the claim into a measurement, and sharing that measurement across parties is the open problem.

the disagreement is not about mechanism — every system digests the bytes — but about where the perimeter sits and who is allowed to say a binary may be used. a design that names its perimeter explicitly can express all three positions; one that inherits a cache silently has chosen the second without saying so.

## questions

- what does an offer claim, exactly: which source, which realization coordinates, and what evidence?
- which admission facts are measurable locally and which must be authorized, and where does the authorization live when no keys exist?
- can a rejected offer be remembered without creating state no input names?
- does a selection that depended on offer availability remain reproducible under the same candidate universe?
- what upgrades when key infrastructure arrives, and does it touch any leg other than the authorization one?

## sources

- [Nix conf reference: substituters, trusted-public-keys, require-sigs](https://nix.dev/manual/nix/latest/command-ref/conf-file)
- [Nix binary cache substituter](https://nix.dev/manual/nix/latest/package-management/binary-cache-substituter.html)
- [Nix local-store import hash checks](https://github.com/NixOS/nix/blob/master/src/libstore/local-store.cc)
- [Nix RFC 0062: content-addressed paths](https://github.com/NixOS/rfcs/blob/master/rfcs/0062-content-addressed-paths.md)
- [Nix content-addressed outputs manual](https://manual.determinate.systems/store/derivation/outputs/content-address.html)
- [Nix CA call for testers](https://discourse.nixos.org/t/content-addressed-nix-call-for-testers/12881)
- [Remote Execution API v2 proto](https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto)
- [Bazel remote caching](https://bazel.build/remote/caching)
- [Bazel command-line reference: remote_verify_downloads, guard_against_concurrent_changes](https://bazel.build/reference/command-line-reference)
- [bazel-discuss: security risks of a shared remote cache](https://groups.google.com/g/bazel-discuss/c/0005BAB_mlI)
- [ccache manual](https://ccache.dev/manual/4.11.1.html)
- [sccache](https://github.com/mozilla/sccache)
- [Debian SecureApt](https://wiki.debian.org/SecureApt)
- [apt-secure(8)](https://manpages.debian.org/bookworm/apt/apt-secure.8.en.html)
- [Arch package signing](https://wiki.archlinux.org/title/Pacman/Package_signing)
- [repo-add(8)](https://man.archlinux.org/man/repo-add.8.en)
- [SLSA provenance v1](https://slsa.dev/spec/v1.0/provenance)
- [SLSA build levels](https://slsa.dev/spec/v1.0/levels)
- [SLSA verifying artifacts](https://slsa.dev/spec/v1.0/verifying-artifacts)
- [in-toto specification](https://github.com/in-toto/docs)
- [in-toto attestation framework](https://github.com/in-toto/attestation)
- [rebuilderd](https://github.com/kpcyrd/rebuilderd)
- [rebuilderd PR 186: better attestations](https://github.com/kpcyrd/rebuilderd/pull/186)
- [in-toto/attestation issue 3: reproducible builds as provenance](https://github.com/in-toto/attestation/issues/3)
- [reproducible-builds: which problems are solved](https://reproducible-builds.org/docs/which-problems-do-reproducible-builds-solve/)
- [reproducible-builds: sharing certifications](https://reproducible-builds.org/docs/sharing-certifications/)
