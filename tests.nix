{
  lib,
  system,
  rustPlatform-cov,
  rust-toolchain-llvm-tools,
  python3,
  assetFilter,
  sourceFilter,
}:

let
  inherit (python3.pkgs) lzallright;
  testFilter = p: t: builtins.match ".*/(pyproject\.toml|tests|tests/.*\.py)$" p != null;
in
{
  pytest =
    with python3.pkgs;
    buildPythonPackage {
      inherit (lzallright) version;
      pname = "${lzallright.pname}-tests-pytest";
      format = "other";

      src = lib.cleanSourceWith {
        src = ./.;
        filter = p: t: (testFilter p t) || (assetFilter p t);
      };

      dontBuild = true;
      dontInstall = true;

      nativeCheckInputs = [
        lzallright
        pytestCheckHook
      ];
    };
}
// lib.optionalAttrs (system == "x86_64-linux") {
  coverage =
    let
      lzallright-cov = lzallright.override {
        coverage = true;
        rustPlatform = rustPlatform-cov;
        cargo = rust-toolchain-llvm-tools;
        rustc = rust-toolchain-llvm-tools;
      };
    in
    with python3.pkgs;
    buildPythonPackage {
      inherit (lzallright) version cargoDeps;
      pname = "${lzallright.pname}-tests-coverage";
      format = "other";

      src = lib.cleanSourceWith {
        src = ./.;
        filter = p: t: (sourceFilter p t) || (testFilter p t) || (assetFilter p t);
      };

      dontBuild = true;
      dontInstall = true;

      preCheck = ''
        source <(cargo llvm-cov show-env --export-prefix)
        LLVM_COV_FLAGS=$(echo -n $(find ${lzallright-cov} -name "*.so"))
        export LLVM_COV_FLAGS
      '';

      postCheck = ''
        rm -r $out
        cargo llvm-cov report -vv --ignore-filename-regex cargo-vendor-dir --codecov --output-path $out
      '';

      nativeBuildInputs = (
        with rustPlatform-cov;
        [
          cargoSetupHook
        ]
      );

      nativeCheckInputs = with pkgs; [
        rust-toolchain-llvm-tools
        cargo-llvm-cov
        lzallright-cov
        pytestCheckHook
      ];
    };
}
