{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem.treefmt = {
    projectRootFile = "flake.nix";
    programs = {
      rustfmt.edition = "2024";
      alejandra.enable = true;
      typos.enable = true;
    };
  };
}
