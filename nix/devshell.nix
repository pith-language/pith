{ inputs, ... }:
{
  perSystem = { system, ... }:
    let
      pkgs = import inputs.nixpkgs {
        inherit system;
        overlays = [ inputs.rust-overlay.overlays.default ];
      };

      pkgsUnstable = import inputs.nixpkgs-unstable { inherit system; };

      rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./../rust-toolchain.toml;

      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;
    in
    {
      _module.args = {
        inherit pkgs pkgsUnstable craneLib rustToolchain;
      };

      devShells.default = pkgs.mkShell {
        packages = [
          rustToolchain
          pkgs.cargo-nextest
          pkgs.git
        ];

        shellHook = ''
          if [ -n "$PS1" ]; then
            echo "pith devshell"
            echo "  rust $(rustc --version)"
          fi
        '';
      };
    };
}
