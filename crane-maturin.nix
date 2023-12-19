{ lib
, crane
, system
, makeRustPlatform
, callPackage
}:

let
  craneLib = crane.lib.${system};
  inherit (lib) optional optionalAttrs optionalString;

  mkMaturinDerivation = { stdenv, python3, libiconv, maturin, cargo, rustc, cargo-llvm-cov, zstd, rsync, rust, llvm }: args:
    let
      drv = lib.makeOverridable
        (args@{ src, testSrc ? src, coverage ? false, ... }: python3.pkgs.buildPythonPackage
          (
            let
              commonArgs = {
                inherit src;

                doNotLinkInheritedArtifacts = true;
                installCargoArtifactsMode = "use-zstd";
                env.PYO3_PYTHON = "${python3}/bin/python";

                buildInputs = [
                  python3
                ] ++ lib.optionals stdenv.isDarwin [
                  libiconv
                ];
              };

              cargoVendorDir = craneLib.vendorCargoDeps { inherit src; };

              cargoMaturinArtifacts = craneLib.buildDepsOnly ((builtins.removeAttrs commonArgs [ "src" ]) // {
                pnameSuffix = "maturin-deps";
                dummySrc = craneLib.mkDummySrc {
                  inherit src;
                  extraDummyScript =
                    let
                      pyprojectToml = builtins.fromTOML (builtins.readFile (src + "/pyproject.toml"));
                      cleanedPyprojectToml = {
                        inherit (pyprojectToml) build-system;
                        tool = {
                          inherit (pyprojectToml.tool) maturin;
                        };
                      };
                    in ''
                      cp ${craneLib.writeTOML "pyproject.toml" cleanedPyprojectToml} $out/pyproject.toml
                    '';
                };

                buildPhaseCargoCommand = "${rust.envVars.setEnv} maturin build --manylinux off --release";
                doCheck = false;
                cargoToml = src + "/Cargo.toml";
                inherit cargoVendorDir;
                nativeBuildInputs = [ maturin python3 rustc ];
              });

              cargoArtifacts = (craneLib.buildDepsOnly (commonArgs // {
                inherit  cargoVendorDir;
              })).overrideAttrs (_: { cargoArtifacts = cargoMaturinArtifacts; });

              crate = craneLib.buildPackage (commonArgs // {
                inherit cargoArtifacts;
              });

              rustPlatform = makeRustPlatform { inherit cargo rustc; };

            in
            (lib.recursiveUpdate (commonArgs // (builtins.removeAttrs args [ "cargo" "rustc" "coverage" ])) {
              inherit (crate) version;
              pname = crate.pname + (optionalString coverage "-coverage");

              inherit cargoVendorDir;
              cargoArtifacts = cargoMaturinArtifacts;
              format = "pyproject";

              strictDeps = true;

              preConfigure = optionalString coverage ''
                source <(cargo llvm-cov show-env --export-prefix)
              '' + (args.preConfigure or "");

              env = {
                CARGO_LOG="cargo::core::compiler::fingerprint=info";
              }
                // optionalAttrs coverage {
                LLVM_COV = "${llvm}/bin/llvm-cov";
                LLVM_PROFDATA = "${llvm}/bin/llvm-profdata";
              };

              nativeBuildInputs = with rustPlatform; with craneLib; [
                cargo
                cargoHelperFunctionsHook
                configureCargoCommonVarsHook
                configureCargoVendoredDepsHook
                inheritCargoArtifactsHook
                installCargoArtifactsHook
                replaceCargoLockHook
                rsync
                zstd
                (maturinBuildHook.override { pkgsHostTarget = { inherit maturin cargo rustc; }; })
              ]
              ++ optional coverage cargo-llvm-cov;

              passthru = {
                tests = callPackage ./crane-maturin-tests.nix { inherit craneLib commonArgs cargoArtifacts crate drv cargo testSrc; };
                withCoverage = drv.override { coverage = true; };
              };
            })
          ))
        args;
    in
    drv;
in
callPackage mkMaturinDerivation { }
