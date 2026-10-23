{
  lib,
  stdenvNoCC,
  oras,
  cacert,
}:
{
  repository,
  digest,
  hash,
}:
stdenvNoCC.mkDerivation {
  name = baseNameOf repository;
  nativeBuildInputs = [ oras ];

  outputHashMode = "recursive";
  outputHash = hash;

  impureEnvVars = lib.fetchers.proxyImpureEnvVars;
  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";

  buildCommand = /* sh */ ''
    oras pull --output "$out" ${lib.escapeShellArg "${repository}@${digest}"}
  '';
}
