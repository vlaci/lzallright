{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, advisory-db, ... }@inputs:
    let
      supportedSystems = [ "x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      nixpkgsFor = forAllSystems (system: import nixpkgs { inherit system; overlays = [ self.overlays.default ]; });
    in
    {
      overlays.default = final: prev: {
        pythonPackagesExtensions = prev.pythonPackagesExtensions ++ [
          (python-final: python-prev: {
            lzallright = final.callPackage (import ./lzallright.nix inputs) { python3 = python-final.python; };
          })
        ];
      };
      checks = forAllSystems (system:
        let
          inherit (nixpkgsFor.${system}) lib;
          inherit (nixpkgsFor.${system}.python3Packages) lzallright;
          inherit (lzallright) liblzallright cargoArtifacts commonArgs craneLib craneLibLLvmTools src;
        in
        lzallright.passthru.tests // {
          # Build the crate as part of `nix flake check` for convenience
          inherit liblzallright;

          # Run clippy (and deny all warnings) on the crate source,
          # again, resuing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          liblzallright-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          liblzallright-doc = craneLib.cargoDoc (commonArgs // {
            inherit cargoArtifacts;
          });

          # Check formatting
          liblzallright-fmt = craneLib.cargoFmt {
            inherit src;
          };

          # Audit dependencies
          liblzallright-audit = craneLib.cargoAudit {
            inherit src advisory-db;
          };
        } // lib.optionalAttrs
          (system == "x86_64-linux")
          {
            # Check code coverage (note: this will not upload coverage anywhere)
            liblzallright-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs // {
              inherit cargoArtifacts;
              cargoLlvmCovExtraArgs = "--ignore-filename-regex /nix/store --codecov --output-path $out";

              env.RUSTFLAGS = "-Z linker-features=-lld";
            });

            # Run tests with cargo-nextest
            # Consider setting `doCheck = false` on `liblzallright` if you do not want
            # the tests to run twice
            liblzallright-nextest = craneLib.cargoNextest (commonArgs // {
              inherit cargoArtifacts;
              partitions = 1;
              partitionType = "count";
            });
          });

      packages = forAllSystems (system:
        let
          inherit (nixpkgsFor.${system}.python3Packages) lzallright;
        in
        {
          inherit lzallright;
          default = lzallright;
        });

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgsFor.${system};
        in
        {
          default = pkgs.mkShell {
            inputsFrom = builtins.attrValues self.checks.${system};

            # Extra inputs can be added here
            nativeBuildInputs = with pkgs;
              [
                maturin
                pdm
                cargo-msrv
              ];
          };
        });

      formatter = forAllSystems (system: nixpkgsFor.${system}.nixpkgs-fmt);
    };
}
