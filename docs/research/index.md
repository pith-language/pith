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
- [desired state and deployment](deployment-and-state.md)
- [reproducibility lineage](reproducibility.md)
- [artifacts, identity, and trust](artifacts-and-trust.md)
- [tooling and inspectability](tooling.md)
- [source ledger](sources.md)

## research depth

the build-system note has the strongest evidence so far. it uses first-party design documents from Bazel, Buck2, Pants, and the papers behind their incremental engines.

the deployment note is the next strongest. it covers Terraform, Kubernetes, Pulumi, and Crossplane from primary sources and operator-documented failure modes, and it grounds the deployment decisions 0012 and 0013.

the reproducibility note covers the Reproducible Builds project and the SOURCE_DATE_EPOCH specification. it grounds decision 0014.

the Nix baseline comes from the original project notes plus Nix documentation. it needs a closer pass over the thesis, early mailing-list discussions, module-system history, flakes, and content-addressed derivations.

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
