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
- [Nix manual: garbage collection](https://nix.dev/manual/nix/2.24/package-management/garbage-collection.html)
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
- [Bazel Skyframe reference](https://bazel.build/reference/skyframe)
- [Bazel: optimize memory, `--discard_analysis_cache` and Skyfocus](https://bazel.build/advanced/performance/memory)
- [Salsa RFC0004: LRU](https://github.com/salsa-rs/salsa-rfcs/blob/master/RFC0004-LRU.md)
- [Salsa tuning: LRU and durability](https://salsa-rs.netlify.app/tuning)
- [rust-analyzer: durable incrementality](https://rust-analyzer.github.io/blog/2023/07/24/durable-incrementality.html)
- [rustc dev guide: incremental compilation in detail](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html)
- [Build systems à la carte: theory and practice](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/097CE52C750E69BD16B78C318754C7A4/S0956796820000088a.pdf/build_systems_a_la_carte_theory_and_practice.pdf)

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

## package builds

- [nixpkgs stdenv setup.sh](https://raw.githubusercontent.com/NixOS/nixpkgs/master/pkgs/stdenv/generic/setup.sh)
- [Spack packaging guide](https://spack.readthedocs.io/en/latest/packaging_guide.html)
- [Guix build systems](https://guix.gnu.org/manual/en/html_node/Build-Systems.html)
- [Guix g-expressions](https://guix.gnu.org/manual/en/html_node/G_002dExpressions.html)
- [Debian policy chapter 5: source packages](https://www.debian.org/doc/debian-policy/ch-source.html)
- [debhelper dh(1)](https://manpages.debian.org/testing/debhelper/dh.1.en.html)
- [cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)
- [gentoo development guide](https://devmanual.gentoo.org/ebuild-writing/functions/)
- [homebrew formula cookbook](https://docs.brew.sh/Formula-Cookbook)

## index formats

- [cargo registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- [RFC 2141: alternative registries](https://rust-lang.github.io/rfcs/2141-alternative-registries.html)
- [RFC 2789: sparse registry](https://rust-lang.github.io/rfcs/2789-sparse-index.html)
- [Debian repository format](https://wiki.debian.org/DebianRepository/Format)
- [Debian policy 5: control files and fields](https://www.debian.org/doc/debian-policy/ch-controlfields.html)
- [dpkg-gencontrol(1)](https://manpages.org/dpkg-gencontrol/1) and [dpkg-scanpackages(1)](https://manpages.org/dpkg-scanpackages/1)
- [Alpine Apk_spec](https://wiki.alpinelinux.org/wiki/Apk_spec) and [apk-package(5)](https://github.com/alpinelinux/apk-tools/blob/master/doc/apk-package.5.scd)
- [PEP 503](https://peps.python.org/pep-0503/), [PEP 658](https://peps.python.org/pep-0658/), [PEP 691](https://peps.python.org/pep-0691/), and [PEP 714](https://peps.python.org/pep-0714/)
- [libsolv](https://github.com/openSUSE/libsolv)
- [dart pub solver](https://github.com/dart-lang/pub/blob/master/doc/solver.md) and [hosted repository spec v2](https://github.com/dart-lang/pub/blob/master/doc/repository-spec-v2.md)
- [pubgrub-rs guide](https://pubgrub-rs-guide.netlify.app/)
- [CUDF 2.0 specification](https://www.mancoosi.org/reports/tr3.pdf)

## arithmetic and numeric representation

- [PEP 237: unifying long integers and integers](https://peps.python.org/pep-0237/) and [CPython `longintrepr.h`](https://github.com/python/cpython/blob/main/Include/cpython/longintrepr.h)
- [Nix language types](https://nix.dev/manual/nix/latest/language/types), [NixOS/nix#11188](https://github.com/NixOS/nix/pull/11188), and the [Nix 2.25 release notes](https://nix.dev/manual/nix/2.29/release-notes/rl-2.25.html)
- [Dhall standard](https://github.com/dhall-lang/dhall-lang/blob/master/standard/README.md), [beta normalization](https://github.com/dhall-lang/dhall-lang/blob/master/standard/beta-normalization.md), and [binary encoding](https://github.com/dhall-lang/dhall-lang/blob/master/standard/binary.md)
- [CUE specification](https://cuelang.org/docs/reference/spec/)
- [Starlark specification](https://github.com/bazelbuild/starlark/blob/master/spec.md) and [starlark-go `int.go`](https://github.com/google/starlark-go/blob/master/starlark/int.go)
- [RFC 8949: CBOR](https://www.rfc-editor.org/rfc/rfc8949.html)
- [`java.math.BigInteger`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/math/BigInteger.html)
- [protobuf encoding](https://protobuf.dev/programming-guides/encoding/)

## tooling

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [Tree-sitter](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter syntax injection](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection)
- [Buck2 BXL](https://buck2.build/docs/bxl/)
- [Buck2 Starlark development](https://buck2.build/docs/developers/starlark/)

## writing reference

- [Wikipedia: signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing)
