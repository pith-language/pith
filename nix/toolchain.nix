# One pkgs, toolchain, and craneLib per system, with the
# rust-toolchain.toml pin as the single source of truth for devshells
# and checks alike.
{inputs, ...}: {
  perSystem = {system, ...}: let
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
  in {
    _module.args = {
      inherit pkgs pkgsUnstable craneLib rustToolchain nightlyToolchain;
    };
  };
}
