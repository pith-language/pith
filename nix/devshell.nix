{
  perSystem = {
    lib,
    config,
    pkgs,
    rustToolchain,
    nightlyToolchain,
    ...
  }: let
    # Every CI shell drives its task through the justfile.
    commonCiPackages = [pkgs.just];

    ciRustPackages =
      commonCiPackages
      ++ [
        rustToolchain
        pkgs.git
        pkgs.clang
      ];
    ciFormatPackages = commonCiPackages ++ [config.treefmt.build.wrapper];
    # cargo-deny shells out to `cargo metadata`, so it needs the cargo from the
    # pinned toolchain even though it never compiles anything.
    ciDenyPackages =
      commonCiPackages
      ++ [
        rustToolchain
        pkgs.cargo-deny
      ];
  in {
    devShells = {
      # The combined shell is convenient locally. CI uses the task-specific
      # shells below so each isolated runner realizes only what its job needs.
      ci = pkgs.mkShell {
        packages = lib.unique (
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
          pkgs.cargo-outdated
          pkgs.cargo-edit
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

          # source
          pkgs.forgejo-cli
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
