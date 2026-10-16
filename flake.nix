{
  inputs = {
    nix-daml-sdk = {
      url = "github:obsidiansystems/nix-daml-sdk";
      flake = false;
    };
  };

  outputs =
    { nix-daml-sdk, ... }:
    let
      system = "x86_64-linux";
      daml = import nix-daml-sdk {
        inherit system;
        sdkVersion = "3.4.11";
      };
      inherit (daml) pkgs;
      sdkRoot = "${daml.sdk}/sdk/${daml.sdkVersion}";
    in
    {
      packages.${system}.basic = pkgs.stdenvNoCC.mkDerivation {
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
    };
}
