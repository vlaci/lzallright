{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, rust-overlay, flake-utils, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib makeRustPlatform python3Packages;

        rust-toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;
        cppFilter = path: _type: builtins.match ".*(h|c)pp$" path != null;
        assetFilter = path: _type: builtins.match ".*(benches|benches/.*\.txt)$" path != null;

        sourceFilter = path: type:
          (cppFilter path type) || (assetFilter path type) || (craneLib.filterCargoSources path type);
        src = lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = sourceFilter;
        };

        # Common arguments can be set here to avoid repeating them later
        commonArgs = {
          inherit src;

          buildInputs = [
            pkgs.python3
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
          ];

          # Additional environment variables can be set directly
          # MY_CUSTOM_VAR = "some value";
        };

        rust-toolchain-cov = rust-toolchain.override {
          extensions = [
            "llvm-tools-preview"
          ];
        };
        craneLibLLvmTools = craneLib.overrideToolchain
          rust-toolchain-cov
        ;

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        liblzallright = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

        rustPlatform = makeRustPlatform {
          rustc = rust-toolchain;
          cargo = rust-toolchain;
        };
        rustPlatform-cov = makeRustPlatform {
          rustc = rust-toolchain-cov;
          cargo = rust-toolchain-cov;
        };

        pyFilter = path: _type: builtins.match ".*py$|.*/README.md$|.*/LICENSE" path != null;
        testFilter = p: t: builtins.match ".*/(pyproject\.toml|tests|tests/.*\.py)$" p != null;
        lzallright = python3Packages.buildPythonPackage
          (commonArgs //
            {
              inherit (liblzallright) pname version;
              format = "pyproject";

              src = lib.cleanSourceWith {
                src = craneLib.path ./.;
                filter = p: t: (pyFilter p t) || (sourceFilter p t);
              };

              strictDeps = true;
              doCheck = false;
              cargoDeps = rustPlatform.importCargoLock {
                lockFile = ./Cargo.lock;
              };

              nativeBuildInputs =
                (with rustPlatform; [
                  cargoSetupHook
                  maturinBuildHook
                ]);

              passthru = {
                tests = {
                  coverage =
                    let
                      lzallright-cov = lzallright.overridePythonAttrs (with pkgs; super: {
                        pname = "${super.pname}-coverage";
                        nativeBuildInputs = (with rustPlatform-cov; [
                          cargoSetupHook
                          maturinBuildHook
                          cargo-llvm-cov
                        ]);
                        preConfigure = (super.preConfigure or "") + ''
                          source <(cargo llvm-cov show-env --export-prefix)
                        '';
                      });
                    in
                    with python3Packages; buildPythonPackage
                      {
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

                        nativeBuildInputs =
                          (with rustPlatform; [
                            cargoSetupHook
                          ]);

                        nativeCheckInputs = with pkgs; [
                          rust-toolchain-cov
                          cargo-llvm-cov
                          lzallright-cov
                          pytestCheckHook
                        ];
                      };
                  pytest =
                    with python3Packages; buildPythonPackage
                      {
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
                };
              };
            });
      in
      {
        checks = lzallright.passthru.tests // {
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
            });

            # Run tests with cargo-nextest
            # Consider setting `doCheck = false` on `liblzallright` if you do not want
            # the tests to run twice
            liblzallright-nextest = craneLib.cargoNextest (commonArgs // {
              inherit cargoArtifacts;
              partitions = 1;
              partitionType = "count";
            });
          };

        packages = {
          default = lzallright;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = builtins.attrValues self.checks.${system};

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "

          # Extra inputs can be added here
          nativeBuildInputs = with pkgs;
            [
              maturin
              pdm
            ];
        };
      });
}
