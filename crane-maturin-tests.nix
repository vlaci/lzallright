{ lib, craneLib, system, cargo, drv, commonArgs, cargoArtifacts, crate, llvm, testSrc, python3, cargo-llvm-cov }:

let
  inherit (python3.pkgs) buildPythonPackage pytestCheckHook;

  mkPytest = { nameSuffix, ... }@args: buildPythonPackage ({
    inherit (drv) version;
    pname = "${drv.pname}-test-${nameSuffix}";
    format = "other";

    src = testSrc;

    dontBuild = true;
    dontInstall = true;
  } // (builtins.removeAttrs args [ "nameSuffix" ]));
in
{
  pytest = mkPytest {
    nameSuffix = "pytest";
    nativeCheckInputs = [
      drv
      pytestCheckHook
    ];
  };

  clippy = craneLib.cargoClippy (commonArgs // {
    inherit cargoArtifacts;
    cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  });


  doc = craneLib.cargoDoc (commonArgs // {
    inherit cargoArtifacts;
  });

  nextest = craneLib.cargoNextest (commonArgs // {
    inherit cargoArtifacts;
    partitions = 1;
    partitionType = "count";
  });

} // lib.optionalAttrs (system == "x86_64-linux") {
  pytest-coverage = mkPytest {
    nameSuffix = "pytest-coverage";

    nativeCheckInputs = [
      drv.withCoverage
      cargo
      cargo-llvm-cov
      pytestCheckHook
    ];

    env = {
      LLVM_COV = "${llvm}/bin/llvm-cov";
      LLVM_PROFDATA = "${llvm}/bin/llvm-profdata";
    };

    preCheck = ''
      source <(cargo llvm-cov show-env --export-prefix)
      LLVM_COV_FLAGS=$(echo -n $(find ${drv.withCoverage} -name "*.so"))
      export LLVM_COV_FLAGS
    '';

    postCheck = ''
      rm -r $out
      cargo llvm-cov report --ignore-filename-regex "(/nix/store|/std/src)" --summary-only
      cargo llvm-cov report --ignore-filename-regex "(/nix/store|/std/src)" --codecov --output-path $out
    '';

  };
}
