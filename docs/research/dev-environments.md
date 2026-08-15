---
schema: design-doc/v1
id: research-dev-environments
title: development environments
summary: how Nix's shells, Guix, Spack, conda, rustup, asdf and mise, Flox, devenv, devbox, direnv, devcontainers, and Python's venv each answer what a development environment is, and whether it is a lock consumer, a lock producer, or a lock of its own
kind: research
status: researching
evidence: preliminary
created: 2026-07-06
updated: 2026-07-27
tags:
  - research
  - packages
  - environments
relations:
  informed_by: []
  depends_on:
    - research-method
    - research-nix
    - research-dependency-resolution
    - research-binary-reuse
  supersedes: []
---

# development environments

the systems below disagree about one question more than any other: is a development environment a *derivation of a resolution* — one more computed thing over pinned inputs — or a *materialized directory* whose identity is the state of the machine it sits on? the disagreement decides everything downstream: whether the environment has a lock, whether entering it is an effect, and what "reproducible" can honestly mean about it.

## Nix: the environment as a recorded build environment

`mkShell`, read from its source in nixpkgs (`pkgs/build-support/mkshell/default.nix`), is a `stdenv.mkDerivation` wrapper "only meant to be consumed by the nix-shell": its `phases` default to `["buildPhase"]`, the single phase writes an `$out` carrying a warning that "the existence of this path is not guaranteed", `packages` and `inputsFrom` are folded into the input lists without reaching the derivation arguments, and `allowSubstitutes = false`. a dev shell is therefore a derivation that exists to have an environment, not an output.

`nix develop` (Nix 2.34 manual) "starts a bash shell that provides an interactive build environment nearly identical to what Nix would use to build" the installable, and obtains it by building a modified derivation that "just records the environment initialised by stdenv and exits". `nix print-dev-env` is the same fact one step earlier: it prints shell code that can be *sourced* to reproduce the build environment, or `--json` giving the variables and bash functions as data "for consumption by another program". the split is the load-bearing idea: evaluating the environment and applying it to a shell are separate operations, and the second is unapologetically an effect in the user's shell — environment variables, shell functions, a hook that runs arbitrary code.

what the environment is *not* is locked by its own act: the lock a flake carries (`flake.lock`) locks the *inputs* to evaluation, and the dev shell's contents are whatever the evaluated expression produced. the reproducibility claim is Nix's usual one — same closure inputs, same store paths — and the interactive shell inherits ambient authority the build sandbox would have removed.

## direnv and nix-direnv: caching the evaluation, protecting it from gc

the direnv wiki's Nix page records the economics: plain `use nix` is "inconveniently slow" on directory entry, so every integration caches the computed environment, "generally in a project's .direnv folder". the hand-rolled flakes idiom is three lines — `watch_file flake.nix`, `watch_file flake.lock`, `eval "$(nix print-dev-env)"` — which gets the print-dev-env split into an automatic hook at the cost of no garbage-collection protection; nix-direnv adds the gc roots and keeps older caches so reverting a change after a collection does not force rebuilds. invalidation is by watched-file modification time or content hash depending on the integration, and the wiki notes flakes intrinsically cache evaluation, which is why the caching question matters less there. the cache is invisible state keyed by files, the same shape pith's invalidation explanations exist to avoid hiding.

## devenv, devbox, Flox: three positions on what a dev environment adds

devenv composes over nix a whole project runtime: `devenv.nix` module options plus `devenv.yaml` inputs pinned in `devenv.lock`, processes and services started by `devenv up` across a choice of process managers, tasks, `devenv test` for ci, containers built from the environment, and `.env`/direnv integrations. its position: an environment is a *lifecycle*, not a shell — the selection is one input among several, and the lock it carries locks its nix inputs.

devbox declares packages in `devbox.json` and builds isolated shells from them, positioning itself against "pure Docker containers, Nix Shells, or managing your own environment directly": no virtualization layer, per-project isolation, and one definition portable to a local shell, a devcontainer, a Dockerfile, or a cloud environment. its position: an environment is a *portable packaging of tool availability*; the isolation is per-project on one machine, not a sandbox.

Flox declares packages, variables, activation scripts, and services in `manifest.toml`, activates them as "pre-configured sub-shells, not containers", publishes environments through FloxHub, and can `containerize` them as pinned OCI images; it uses nix "to ensure reproducibility — without requiring you to learn Nix". its position: an environment is a *shareable artifact* with its own manifest and lock, closer to Spack's shape than to mkShell's.

all three agree the environment should be checked into version control and disagree about whether the environment's own file is a manifest (devbox), a manifest plus lock (Flox, devenv), or a module configuration evaluated against someone else's lock (devenv's inputs).

## Guix: manifests, cached environments, and the container as the honest isolation

`guix shell` (guix manual) builds one-off environments without touching the profile: `GUIX_ENVIRONMENT` names the profile, `-D` takes a package's dependencies rather than the package, `--root` symlinks the profile as a gc root so the environment survives collection, and in a directory with `manifest.scm` an argument-less invocation uses it — but only if the directory appears in `~/.config/guix/shell-authorized-directories`, an explicit trust decision about which trees may define an environment. the environment is cached ("subsequent uses are instantaneous", lru-evicted) and the cache invalidates when the manifest file changes.

`--container` is the strongest isolation any surveyed system applies to a *development* environment: a container with no network beyond loopback, no filesystem except the working directory, a dummy home and matching `/etc/passwd` entry, `--expose`/`--share` mounts as the explicit exceptions. guix thereby admits what the others leave implicit — that an augmented shell is not isolated at all, and making it so is a separate, opt-in mechanism with its own costs.

## Spack: the environment with its own lock

Spack environments are the closest precedent for an environment that owns a lock. `spack.yaml` is the manifest — abstract root specs, concretizer policy, view configuration; `spack.lock` holds "the fully configured and concretized specs". `spack add` edits the manifest without touching the lock; `spack concretize` resolves into it; `spack install` installs what the lock says; and without `-f` "Spack guarantees that already concretized specs are unchanged", so registry motion does not move an environment until reconcretization is asked for. an environment created from a `spack.lock` "on the same or a compatible machine, is guaranteed to initially have the same concrete specs as the original", while creation from `spack.yaml` may concretize differently under different configuration or Spack version. activation (`spack env activate`) makes commands environment-aware and prepends a *view* — a link tree of the installed packages — to `PATH`.

the manifest/lock split inside one environment, with the lock's guarantee scoped exactly to "same or compatible machine", is the honest wording of what any environment-level lock can promise.

## conda: the manifest that stayed a manifest

conda's `environment.yml` declares name, channels, dependencies with wildcards (`numpy=1.21.*`), saved `variables` (recommended over activate scripts, which "run arbitrary code"), and a nested pip list. it is a constraint set, not a lock: the solver runs at creation, and reproduction is a separate act — `conda list --explicit` produces url lists that "are not usually cross platform", `--from-history` exports what was asked rather than what was resolved, and exact recreation now goes through lockfiles (`conda export --file conda-lock.yaml`, conda 26.5+) that record "the exact packages, versions, builds, and channels" and let conda skip the solver. conda's history is the cautionary line: years of environments whose resolved content lived only on the machine that resolved them.

## rustup, asdf, mise: the toolchain pin

rustup's `rust-toolchain.toml` pins a channel (`stable`, `nightly-2020-07-10`, `1.0.0`), plus additive `components`, `targets`, and a `profile`; override precedence is command-line shorthand, then `RUSTUP_TOOLCHAIN`, then directory overrides and toolchain files ranked by proximity up the tree, then the default. the file pins one dimension — the toolchain — and resolves nothing else: it is a lock over a single input, trusted to rustup's channel infrastructure.

asdf's `.tool-versions` (one `plugin version` per line, fuzzy versions and `latest` allowed) and mise's `mise.toml` (`[tools]`, `[env]`, `[tasks]`, additive merges up the directory tree) generalize the same idea to a set of tools. none of them resolves *packages*: the tool versions are the whole environment, and anything installed inside it (a language package manager's output) is out of the file's sight.

## devcontainers: the environment as an image plus metadata

the development container specification deliberately standardizes metadata rather than orchestration: `devcontainer.json` (jsonc) plus image labels and reusable Features enrich "existing formats" with development settings, spanning single containers to compose-orchestrated setups, and the reference cli plus ci actions reuse the same definition locally and in ci. the environment's identity is an image digest plus feature scripts; there is no package resolution inside the spec at all. its distinctive claim is portability across tools by being editor-agnostic metadata — the same move lsp made for language analysis.

## PEP 405: the minimal end of the range

python's venv is a directory containing a copy or symlink of the interpreter, a `pyvenv.cfg` whose `home` key points at the base installation, and a private `site-packages`. `sys.prefix` moves to the venv while `sys.base_prefix` keeps the base; the standard library, headers, and the interpreter itself stay shared, os-level packages stay wholly outside the mechanism, and system site-packages are excluded unless explicitly included. the venv is state, not declaration: nothing about how it came to be is recorded in it, and its reproducibility is whatever the commands that populated it happened to be.

## what the disagreement is

on the three-way question the systems sort as follows. **lock consumers**: mkShell and `nix develop` (the lock locks evaluation inputs, the environment is computed from them), mise and asdf (a pin file consumed per tool), devcontainers (an image plus metadata, no resolution). **environments with their own lock**: Spack (`spack.yaml`/`spack.lock`, the guarantee scoped to compatible machines), Flox and devenv (manifest plus their own lock over nix inputs), devbox (manifest plus lock). **manifests only, resolution unrecorded or recorded separately**: conda's `environment.yml`, historically the whole story. **state, not declaration**: PEP 405's venv, where the environment is the materialized directory itself.

on isolation they sort into three positions as well: no isolation claim at all (mkShell, direnv, rustup, mise, venv — the augmented shell), caching plus gc roots as the only protection (nix-direnv, `guix shell --root`), and a real container boundary as an explicit option (guix `--container`, devcontainers by construction). no surveyed system makes the interactive environment confined by default, and the ones that confine it make that a separate mechanism from defining it.

on the effect boundary, `nix print-dev-env` is the cleanest statement in the field: evaluate the environment to data (shell code, or json "for consumption by another program"), and let the consumer apply it. entering an environment mutates a caller's shell; the systems that separate computing the environment from entering it can be honest about that, and the systems that blur it cannot.

## questions for this project

- is a pith environment a value derived from a resolution (Nix's position, composed over phloem's lock) or an artifact with its own lock (Spack's position)? 0041's lock already binds selections; forking a second lock shape for environments has to argue against reusing it.
- which of 0014's three properties does "reproducible development environment" assert — and is the answer "the selection and the rendering are deterministic functions of recorded inputs", with bit-for-bit reproducibility of the packages themselves left to the build instructions and the rebuild check?
- where does entering an environment sit relative to 0003's boundary, and is `print-dev-env`'s evaluate-to-data split the right shape for the prototype, given no shell integration ships in M-4?
- where do 0042's substitution records persist, given the lock refuses binaries by construction?
- what does a pith environment leave in the two stores 0027 governs, and is anything a pin rather than ordinary collectable content?

## sources

- [mkShell source, nixpkgs](https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/mkshell/default.nix)
- [nix develop, Nix manual](https://nix.dev/manual/nix/stable/command-ref/new-cli/nix3-develop.html)
- [nix print-dev-env, Nix manual](https://nix.dev/manual/nix/stable/command-ref/new-cli/nix3-print-dev-env)
- [direnv wiki, Nix](https://github.com/direnv/direnv/wiki/Nix)
- [devenv getting started](https://devenv.sh/getting-started/)
- [devbox documentation](https://www.jetify.com/devbox/docs/)
- [Flox documentation](https://flox.dev/docs/)
- [guix shell, Guix manual](https://guix.gnu.org/manual/en/html_node/Invoking-guix-shell.html)
- [Spack environments](https://spack.readthedocs.io/en/latest/environments.html)
- [conda managing environments](https://docs.conda.io/projects/conda/en/latest/user-guide/tasks/manage-environments.html)
- [rustup overrides](https://rust-lang.github.io/rustup/overrides.html)
- [mise configuration](https://mise.jdx.dev/configuration.html)
- [development containers overview](https://containers.dev/overview)
- [PEP 405](https://peps.python.org/pep-0405/)
