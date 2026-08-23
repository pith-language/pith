{
  self,
  lib,
  ...
}: {
  perSystem = {
    system,
    pkgs,
    craneLib,
    rustToolchain,
    ...
  }: let
    # Shared by every cargo derivation: mismatched RUSTFLAGS or profiles
    # change fingerprints and make each check recompile the workspace.
    commonArgs = {
      pname = "pith";
      version = "0.1.0";
      src = craneLib.cleanCargoSource self.outPath;
      strictDeps = true;
      RUSTFLAGS = "-D warnings";
      # The dev profile keeps the debug assertions and overflow checks
      # the test suite relies on.
      CARGO_PROFILE = "dev";
    };

    # Virtual manifest at the root: no package for cargo to select.
    cargoArtifacts = craneLib.buildDepsOnly (commonArgs
      // {
        cargoBuildCommand = "cargo build --workspace";
      });

    advisory-db-tarball = pkgs.fetchFromGitHub {
      owner = "rustsec";
      repo = "advisory-db";
      rev = "bf5c0d245a92671908518d7e765914d437954ed6";
      hash = "sha256-6JNCbYbrWi2r0+kVgXtVi7Vu6gSuSjmyifCSWyZaS9o=";
    };

    # cargo-deny reads the database's git HEAD for freshness.
    advisory-db =
      pkgs.runCommand "advisory-db" {
        nativeBuildInputs = [pkgs.git];
        env = {
          GIT_AUTHOR_NAME = "nix";
          GIT_AUTHOR_EMAIL = "nix@invalid";
          GIT_COMMITTER_NAME = "nix";
          GIT_COMMITTER_EMAIL = "nix@invalid";
          GIT_AUTHOR_DATE = "2026-08-23T00:00:00+00:00";
          GIT_COMMITTER_DATE = "2026-08-23T00:00:00+00:00";
        };
      } ''
        cp -r --no-preserve=all ${advisory-db-tarball} $out
        cd $out
        git init -q
        git add -A
        git commit -qm "rustsec/advisory-db bf5c0d245a92671908518d7e765914d437954ed6"
      '';

    xtask = craneLib.mkCargoDerivation (commonArgs
      // {
        pname = "pith-xtask";
        cargoArtifacts = null;
        doInstallCargoArtifacts = false;
        buildPhaseCargoCommand = "cargoWithProfile build --locked -p xtask";
        installPhaseCommand = ''
          mkdir -p $out/bin
          cp target/debug/xtask $out/bin/
        '';
      });

    # Runs a command with the flake source snapshot as the working
    # directory.
    sourceCheck = name: packages: command:
      pkgs.runCommand name {nativeBuildInputs = packages;} ''
        cd ${self.outPath}
        ${command}
        touch $out
      '';
  in {
    packages = lib.mkIf (system == "x86_64-linux") {
      # The complement to `tests`: phloem's host-integration suite needs
      # a live nix daemon to answer its closure queries and a kernel
      # without a syscall sandbox over the executor's own seccomp (the
      # hosted NixCI test sandbox kills its actions with SIGSYS). The
      # nix-ci.nix test job stays disabled until such a worker exists;
      # the package is the ready runner for when one does.
      phloem-host-tests = pkgs.writeShellScriptBin "phloem-host-tests" ''
        set -euo pipefail
        # Fully self-contained, and exported before anything else: the
        # test worker starts the program with an empty PATH, and any
        # host compiler leaking in would make discovery fail outside
        # /nix/store.
        export PATH="${lib.makeBinPath [rustToolchain pkgs.stdenv.cc pkgs.clang pkgs.nix pkgs.coreutils]}"
        export CARGO_HOME="${craneLib.vendorCargoDeps {src = craneLib.cleanCargoSource self.outPath;}}"
        # The worker's root filesystem is read-only; put the build
        # somewhere the run is given write access to.
        export CARGO_TARGET_DIR="''${TMPDIR:-$PWD}/pith-target"
        mkdir -p "$CARGO_TARGET_DIR"
        export CARGO_NET_OFFLINE=true
        export RUSTFLAGS="-D warnings"
        cd "${self.outPath}"
        cargo test --locked --workspace
      '';
    };

    checks = lib.mkIf (system == "x86_64-linux") {
      clippy = craneLib.cargoClippy (commonArgs
        // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--workspace --all-targets -- -D warnings";
        });

      # phloem's integration tests exercise the host's C toolchain and
      # its nix closure, and by their own discipline (crates/phloem/
      # tests/base_case.rs) fail rather than skip when no daemon
      # answers — which no build sandbox provides. They run via `just
      # test` and the unsandboxed CI tier; this check runs everything
      # hermetic.
      tests = craneLib.mkCargoDerivation (commonArgs
        // {
          inherit cargoArtifacts;
          buildPhaseCargoCommand = "";
          checkPhaseCargoCommand = ''
            cargoWithProfile test --locked --workspace --exclude phloem
            cargoWithProfile test --locked -p phloem --lib --doc
          '';
        });

      docs = craneLib.cargoDoc (commonArgs
        // {
          inherit cargoArtifacts;
          RUSTDOCFLAGS = "-D warnings";
          cargoExtraArgs = "--locked --workspace";
          cargoDocExtraArgs = "--no-deps --document-private-items";
        });

      deny = craneLib.mkCargoDerivation {
        src = craneLib.cleanCargoSource self.outPath;
        pname = "pith-deny";
        version = "0.1.0";
        cargoArtifacts = null;
        doInstallCargoArtifacts = false;
        # The store path is owned by a different uid than the build's,
        # which git refuses to touch without an exception.
        env = {
          GIT_CONFIG_COUNT = "1";
          GIT_CONFIG_KEY_0 = "safe.directory";
          GIT_CONFIG_VALUE_0 = "${advisory-db}";
        };
        buildPhaseCargoCommand = ''
          # The directory name is the hash cargo-deny derives from the
          # db-urls entry in deny.toml; it must move with that URL.
          mkdir -p .cargo-home/advisory-dbs
          ln -s ${advisory-db} .cargo-home/advisory-dbs/advisory-db-3157b0e258782691
          cargo --offline deny --locked check --disable-fetch advisories bans licenses sources
        '';
        nativeBuildInputs = [pkgs.cargo-deny pkgs.git];
      };

      repo-check =
        sourceCheck "pith-repo-check" [xtask]
        ''PITH_WORKSPACE_ROOT=${self.outPath} xtask check'';

      just-fmt = sourceCheck "pith-just-fmt" [pkgs.just] "just --fmt --check";
    };
  };
}
