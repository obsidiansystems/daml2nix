{
  lib,
  stdenvNoCC,
  jq,
  sdk,
}:
{
  name,
  version,
  root,
  path,
  dependencies ? [ ],
  dataDependencies ? [ ],
}:
let
  package = root + "/${path}";
  dar = "${name}-${version}.dar";
in
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = name;
  inherit version;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.difference package (lib.fileset.maybeMissing (package + "/.daml"));
  };

  nativeBuildInputs = [ jq ];

  # damlc 3.5+ takes resolved dependency paths from DPM_RESOLUTION_FILE in
  # place of daml.yaml's, so one unmodified daml.yaml drives dpm and Nix.
  resolution = builtins.toJSON {
    sdk-version = sdk.version;
    imports = {
      resolved-dependencies = dependencies;
      resolved-data-dependencies = dataDependencies;
    };
  };

  buildPhase = ''
    runHook preBuild
    cd ${lib.escapeShellArg path}
    jq -n --arg root "$PWD" --arg sdk ${sdk.version} --argjson package "$resolution" \
      '($package | .imports.dars = $ARGS.positional) as $resolved
        | { packages: { ($root): $resolved }, "default-sdk": { ($sdk): $resolved } }' \
      --args ${sdk.damlScript}/*.dar > "$TMPDIR/resolution.json"
    DPM_SDK_VERSION=${sdk.version} DPM_RESOLUTION_FILE="$TMPDIR/resolution.json" DAML_CACHE="$TMPDIR/cache" \
      ${sdk.damlc}/damlc-dist-dpm/damlc build --enable-multi-package=no
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm444 .daml/dist/${dar} $out/${dar}
    runHook postInstall
  '';

  passthru.dar = "${finalAttrs.finalPackage}/${dar}";
})
