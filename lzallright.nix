inputs:

{ callPackage
, lib
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
, zstd
, rsync
, rust
}:

let
  inherit (lib) optional optionalString;
  mkMaturinDerivation = callPackage ./crane-maturin.nix { inherit (inputs) crane; };
  craneLib = inputs.crane.lib.${system};
  cppFilter = path: _type: builtins.match ".*(h|c)pp$" path != null;
  assetFilter = path: _type: builtins.match ".*(benches|benches/.*\.txt)$" path != null;

  pyFilter = path: _type: builtins.match ".*pyi?$|.*/py\.typed$|.*/README.md$|.*/LICENSE$" path != null;
  sourceFilter = path: type:
    (cppFilter path type) || (assetFilter path type) || (craneLib.filterCargoSources path type);

  src = lib.cleanSourceWith {
    src = craneLib.path ./.;
    filter = p: t: (pyFilter p t) || (sourceFilter p t);
  };

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

in
mkMaturinDerivation {
  inherit src;
  doCheck = false;
}
