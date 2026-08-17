---
schema: design-doc/v1
id: foundation-name
title: name and brand
summary: the working name, word forms, file extension, store paths, ecosystem vocabulary, forge strategy, and domain plan
kind: foundation
status: active
created: 2026-02-26
updated: 2026-08-18
tags:
  - name
  - brand
  - identity
relations:
  informed_by:
    - foundation-scope
    - foundation-principles
  depends_on:
    - foundation-problem
  supersedes: []
---

# name and brand

the working name is **Pith**. this document fixes how the name appears in prose, code, files, storage, the ecosystem, and on the web. it can still be replaced by a later decision record.

## why this name

the pith is the central tissue of a plant stem. the project is looking for the smaller mechanism underneath build, package, system, and deployment tools, so the metaphor fits closely enough. the common phrase "the pith of it" points at the same idea.

## word forms

- in prose, the name is written **Pith**.
- in commands, identifiers, and files, it is written `pith`.
- there is no separate short form. the word is four letters.

## file extension

source files use `.pi`. generated files use a distinct name so they are visibly not source.

- source: `config.pi`
- lock file: `pith.lock`
- store paths and blobs have no extension; they are digest paths.

`.pth` was rejected. it is in heavy use by pytorch model files and by python `site-packages` path configuration, which would collide in editors, in github search, and inside repositories that mix pith and python. `.pi` is short, unregistered in github linguist, and clear of major collisions.

## store paths

this section is the intended layout, not the built one. nothing below exists yet, and the distinction matters because this document is `active` — the names are fixed, the paths are a plan.

- `/pith/store/<digest>-<name>` is a content-addressed store path, the same shape as `/nix/store` in nix.
- `/pith/blob/<digest>` is a raw immutable blob.
- `/pith/tree/<digest>` is a serialized tree value.
- `/etc/pithos` holds system configuration for pithos.
- `/pith/var/cache` holds the local cache.

what `pith-store` builds today is a caller-supplied root holding `blobs/<digest>` and `trees/<digest>`, with no absolute prefix, no `<digest>-<name>` shape, and no system location — a build fixture points it at a temporary directory. an absolute root is a system-installation concern that arrives with the first installable artifact, and the `<digest>-<name>` shape is a legibility choice the digest-only path does not need yet. storage is otherwise described in [identity and storage](../design/identity-and-storage.md), which specifies no layout, so this list is the only written one.

materialization is separate from identity. a graph can refer to remote content without copying it to the local filesystem.

## ecosystem

the shape mirrors the nix ecosystem.

- **Pith** is the kernel.
- **PithOS** is the operating system built on it.
- **pithpkgs** is the package set.
- **pith** is the CLI and the language.

### first-party libraries

first-party libraries take names from the parts of a plant stem. each name lines up with a structural role in the kernel.

- `pith` is the kernel, the central core.
- `cambium` is the rule and graph engine, the growth layer that produces the rest.
- `xylem` is build and dependency transport, the tissue that carries water and nutrients upward.
- `phloem` is packaging, the tissue that carries the products of photosynthesis outward.
- `bark` is policy, the outer protective layer.
- `periderm` is secrets and the capability boundary, the selective outer barrier.
- `lenticel` is the adapter ports, the pores that let things cross the bark.

the intent is that the names are self-explaining once the mapping is known.

the scheme holds for the domain libraries and not yet for the kernel. `xylem` and `phloem` exist under these names and in these roles. the kernel ships as ten `pith-*` crates rather than one `pith`, split along the boundaries the implementation found — `pith-core` for the typed ir, `pith-engine` and `pith-arena` for the rule and graph engine that `cambium` names, then `pith-ids`, `pith-store`, `pith-state-sqlite`, `pith-diag`, `pith-output`, `pith-executor-local`, and `pith-cli`. `bark`, `periderm`, and `lenticel` name roles no crate fills.

whether the kernel's split is permanent or collapses toward `pith` and `cambium` is a question the release record owns, since crate boundaries become a published surface at that point and not before. until then the `pith-*` prefix says what a crate is part of, which is the property a twelve-crate workspace needs and a seven-name scheme does not supply.

### community packages

community packages attach `pith` as a suffix, following the nix convention of attaching `nix`.

- `sops-pith` manages secrets through sops.
- `home-pith` manages home environments.
- `disk-pith` handles disk partitioning.
- `pith-darwin` is the macos target.

the general form for a third-party package is `<topic>-pith`.

## forge

the canonical source and the collaboration point is github.

- the organization is `github.com/pith-language`.
- issues, pull requests, and releases live there.
- continuous integration runs on github actions. public-repo minutes are free without limit.
- a read-only copy is mirrored at `codeberg.org/pith`, with issues and pull requests disabled.

the choice is pragmatic. a new ecosystem needs contributor reach and free CI, and the alternatives impose either operational cost, in the self-hosted case, or limited capacity and reach, in the codeberg case.

the CI workflow files are written to be valid under forgejo actions as well as github actions, so moving the primary to a self-hosted forgejo later is a configuration change rather than a migration. published artifacts are addressed through the project's own hostnames, not github-locked URLs, so a move does not break downstream references.

## domains

one root domain is used, with subdomains for each surface.

- `pith-lang.org` is the canonical home.
- `pith-lang.com` redirects to it.
- `docs.pith-lang.org` holds the documentation.
- `pkgs.pith-lang.org` holds the package registry.
- `git.pith-lang.org` is reserved for a self-hosted forge, if the primary ever moves.

`.org` is the primary because it is the standard top-level domain for community and open-source projects. the `-lang` suffix follows the common convention for programming-language homes and keeps the project distinct from unrelated products that use the bare word "pith".

### note on pith.org

`pith.org` is held by an individual and is not part of the plan. a single inquiry to its owner is acceptable as a low-cost probe. if it were acquired at a reasonable price, the canonical home would move to `pith.org` and `pith-lang.org` would redirect to it. the plan does not depend on this.

## namespaces

- the reserved crate names are `pith`, `cambium`, `xylem`, `phloem`, `bark`, `periderm`, and `lenticel`. the workspace today publishes none of them: `xylem` and `phloem` exist, the kernel is ten `pith-*` crates, and `publish = false` holds across all of them under 0048.
- the github organization is `pith-language` and the codeberg organization is `pith`.
- the package registry namespace is `pithpkgs`.
