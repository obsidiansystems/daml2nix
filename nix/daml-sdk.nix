{ runCommand, fetchOciComponent }:
let
  components = "europe-docker.pkg.dev/da-images/public/components";
in
{
  version = "3.6.3";

  # The wrapper scripts start with #!/usr/bin/env sh, which the sandbox lacks.
  damlc =
    runCommand "damlc-3.6.2"
      {
        src = fetchOciComponent {
          repository = "${components}/damlc";
          digest = "sha256:87bfc536992de2a79a229cbf432e0de688964f4dfe3170e406b5cd6f39edcedb";
          hash = "sha256-kKtq4VYA5ufxnBuh/+d7fL9j0yMVRhJ+o1xolOIg5b0=";
        };
      }
      ''
        cp -r --no-preserve=ownership "$src" "$out"
        chmod -R u+w "$out"
        patchShebangs "$out"
      '';

  damlScript = fetchOciComponent {
    repository = "${components}/daml-script";
    digest = "sha256:51feaf4e1f73e827978ec7702430f647570ed18163f4b7a3a074511ac641c0e0";
    hash = "sha256-bTltqHfvLru4sDYlCRmj2B8YPpJTlDd7Fk8P3QojPQA=";
  };
}
