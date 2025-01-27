{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    { flake-utils, nixpkgs, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        mkShell = pkgs.mkShell.override { stdenv = pkgs.useMoldLinker pkgs.stdenv; };
      in
      {
        devShells.default = mkShell {
          packages = with pkgs; [
            pre-commit

            typos
            committed

            nixfmt-rfc-style
            statix

            nodePackages.prettier
            dprint
          ];
        };
      }
    );
}
