{ buildDamlPackage }:
rec {
  asset = buildDamlPackage {
    name = "asset";
    version = "1.0.0";
    root = ./.;
    path = "asset";
    dependencies = [
      "daml-prim"
      "daml-stdlib"
    ];
  };

  trading = buildDamlPackage {
    name = "trading";
    version = "1.0.0";
    root = ./.;
    path = "trading";
    dependencies = [
      "daml-prim"
      "daml-stdlib"
    ];
    dataDependencies = [ asset.dar ];
  };
}
