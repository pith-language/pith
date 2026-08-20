{inputs, ...}: {
  perSystem = {
    system,
    config,
    ...
  }: let
    pkgs = import inputs.nixpkgs {
      inherit system;
      overlays = [inputs.rust-overlay.overlays.default];
    };

    pkgsUnstable = import inputs.nixpkgs-unstable {inherit system;};

    rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./../rust-toolchain.toml;

    # A nightly toolchain for investigative tools (miri, cargo-fuzz) that the
    # stable toolchain cannot run. `selectLatestNightlyWith` skips a dated
    # nightly whose components are unavailable instead of failing the whole
    # build, which is the rust-overlay-recommended way to consume nightly.
    nightlyToolchain = pkgs.rust-bin.selectLatestNightlyWith (
      toolchain:
        toolchain.minimal.override {
          extensions = ["rust-src" "miri" "clippy"];
        }
    );

    craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

    ciRustPackages = [
      rustToolchain
      pkgs.just
      pkgs.git
      pkgs.clang
    ];
    ciFormatPackages = [
      config.treefmt.build.wrapper
      pkgs.just
    ];
    # cargo-deny shells out to `cargo metadata`, so it needs the cargo from the
    # pinned toolchain even though it never compiles anything.
    ciDenyPackages = [
      rustToolchain
      pkgs.cargo-deny
      pkgs.just
    ];
  in {
    _module.args = {
      inherit pkgs pkgsUnstable craneLib rustToolchain nightlyToolchain;
    };

    devShells = {
      # The combined shell is convenient locally. CI uses the task-specific
      # shells below so each isolated runner realizes only what its job needs.
      ci = pkgs.mkShell {
        packages = pkgs.lib.unique (
          ciRustPackages ++ ciFormatPackages ++ ciDenyPackages
        );
      };

      ci-rust = pkgs.mkShell {packages = ciRustPackages;};
      ci-format = pkgs.mkShell {packages = ciFormatPackages;};
      ci-deny = pkgs.mkShell {packages = ciDenyPackages;};

      default = pkgs.mkShell {
        packages = [
          rustToolchain
          nightlyToolchain
          config.treefmt.build.wrapper
          pkgs.cargo-nextest
          pkgs.cargo-llvm-cov
          pkgs.cargo-mutants
          pkgs.cargo-hack
          pkgs.cargo-machete
          pkgs.cargo-deny
          pkgs.cargo-fuzz
          pkgs.cargo-modules
          pkgs.tokei
          pkgs.typos
          pkgs.lychee
          pkgs.bacon
          pkgs.just
          pkgs.deadnix
          pkgs.statix
          pkgs.git
          pkgs.clang
          pkgs.jujutsu

          # web
          pkgs.nodejs
          pkgs.pnpm
          pkgs.wrangler
        ];

        shellHook = ''
          # Expose the nightly cargo so justfile recipes can invoke it without
          # depending on rustup's `+nightly` proxy. Falls back to `cargo
          # +nightly` outside the devshell (see the justfile `env(...)` defaults).
          export CARGO_NIGHTLY="${nightlyToolchain}/bin/cargo"
          if [ -n "$PS1" ]; then
            echo "pith devshell"
            echo "  rust   $(rustc --version)"
            echo "  nightly $("${nightlyToolchain}/bin/rustc" --version)"
            echo "  node   $(node --version)"
            echo "  pnpm   $(pnpm --version)"
          fi
        '';
      };
    };
  };
}
