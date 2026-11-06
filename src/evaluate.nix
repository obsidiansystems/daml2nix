{ flake, generated }:
let
  lib = (builtins.getFlake flake).lib.${builtins.currentSystem};
  function = import (/. + generated);
  packages = function (builtins.intersectAttrs (builtins.functionArgs function) lib);
in
{
  inherit packages;

  summary = builtins.mapAttrs (_: package: {
    source = package.src.outPath;
    derivation = package.drvPath;
    dar = builtins.unsafeDiscardStringContext package.dar;
    inherit (package) directory;
  }) packages;
}
