{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-daml-sdk = {
      url = "github:obsidiansystems/nix-daml-sdk";
      flake = false;
    };
  };

  outputs =
    {
      nixpkgs,
      fenix,
      nix-daml-sdk,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      daml = import nix-daml-sdk {
        inherit system;
        sdkVersion = "3.4.11";
      };
      sdkRoot = "${daml.sdk}/sdk/${daml.sdkVersion}";
    in
    {
      lib.${system}.fetchOciDar = daml.pkgs.callPackage ./nix/fetch-oci-dar.nix { };

      packages.${system}.basic = daml.pkgs.stdenvNoCC.mkDerivation {
        name = "basic";
        src = ./fixtures/basic;
        env.DAML_SDK = sdkRoot;
        env.DAML_SDK_VERSION = daml.sdkVersion;
        buildPhase = ''
          runHook preBuild
          ${sdkRoot}/damlc/damlc build -o basic.dar
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          install -Dm444 basic.dar $out/basic.dar
          runHook postInstall
        '';
      };

      devShells.${system}.default = pkgs.mkShell {
        strictDeps = true;
        packages = [
          pkgs.rustc
          pkgs.cargo
          fenix.packages.${system}.latest.rustfmt
          pkgs.rust-analyzer
          pkgs.clippy
          pkgs.nixfmt
          pkgs.tombi
        ];
      };
    };
}
