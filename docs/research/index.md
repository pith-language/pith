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
- [tooling and inspectability](tooling.md)
- [source ledger](sources.md)

the development-environments note draws mkShell from its nixpkgs source, `nix develop` and `nix print-dev-env` from the Nix manual, direnv's caching from its wiki, Guix's shells and containers, Spack's `spack.yaml`/`spack.lock` split, conda's environment files and their lockfile history, rustup's toolchain overrides, mise and asdf's pin files, the devcontainers specification, and PEP 405 from primary sources. it grounds decision 0043 and records the disagreement those systems never settled about whether an environment is a lock consumer, a lock producer, or a lock of its own.

the package-builds note draws stdenv's phases from nixpkgs's setup.sh, Spack's package.py and builder from its documentation and source, Guix's build systems and g-expressions from its manual, Debian's source-package format and `debian/rules` from policy and the debhelper and dpkg manuals, cargo's `.crate` and `build.rs` from its reference, portage's ebuild phases from the gentoo development guide, and Homebrew's formula DSL from its cookbook and API docs. it grounds decision 0045 and records the disagreement those systems never settled about whether a package's build is data the system can inspect or code it can only run.

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
