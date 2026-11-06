{
  lib,
  stdenvNoCC,
  mkShellNoCC,
  jq,
  sdk,
}:
{
  name,
  version,
  root,
  path,
  source,
  sdkVersion,
  dependencies ? [ ],
  dataDependencies ? [ ],
}:
assert lib.assertMsg (
  sdkVersion == sdk.version
) "${name} needs SDK ${sdkVersion}, this builder packages ${sdk.version}";
let
  package = root + "/${path}";
  dar = "${name}-${version}.dar";

  # damlc 3.5+ takes resolved dependency paths from DPM_RESOLUTION_FILE in
  # place of daml.yaml's, so one unmodified daml.yaml drives dpm and Nix.
  resolution = builtins.toJSON {
    sdk-version = sdk.version;
    imports = {
      resolved-dependencies = dependencies;
      resolved-data-dependencies = dataDependencies;
    };
  };

  writeResolution = packageRoot: output: ''
    jq -n --arg root ${packageRoot} --arg sdk ${sdk.version} --argjson package "$resolution" \
      '($package | .imports.dars = $ARGS.positional) as $resolved
        | { packages: { ($root): $resolved }, "default-sdk": { ($sdk): $resolved } }' \
      --args ${sdk.damlScript}/*.dar > ${output}'';
in
stdenvNoCC.mkDerivation (finalAttrs: {
  pname = name;
  inherit version resolution;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (package + "/daml.yaml")
      (package + "/${source}")
    ];
  };

  nativeBuildInputs = [ jq ];

  buildPhase = ''
    runHook preBuild
    cd ${lib.escapeShellArg path}
    ${writeResolution ''"$PWD"'' ''"$TMPDIR/resolution.json"''}
    DPM_SDK_VERSION=${sdk.version} DPM_RESOLUTION_FILE="$TMPDIR/resolution.json" DAML_CACHE="$TMPDIR/cache" \
      ${sdk.damlc}/damlc-dist-dpm/damlc build --enable-multi-package=no
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm444 .daml/dist/${dar} $out/${dar}
    runHook postInstall
  '';

  passthru = {
    dar = "${finalAttrs.finalPackage}/${dar}";
    directory = toString package;

    shell = mkShellNoCC {
      inherit resolution;
      packages = [ jq ];
      shellHook = ''
        cd ${lib.escapeShellArg (toString package)}
        mkdir -p .daml/nix
        ${writeResolution ''"$PWD"'' ".daml/nix/resolution.json"}
        export PATH=${sdk.damlc}/damlc-dist-dpm:$PATH
        export DPM_SDK_VERSION=${sdk.version}
        export DPM_RESOLUTION_FILE="$PWD/.daml/nix/resolution.json"
        export DAML_CACHE="$PWD/.daml/nix/cache"
      '';
    };
  };
})
