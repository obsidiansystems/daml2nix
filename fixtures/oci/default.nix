{ buildDamlPackage, fetchOciDar }:
buildDamlPackage {
  name = "trading";
  version = "1.0.0";
  root = ./.;
  path = ".";
  dependencies = [
    "daml-prim"
    "daml-stdlib"
  ];
  dataDependencies = [
    (fetchOciDar {
      uri = "oci://127.0.0.1:5800/fixtures/asset:1.0.0";
      manifestDigest = "sha256:82e615616ad27ff42385902f479384c5514752e67d5ef2095ca126846402e3eb";
      darDigest = "sha256:163dba677ec64c0be949e2086dcb05511194a129750d75c4610d71535fd68457";
      insecure = true;
    })
  ];
}
