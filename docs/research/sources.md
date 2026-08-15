---
schema: design-doc/v1
id: research-sources
title: source ledger
summary: primary sources already used and sources queued for a closer historical pass
kind: research
status: active
evidence: preliminary
created: 2026-03-02
updated: 2026-08-14
tags:
  - research
  - sources
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# source ledger

this file tracks sources already used or queued for a deeper pass. a link here does not mean every claim in the source has been accepted.

## project material

- [design principles](../foundation/principles.md)
- [scope](../foundation/scope.md), [declarative semantics](../decisions/0002-declarative-semantics.md), and the [generic-kernel decision](../decisions/0001-generic-kernel.md)

## Nix

- [Nix language manual](https://nix.dev/manual/nix/latest/language/)
- [Nix store manual](https://nix.dev/manual/nix/latest/store/)
- [Nix advanced attributes: fixed-output derivations](https://nix.dev/manual/nix/stable/language/advanced-attributes)
- [NixOS module-system manual](https://nixos.org/manual/nixos/stable/#sec-writing-modules)
- [Eelco Dolstra's publications](https://edolstra.github.io/pubs/)

## build systems and incremental computation

- [Bazel basics](https://bazel.build/basics)
- [Bazel build systems](https://bazel.build/versions/7.4.0/basics/build-systems)
- [Bazel Skyframe](https://bazel.build/versions/8.2.0/reference/skyframe)
- [Bazel hermeticity](https://bazel.build/basics/hermeticity)
- [Bazel BUILD files](https://bazel.build/concepts/build-files)
- [Buck2 rationale](https://buck2.build/docs/about/why/)
- [Buck2 architecture](https://buck2.build/docs/concepts/architecture/)
- [Buck2 DICE](https://buck2.build/docs/insights_and_knowledge/modern_dice/)
- [Buck2 remote execution](https://buck2.build/docs/users/remote_execution/)
- [Buck2 BXL](https://buck2.build/docs/bxl/)
- [Buck2 dependency files](https://buck2.build/docs/rule_authors/dep_files/)
- [Buck2 incremental actions](https://buck2.build/docs/rule_authors/incremental_actions/)
- [Meta's Buck2 retrospective](https://engineering.fb.com/2023/04/06/open-source/buck2-open-source-large-scale-build-system/)
- [Pants v2 design lessons](https://www.pantsbuild.org/blog/2020/10/27/introducing-pants-v2)
- [Pants engine overview](https://www.pantsbuild.org/dev/docs/introduction/how-does-pants-work)
- [Adapton](https://matthewhammer.org/adapton/)
- [Build Systems à la Carte](https://www.microsoft.com/en-us/research/publication/build-systems-a-la-carte/)
- [Bazel Remote Execution API](https://github.com/bazelbuild/remote-apis)

## configuration languages

- [Dhall safety guarantees](https://docs.dhall-lang.org/discussions/Safety-guarantees.html)
- [Nickel manual](https://nickel-lang.org/user-manual/introduction/)
- [CUE configuration use case](https://cuelang.org/docs/concept/configuration-use-case/)
- [Starlark specification](https://github.com/bazelbuild/starlark/blob/master/spec.md)

## desired state and deployment

- [Terraform language](https://developer.hashicorp.com/terraform/language)
- [Terraform state](https://developer.hashicorp.com/terraform/language/state)
- [purpose of Terraform state](https://developer.hashicorp.com/terraform/language/state/purpose)
- [Terraform providers](https://developer.hashicorp.com/terraform/language/providers)
- [Ansible playbooks](https://docs.ansible.com/projects/ansible/latest/playbook_guide/playbooks_intro.html)
- [Kubernetes controllers](https://kubernetes.io/docs/concepts/architecture/controller/)

## dependency resolution

- [libsolv](https://github.com/openSUSE/libsolv)
- [openSUSE SAT solver documentation](https://doc.opensuse.org/projects/satsolver/SLE11SP3/html/index.html)
- [PubGrub algorithm](https://dart.googlesource.com/pub.git/%2B/f27dcfdb/doc/solver.md)
- [Spack concretizer](https://spack.readthedocs.io/en/latest/glossary.html#term-concretizer)
- [Using Answer Set Programming for HPC dependency solving](https://arxiv.org/abs/2210.08404)

## artifacts and trust

- [OCI image specification](https://github.com/opencontainers/image-spec)
- [OSTree](https://ostreedev.github.io/ostree/)
- [BuildStream](https://docs.buildstream.build/)
- [The Update Framework](https://theupdateframework.io/)
- [TUF specification](https://theupdateframework.github.io/specification/latest/)
- [Go checksum database design](https://go.googlesource.com/proposal/+/master/design/25530-sumdb.md)
- [transparent-log design](https://research.swtch.com/tlog)
- [cargo registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- [Debian repository format](https://wiki.debian.org/DebianRepository/Format)
- [git objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)
- [in-toto](https://in-toto.io/)
- [SLSA](https://slsa.dev/spec/)
- [Sigstore](https://docs.sigstore.dev/)
- [SPIFFE](https://spiffe.io/docs/latest/spiffe-specs/)

## tooling

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [Tree-sitter](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter syntax injection](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection)
- [Buck2 BXL](https://buck2.build/docs/bxl/)
- [Buck2 Starlark development](https://buck2.build/docs/developers/starlark/)

## writing reference

- [Wikipedia: signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing)
