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

a second disagreement runs alongside it, about where the environment stops: at the packages on `PATH`, or at processes, readiness, and ports. the field splits into shells (Nix, direnv, rustup, mise), which stop at `PATH`, and lifecycles (devenv, Flox services, devbox services), which supervise.

## Nix: the environment as a recorded build environment

`mkShell`, read from its source in nixpkgs (`pkgs/build-support/mkshell/default.nix`), is "a special kind of derivation that is only meant to be consumed by the nix-shell". its `phases` default to `["buildPhase"]`, and the single phase appends to `$out` a boxed `WARNING: the existence of this path is not guaranteed`, `It is an internal implementation detail for pkgs.mkShell`, followed by a bare `export` — "Record all build inputs as runtime dependencies", which is the whole point of the output: it exists to hold a reference set, not to be built. `packages` and `inputsFrom` are folded into the input lists (`inputsFrom` propagates the *dependencies* of the given derivations, never the derivations themselves, and concatenates their `shellHook`s in reverse order before the shell's own), `preferLocalBuild` defaults true, and `allowSubstitutes` defaults false. a dev shell is a derivation that exists to have an environment, not an output.

`nix develop` (Nix 2.34 manual) "starts a bash shell that provides an interactive build environment nearly identical to what Nix would use to build" the installable, and obtains that environment by building a modified derivation that "just records the environment initialised by stdenv and exits". the shell that results is interactive in the strong sense: phase functions (`configurePhase`, `buildPhase`, ...) are shell functions you can call, `--redirect` can map a store path like `glibc.dev` to a writable directory, and `--profile` records the environment for reuse.

`nix print-dev-env` is the same fact one step earlier, and the field's cleanest effect boundary. default mode "print[s] shell code that can be sourced by bash to reproduce the build environment of the given installable"; `--json` prints the environment as data — `variables` with types like `exported` and `array`, `bashFunctions` with function bodies — "suitable for consumption by another program", which is what the `nix develop` shell itself sources. evaluating the environment and applying it to a shell are separate operations, and the second is unapologetically an effect in the user's shell: environment variables, shell functions, a hook that runs arbitrary code.

what the environment is *not* is locked by its own act: the lock a flake carries (`flake.lock`) locks the *inputs* to evaluation, and the dev shell's contents are whatever the evaluated expression produced. the reproducibility claim is Nix's usual one — same closure inputs, same store paths — and the interactive shell inherits ambient authority the build sandbox would have removed.

## direnv and nix-direnv: caching the evaluation, protecting it from gc

the direnv wiki's Nix page records the economics: plain `use nix` is "inconveniently slow" on directory entry, so every integration caches the computed environment, "generally in a project's .direnv folder". the hand-rolled flakes idiom is three lines — `watch_file flake.nix`, `watch_file flake.lock`, `eval "$(nix print-dev-env)"` — the print-dev-env split as an automatic hook, at the cost of no garbage-collection protection; the wiki notes flakes "implicitly cache the evaluation", which is why the caching question matters less there.

nix-direnv, read from its README, is the maintained implementation and the field's most explicit statement of what an environment's *cache* is: "significantly faster after the first run by caching the nix-shell environment" and "prevents garbage collection of build dependencies by symlinking the resulting shell derivation in the user's gcroots". two further mechanisms matter for the questions below. "under the covers, `use_flake` calls `nix print-dev-env`" — the ecosystem's activation story routes through the evaluate-to-data command. and when a new environment fails to evaluate, nix-direnv by default reloads the previously working one and sets `NIX_DIRENV_DID_FALLBACK`, a deliberate stale-but-working over fresh-and-broken policy (disable with `nix_direnv_disallow_fallback`), with a manual-reload mode for people who want the staleness named rather than applied. tracked files are explicit: `.envrc`, the direnvrc, the nix file or `flake.nix`/`flake.lock` (plus `devshell.toml` if present) — the invalidation set is a list, not a derivation.

## devenv: the environment as a supervised lifecycle

devenv composes over nix a whole project runtime, and its processes documentation shows the claim is literal, not marketing: "devenv provides built-in process management with supervision, socket activation, file watching, and dependency management". processes are declared with `exec` plus per-process supervision: restart policies (`on_failure` by default, `always`, `never`, with a `max`), and readiness probes of three kinds — an `exec` command whose exit 0 means ready, an `http` poll, and the systemd-style `notify` protocol ("your process should send `READY=1` to the socket path in `$NOTIFY_SOCKET`") — with a systemd-compatible `watchdog` beside them (`WATCHDOG=1` within a `usec` budget or the process is killed and restarted). socket activation passes file descriptors the way systemd does: the process receives `LISTEN_FDS`, `LISTEN_PID`, `LISTEN_FDNAMES`, "file descriptors start at 3". dependencies carry suffixes controlling when a dependency counts as satisfied — `@started`, `@ready` (the default: the probe passed), `@completed` (a soft dependency that does not propagate failure) — and `watch` blocks restart or re-run a process when watched paths change. `devenv processes wait --timeout` blocks until readiness for ci, ports are allocated automatically with a `strict_ports` mode that fails rather than finds the next free port, and the supervisor is swappable (native, process compose, hivemind, honcho, mprocs, overmind) with the declaration above it unchanged.

around the process layer: `devenv.nix` module options and `devenv.yaml` inputs pinned in `devenv.lock`, `devenv up`/`down`, `devenv tasks`, `devenv test` for ci, `devenv container build`, direnv auto-activation, and `.env` integration. its position: an environment is a *lifecycle* — the selection is one input among several, and the lock it carries locks its nix inputs, not the processes.

## devbox: portable tool availability

devbox declares packages in `devbox.json` and builds isolated shells from them, "similar to a package manager like yarn – except the packages it manages are at the operating-system level". the docs position it against "pure Docker containers, Nix Shells, or managing your own environment directly": no virtualization layer, per-project isolation, conflicting versions of one binary in different projects, and one definition rendered four ways — a local shell (`devbox shell`), a devcontainer (`devbox generate devcontainer`), a Dockerfile that "replicates devbox shell" (`devbox generate dockerfile`), and a cloud environment.

the mechanism details that matter below: `devbox shell --print-env` "print[s] a script to setup a devbox shell environment" — print-dev-env's position again — and `--pure` creates "an isolated shell inheriting almost no variables from the current environment", retaining `HOME`, `USER`, `DISPLAY`. the config is found by recursive search up the directory tree, the cache story is nix substituters (`devbox cache configure` points nix at the jetify cache), and packages can come from flakes. its position: an environment is a *portable packaging of tool availability*; the isolation is per-project on one machine, not a sandbox.

## Flox: a constraint-set manifest and a two-tier activation

Flox's `manifest.toml`, read from its man page, is the most lock-like manifest in the field short of Spack's. `[install]` entries are *descriptors*: catalog descriptors carry `pkg-path`, `version` ("either an exact version or a semver range", unspecified fields wildcards), `pkg-group`, `priority`, `outputs`, and `systems`; flake descriptors and store-path descriptors exist beside them. `pkg-group` is a co-resolution unit — "a collection of software that is known to work together at a point in time", "upgraded as a unit" — and `priority` resolves file conflicts when the environment merges every package's `/bin`, `/man`, `/include` (lower wins, default 5). so the manifest is a constraint set, not a selection, and the resolution it drives is recorded in the environment's own lock.

activation is two-tier, and the split is the interesting part. `[hook] on-activate` runs in a predictable bash subshell — it may spawn processes, `eval "$(ssh-agent)"`, create a venv — and "is not re-run when multiple activations are run at the same time": the first activation runs it, concurrent ones inherit the variables it set, and idempotence is recommended because other flox commands may create ephemeral activations. `[profile]` scripts are sourced *by the user's own shell* — `common` plus exactly one of `bash`/`fish`/`tcsh`/`zsh` — for aliases and prompts. activation itself has four modes with a documented matrix: subshell (`flox activate`), in-place (`eval "$(flox activate)"` — the print-dev-env position), shell command (`-c`), and exec (`--`, which skips the profile scripts). what activation does is set "a collection of environment variables" — `PATH` prepended with the environment's merged `bin`, plus `ACLOCAL_PATH`, `RUST_SRC_PATH`, and others. environments are published through FloxHub, layered, composed, run as systemd units, or `containerize`d as "fully pinned" OCI images; `schema-version` forward-migrates old manifests automatically. its position: an environment is a *shareable artifact* with its own manifest and lock, and nix is the substrate under an interface that hides it.

## Guix: manifests, cached environments, and the container as the honest isolation

`guix shell` (guix manual) builds one-off environments without touching the profile: `GUIX_ENVIRONMENT` names the profile, `-D` takes a package's dependencies rather than the package, `--root` symlinks the profile as a gc root so the environment survives collection, and an argument-less invocation in a directory with `manifest.scm` uses it — but only if the directory appears in `~/.config/guix/shell-authorized-directories`, an explicit trust decision about which trees may define an environment. the environment is cached — "guix shell caches the environment so that subsequent uses are instantaneous", lru-evicted, `--rebuild-cache` to force — and the cache invalidates when the manifest file changes.

`--container` is the strongest isolation any surveyed system applies to a *development* environment, in the manual's own words: it "goes one step further by spawning a container isolated from the rest of the system", where "the container lacks network access and shares no files other than the current working directory with the surrounding environment", useful "to prevent access to system-wide resources such as /usr/bin on foreign distros". a dummy home directory and matching `/etc/passwd` entry are created, `--expose`/`--share` are the explicit exceptions, `--network` shares the host's network namespace, and `--emulate-fhs` fabricates `/bin`, `/lib`, `/usr`. guix thereby admits what the others leave implicit — that an augmented shell is not isolated at all, and making it so is a separate, opt-in mechanism with its own costs.

## Spack: the environment with its own lock

Spack environments are the closest precedent for an environment that owns a lock, and the docs frame them exactly so: "a manifest and lock" model "similar to Bundler gemfiles". `spack.yaml` is the manifest — abstract root specs, `concretizer: unify:` policy, `packages:` pinning, view configuration; `spack.lock` "contains the fully configured and concretized specs". `spack add` edits the manifest without touching the lock; `spack concretize` resolves into it; and without `-f` "Spack guarantees that already concretized specs are unchanged", so registry motion does not move an environment until reconcretization is asked for. the guarantee is scoped honestly: an environment created from a `spack.lock`, "when on the same or a compatible machine, is guaranteed to initially have the same concrete specs as the original", while creation from `spack.yaml` may concretize differently under different configuration or Spack version. activation (`spack env activate`) makes commands environment-aware and prepends a *view* — a link tree of the installed packages under `.spack-env/view` — to `PATH` and `CMAKE_PREFIX_PATH`. the manifest/lock split inside one environment, with the lock's guarantee scoped exactly to "same or compatible machine", is the honest wording of what any environment-level lock can promise.

## conda: the manifest that stayed a manifest

conda's `environment.yml` declares name, channels, dependencies with wildcards (`numpy=1.21.*`), a nested pip list, and a `variables` section (`VAR1: valueA`) whose values are "copied verbatim into the activation script" and retained in export output. it is a constraint set, not a lock: the solver runs at creation, and what activation does is the field's minimal mechanism, in conda's own words — "adding entries to PATH for the environment and running any activation scripts that the environment may contain", where activation scripts "are how packages can set arbitrary environment variables". reproduction was for years a separate act and a weak one: `conda list --explicit` produces url lists that "are not usually cross platform", and `--from-history` exports what was asked rather than what was resolved. exact recreation now goes through lockfiles — `conda export --file conda-lock.yaml`, with repeated `--platform` flags for a single multi-platform file, on the conda-lock format — that record the resolved packages so the solver can be skipped. conda's history is the cautionary line: years of environments whose resolved content lived only on the machine that resolved them.

## rustup, asdf, mise: the toolchain pin

rustup's override precedence, verbatim from its documentation: "a toolchain override shorthand used on the command-line, such as `cargo +beta`; the `RUSTUP_TOOLCHAIN` environment variable; a directory override, set with the `rustup override` command; the `rust-toolchain.toml` file; the default toolchain" — first match wins, with one exception: "directory overrides and the rust-toolchain.toml file are also preferred by their proximity to the current directory". `rust-toolchain.toml` pins a channel (`stable`, a dated nightly, a version), additive `components` and `targets`, and a `profile`; it pins one dimension and resolves nothing else, trusting the channel infrastructure to make a channel name stable.

asdf's `.tool-versions` (one `plugin version` per line, fuzzy versions, `latest`, `ref:`/`prefix:`/`path:` scopes) and mise's `mise.toml` (`[tools]`, `[env]`, `[tasks]`, additive merges up the directory tree, tool installs under `MISE_DATA_DIR`, default `~/.local/share/mise`, where mise "stores plugins and tool installs" and which "are not supposed to be shared") generalize the pin to a set of tools. none of them resolves *packages*: the tool versions are the whole environment, and anything installed inside it is out of the file's sight.

## devcontainers: image, features, and a lifecycle protocol

the development container specification standardizes metadata rather than orchestration: `devcontainer.json` over `image` or `build.dockerfile`/`build.context`, plus `features` (referenced by id with an options object, auto-ordered by each feature's `installsAfter`), `mounts` taking "the same values as the Docker CLI --mount flag", and `workspaceFolder`. two field splits are worth keeping. `containerEnv` sets variables on the container itself, static for its lifetime, while `remoteEnv` sets variables for the connecting tool and its sub-processes and can change without a rebuild — environment variables for the machine versus for the session, a distinction every shell-based system collapses. and the lifecycle is a fixed protocol: `initializeCommand` on the host, then `onCreateCommand`, `updateContentCommand`, `postCreateCommand` inside the container, then `postStartCommand` on every start and `postAttachCommand` on every attach, with `waitFor` (default `updateContentCommand`) controlling what tools block on. the environment's identity is an image digest plus feature scripts; there is no package resolution inside the spec at all. its distinctive claim is portability across tools by being editor-agnostic metadata — the same move lsp made for language analysis.

## PEP 405: the minimal end of the range

python's venv, from the PEP: a `pyvenv.cfg` "found either adjacent to the Python executable or one directory above it (if the executable is a symlink, it is not dereferenced)" is scanned for `key = value` lines, and "if a `home` key is found, this signifies that the Python binary belongs to a virtual environment", the value pointing at the base installation. `sys.prefix` moves to the venv while `sys.base_prefix` keeps the base; the standard library, headers, and interpreter stay shared; system site-packages are excluded unless `include-system-site-packages = true`; and os-level packages stay wholly outside the mechanism. the venv is state, not declaration: nothing about how it came to be is recorded in it, and its reproducibility is whatever the commands that populated it happened to be.

## what the disagreement is

on the three-way question the systems sort as follows. **lock consumers**: mkShell and `nix develop` (the lock locks evaluation inputs, the environment is computed from them), mise and asdf (a pin file consumed per tool), devcontainers (an image plus metadata, no resolution). **environments with their own lock**: Spack (`spack.yaml`/`spack.lock`, the guarantee scoped to compatible machines), Flox (a constraint-set manifest whose resolution lands in the environment's own lock), devenv (a lock over its nix inputs), devbox (a manifest plus its lock, over nix under the hood). **manifests only, resolution unrecorded or recorded separately**: conda's `environment.yml`, historically the whole story, with the lockfile arriving late and separately. **state, not declaration**: PEP 405's venv, where the environment is the materialized directory itself.

on isolation they sort into three positions: no isolation claim at all (mkShell, direnv, rustup, mise, venv — the augmented shell), caching plus gc roots as the only protection (nix-direnv, `guix shell --root`), and a real container boundary as an explicit option (guix `--container`, devcontainers by construction). no surveyed system confines the interactive environment by default, and the ones that confine it make that a separate mechanism from defining it. devbox's `--pure` is the honest middle: it clears inherited variables but retains `HOME`, `USER`, `DISPLAY` — purity of spelling, not a sandbox.

on the effect boundary, the evaluate-to-data position is stronger in the field than it first looks: `nix print-dev-env` states it, nix-direnv's `use flake` is implemented on top of it, devbox ships `--print-env`, and Flox's in-place mode is `eval "$(flox activate)"`. the split between computing an environment and applying it to a shell is everywhere once looked for; what differs is whether the computation is keyed and recorded (nix's eval cache, direnv's cache file, guix's lru cache) or re-run, and whether the applied artifact is code (shell text) or data (flox's variables, print-dev-env's `--json`).

on where the environment stops, the split is `PATH` versus supervision: nix's shell stops at environment variables and a hook, while devenv's processes declare readiness, restart policy, and socket activation, and Flox's `[services]` carry `auto-start`. the shell-shaped systems treat processes as something the person runs; the lifecycle-shaped ones treat them as environment content with its own state machine.

## questions for this project

- is a pith environment a value derived from a resolution (Nix's position, composed over phloem's lock) or an artifact with its own lock (Spack's and Flox's position)? 0041's lock already binds selections; forking a second lock shape for environments has to argue against reusing it.
- which of 0014's three properties does "reproducible development environment" assert — and is the answer "the selection and the rendering are deterministic functions of recorded inputs", with bit-for-bit reproducibility of the packages themselves left to the build instructions and the rebuild check?
- where does entering an environment sit relative to 0003's boundary, and is print-dev-env's evaluate-to-data split the right shape for the prototype, given no shell integration ships in M-4?
- where do 0042's substitution records persist, given the lock refuses binaries by construction?
- what does a pith environment leave in the two stores 0027 governs, and is anything a pin rather than ordinary collectable content?
- if processes ever become environment content, devenv's supervision and readiness model is the measured precedent — but nothing in M-4's scope measures the need, and the record should say so rather than design ahead of it.

## sources

- [mkShell source, nixpkgs](https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/mkshell/default.nix)
- [nix develop, Nix manual](https://nix.dev/manual/nix/stable/command-ref/new-cli/nix3-develop.html)
- [nix print-dev-env, Nix manual](https://nix.dev/manual/nix/stable/command-ref/new-cli/nix3-print-dev-env)
- [direnv wiki, Nix](https://github.com/direnv/direnv/wiki/Nix)
- [nix-direnv README](https://github.com/nix-community/nix-direnv/blob/master/README.md)
- [devenv processes](https://devenv.sh/processes/)
- [devbox documentation](https://www.jetify.com/docs/devbox/index.md)
- [devbox shell CLI reference](https://www.jetify.com/docs/devbox/cli-reference/devbox-shell/index.md)
- [Flox manifest.toml man page](https://flox.dev/docs/man/manifest.toml.md)
- [Flox activating environments](https://flox.dev/docs/concepts/activation.md)
- [guix shell, Guix manual](https://guix.gnu.org/manual/en/html_node/Invoking-guix-shell.html)
- [Spack environments](https://spack.readthedocs.io/en/latest/environments.html)
- [conda managing environments](https://docs.conda.io/projects/conda/en/latest/user-guide/tasks/manage-environments.html)
- [rustup overrides](https://rust-lang.github.io/rustup/overrides.html)
- [mise configuration](https://mise.jdx.dev/configuration.html)
- [development containers: overview](https://containers.dev/overview) and [json reference](https://containers.dev/implementors/json_reference/)
- [PEP 405](https://peps.python.org/pep-0405/)
