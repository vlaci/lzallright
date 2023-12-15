inputs:

{ lib
, system
, stdenv
, python3
, libiconv
, maturin
, cargo
, rustc
, rustPlatform
, makeRustPlatform
, cargo-llvm-cov
, coverage ? false
}:

let
  inherit (lib) optional optionalString;
  craneLib = inputs.crane.lib.${system};
  cppFilter = path: _type: builtins.match ".*(h|c)pp$" path != null;
  assetFilter = path: _type: builtins.match ".*(benches|benches/.*\.txt)$" path != null;

  sourceFilter = path: type:
    (cppFilter path type) || (assetFilter path type) || (craneLib.filterCargoSources path type);

  src = lib.cleanSourceWith {
    src = craneLib.path ./.;
    filter = p: t: (pyFilter p t) || (sourceFilter p t);
  };


  # Common arguments can be set here to avoid repeating them later
  commonArgs = {
    inherit src;

    # python package  build will recompile PyO3 when built with maturin
    # as there are different build features are used for the extension module
    # and the standalone dylib which is used for tests and benchmarks
    doNotLinkInheritedArtifacts = true;

    buildInputs = [
      python3
    ] ++ lib.optionals stdenv.isDarwin [
      libiconv
    ];
  };

  # Build *just* the cargo dependencies, so we can reuse
  # all of that work (e.g. via cachix) when running in CI
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Build the actual crate itself, reusing the dependency
  # artifacts from above.
  liblzallright = craneLib.buildPackage (commonArgs // {
    inherit cargoArtifacts;
  });


  rust-toolchain-llvm-tools = inputs.fenix.packages.${system}.complete.withComponents [
    "llvm-tools-preview"
    "cargo"
    "rustc"
  ];
  rustPlatform-cov = makeRustPlatform {
    rustc = rust-toolchain-llvm-tools;
    cargo = rust-toolchain-llvm-tools;
  };

  craneLibLLvmTools = craneLib.overrideToolchain rust-toolchain-llvm-tools;

  pyFilter = path: _type: builtins.match ".*pyi?$|.*/py\.typed$|.*/README.md$|.*/LICENSE$" path != null;
in
python3.pkgs.buildPythonPackage
  (commonArgs //
  {
    pname = liblzallright.pname + (optionalString coverage "-coverage");
    inherit (liblzallright) version src;
    format = "pyproject";

    strictDeps = true;
    doCheck = false;
    cargoDeps = rustPlatform.importCargoLock {
      lockFile = ./Cargo.lock;
    };

    preConfigure = optionalString coverage ''
      source <(cargo llvm-cov show-env --export-prefix)
    '';

    nativeBuildInputs = with rustPlatform; [
      cargoSetupHook
      (maturinBuildHook.override { pkgsHostTarget = { inherit maturin cargo rustc; }; })
    ] ++ optional coverage cargo-llvm-cov;

    passthru = {
      inherit cargoArtifacts craneLib craneLibLLvmTools commonArgs liblzallright;
      tests = import ./tests.nix {
        inherit lib system rustPlatform-cov rust-toolchain-llvm-tools python3 sourceFilter assetFilter;
      };
    };
  })
