{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      fenix,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      fetchOciDar = pkgs.callPackage ./nix/fetch-oci-dar.nix { };
      fetchOciComponent = pkgs.callPackage ./nix/fetch-oci-component.nix { };
      buildDamlPackage = pkgs.callPackage ./nix/build-daml-package.nix {
        sdk = pkgs.callPackage ./nix/daml-sdk.nix { inherit fetchOciComponent; };
      };
    in
    {
      lib.${system} = {
        inherit fetchOciDar fetchOciComponent buildDamlPackage;
      };

      packages.${system} =
        let
          fixture =
            path:
            let
              generated = import path;
            in
            generated (
              builtins.intersectAttrs (builtins.functionArgs generated) {
                inherit buildDamlPackage fetchOciDar;
              }
            );
        in
        fixture ./fixtures/basic // fixture ./fixtures/multi // fixture ./fixtures/oci;

      devShells.${system}.default = pkgs.mkShell {
        strictDeps = true;
        packages = [
          pkgs.rustc
          pkgs.cargo
          fenix.packages.${system}.latest.rustfmt
          pkgs.rust-analyzer
          pkgs.clippy
          pkgs.nixfmt
          pkgs.oras
          pkgs.tombi
        ];
      };
    };
}
