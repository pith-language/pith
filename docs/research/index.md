---
schema: design-doc/v1
id: research-index
title: research index
summary: research status, reading order, and planned design lineages
kind: research
status: active
evidence: preliminary
created: 2026-03-02
updated: 2026-08-18
tags:
  - research
relations:
  informed_by: []
  depends_on:
    - research-method
  supersedes: []
---

# research index

the point of this research is to understand decisions, not collect feature lists.

each lineage starts with the pressure that created a system. it records the mechanism chosen, alternatives available at the time, consequences, and what later systems kept or changed. the final section translates that history into questions for this project.

## current notes

- [research method](method.md)
- [Nix baseline](nix.md)
- [build-system lineage](build-systems.md)
- [how deep a validity check goes](incremental-revalidation.md)
- [configuration and composition](configuration.md)
- [dependency resolution](dependency-resolution.md)
- [index formats](index-formats.md)
- [desired state and deployment](deployment-and-state.md)
- [reproducibility lineage](reproducibility.md)
- [artifacts, identity, and trust](artifacts-and-trust.md)
- [binary reuse and binary caches](binary-reuse.md)
- [development environments](dev-environments.md)
- [package builds](package-builds.md)
- [declarations and their identity](declarations.md)
- [unbounded integers](arithmetic.md)
- [tooling and inspectability](tooling.md)
- [source ledger](sources.md)
- [composing a system from declared parts](system-composition.md)
- [diagnostic spans](diagnostic-spans.md)
- [extension interfaces](extension-interfaces.md)

the development-environments note draws mkShell from its nixpkgs source, `nix develop` and `nix print-dev-env` from the Nix manual, direnv's caching from its wiki, Guix's shells and containers, Spack's `spack.yaml`/`spack.lock` split, conda's environment files and their lockfile history, rustup's toolchain overrides, mise and asdf's pin files, the devcontainers specification, and PEP 405 from primary sources. it grounds decision 0043 and records the disagreement those systems never settled about whether an environment is a lock consumer, a lock producer, or a lock of its own.

the incremental-revalidation note draws Salsa's LRU RFC and rust-analyzer's account of lazy revalidation, the rustc dev guide on `try_mark_green` and the previous dependency graph, Bazel's Skyframe reference and its memory page including Skyfocus, the Nix manual's garbage-collection chapter, and the *Build Systems à la Carte* rebuilder taxonomy from primary sources. it grounds decision 0051 and records the disagreement those systems never settled about where the transitive cost of a validity check is paid — at check time as a walk and a retained closure, or at record time as a summary and a lost early cutoff.

the arithmetic note draws PEP 237 and CPython's `longintrepr.h` from python.org and the CPython tree, the Nix language manual and the pull request that banned integer overflow from nix.dev and the NixOS repository, Dhall's standard README, beta-normalization, and binary encoding from the dhall-lang standard, the CUE and Starlark specifications with starlark-go's `int.go` beside the latter, RFC 8949's bignum and deterministic-encoding sections, `java.math.BigInteger`, and protobuf's encoding guide from primary sources. it grounds decision 0055 and records what those systems agree on rather than what they dispute: that unboundedness makes addition, subtraction, and multiplication total and leaves division partial in every one of them, and that an unbounded integer needs a stated normal form, because equality, hashing, and any digest over the value all fail when one number has two representations.

the package-builds note draws stdenv's phases from nixpkgs's setup.sh, Spack's package.py and builder from its documentation and source, Guix's build systems and g-expressions from its manual, Debian's source-package format and `debian/rules` from policy and the debhelper and dpkg manuals, cargo's `.crate` and `build.rs` from its reference, portage's ebuild phases from the gentoo development guide, and Homebrew's formula DSL from its cookbook and API docs. it grounds decision 0045 and records the disagreement those systems never settled about whether a package's build is data the system can inspect or code it can only run.

the system-composition note draws NixOS's module merge and priorities from the manual and `lib/modules.nix`, the toplevel and activation from `top-level.nix` and the switch chapters, the JFP and HotOS papers, Guix's service extensions and fold from the manual and `gnu/services.scm`, OSTree's object, deployment, and delta model from its documentation, BuildStream's staging and overlap rule from its docs, and the OCI image-spec's manifest and layer chapters from primary sources. it grounds decision 0052 and records the disagreement those systems never settled about what a composed system is — a tree, a layered sequence, or assertions over a target — finding that four of the five collapse to a tree somewhere, that the artifact's shape and the composition's conflict rule are independent choices, and that assertion-shaped behavior appears only at the activation boundary every one of them keeps.

the extension-interfaces note draws Bazel's rules and extension-concepts pages, the 2020 bazel-discuss announcement of the plan to move the native rules into Starlark and the Bazel 8.0 release that completed it four years later, Buck2's why-buck2 and rule-authoring pages, Terraform's plugin architecture and plugin-protocol documentation, the Kubernetes custom-resources concept page, PostgreSQL's chapter on how extensibility works, and the Nix manual's derivation page from primary sources. it grounds decision 0056 and separates two questions the mechanisms conflate: parity, whether an extension reaches what the built-ins reach, and isolation, whether it can damage what it should not. the out-of-process protocol answers isolation and fixes the shapes an extension may take; the in-process interface can reach parity and proves it only where the built-ins come through the same door. none of the five ships a test whose failure would mean an extension had become second class.

## research depth

the declarations note is the weakest against this file's own standard, and it grounds 0047. it reads protobuf's and cap'n proto's field-number discipline, WIT's registry-qualified names, Dhall's normalization and its 1.17.0 basis change, and Unison's content-addressed definitions, and it quotes all four at length — but it carries no urls and no sources section, and the ledger has no declarations entry, so a reader cannot get from the record to any primary document or check a quote's attribution and date. the method requires both. closing that is the note's next pass, and until then its quotations should be read as unverified against this file's standard rather than as the evidence the other notes provide.

the build-system note has the strongest evidence so far. it uses first-party design documents from Bazel, Buck2, Pants, and the papers behind their incremental engines.

the deployment note is the next strongest. it covers Terraform, Kubernetes, Pulumi, and Crossplane from primary sources and operator-documented failure modes, and it grounds the deployment decisions 0012 and 0013.

the reproducibility note covers the Reproducible Builds project and the SOURCE_DATE_EPOCH specification. it grounds decision 0014.

the binary-reuse note draws Nix's substitution gates, Bazel's action cache, ccache and sccache, Debian's and Arch's signed repositories, SLSA and in-toto, and rebuilderd from primary sources. it grounds decision 0042 and records the disagreement those systems never settled about whether a binary cache is an optimization or a trust boundary.

the Nix baseline draws on Nix documentation and the project's own analysis of Nix's ideas. it needs a closer pass over the thesis, early mailing-list discussions, module-system history, flakes, and content-addressed derivations.

configuration, artifact, and trust notes contain confirmed mechanisms and working questions. they are not finished historical accounts yet.

## next lineages

the current order is based on architectural leverage:

1. Make, Shake, Tup, Blaze/Bazel, Buck/Buck2, and Pants
2. Nix expressions and modules, Guix, Dhall, Nickel, CUE, Jsonnet, and Starlark
3. SAT and libsolv, PubGrub, Spack, and language package managers
4. CFEngine and Puppet, Terraform, Kubernetes controllers, and deployment systems
5. Nix stores, OSTree, BuildStream, OCI, and image-based operating systems
6. signed package repositories, TUF, in-toto, SLSA, Sigstore, and workload identity

the arrows in these lineages mean influence or reaction unless a source establishes direct ancestry.

the diagnostic-spans note draws rustc's span and diagnostics chapters from the rustc dev guide, cpplib's line numbering from the GCC internals manual, miette's `Diagnostic` trait and `SourceCode` from its api documentation, and the Diagnostic structure and position-encoding negotiation from the LSP 3.17 specification. it grounds decision 0053 and records the disagreement those systems never settled about where the text a diagnostic points into lives — a session table the producer owns, an attachment the renderer adds, or a document the client already holds — and when position becomes line and column: eagerly in negotiated units, or lazily from offsets at render.
