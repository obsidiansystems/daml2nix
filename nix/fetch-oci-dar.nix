{
  lib,
  stdenvNoCC,
  oras,
  jq,
  cacert,
}:
{
  uri,
  manifestDigest,
  darDigest,
  insecure ? false,
}:
let
  parsed = builtins.match "oci://(.+/[^:/]+)(:[^/]+)?" uri;
  repository =
    if parsed == null then
      throw "fetchOciDar: '${uri}' is not an oci:// reference"
    else
      lib.head parsed;
  darHash =
    if lib.hasPrefix "sha256:" darDigest then
      lib.removePrefix "sha256:" darDigest
    else
      throw "fetchOciDar: darDigest '${darDigest}' must be a sha256 digest";
  plainHttp = lib.optionalString insecure "--plain-http";
in
stdenvNoCC.mkDerivation {
  name = "${baseNameOf repository}.dar";
  nativeBuildInputs = [
    oras
    jq
  ];

  outputHashMode = "flat";
  outputHashAlgo = "sha256";
  outputHash = darHash;

  impureEnvVars = lib.fetchers.proxyImpureEnvVars;
  SSL_CERT_FILE = "${cacert}/etc/ssl/certs/ca-bundle.crt";

  buildCommand = /* sh */ ''
    oras manifest fetch ${plainHttp} --output manifest.json ${lib.escapeShellArg "${repository}@${manifestDigest}"}
    jq -e --arg digest ${lib.escapeShellArg darDigest} \
      'any(.layers[]; .digest == $digest and (.annotations["org.opencontainers.image.title"] // "" | endswith(".dar")))' manifest.json
    oras blob fetch ${plainHttp} --output "$out" ${lib.escapeShellArg "${repository}@${darDigest}"}
  '';
}
