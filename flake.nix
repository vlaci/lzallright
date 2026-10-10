{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane-maturin.url = "github:vlaci/crane-maturin";
    shell-hooks.url = "github:vlaci/nix-shell-hooks";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      crane-maturin,
      advisory-db,
      git-hooks,
      treefmt-nix,
      shell-hooks,
      rust-overlay,
      ...
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      nixpkgsFor = forAllSystems (
        system:
        import nixpkgs {
          inherit system;
          overlays = [
            self.overlays.default
            rust-overlay.overlays.default
            shell-hooks.overlays.default
          ];
        }
      );

      rustToolchainFor = forAllSystems (
        system:
        nixpkgsFor.${system}.rust-bin.selectLatestNightlyWith (
          toolchain:
          toolchain.default.override {
            extensions = [
              "cargo"
              "clippy"
              "rust-src"
              "rustc"
              "rustfmt"
            ];
          }
        )
      );

      treefmtFor = forAllSystems (
        system:
        treefmt-nix.lib.evalModule nixpkgsFor.${system} {
          projectRootFile = "flake.nix";
          programs = {
            nixfmt.enable = true;
            ruff-format.enable = true;
            rustfmt = {
              enable = true;
              package = rustToolchainFor.${system};
              edition = "2021";
            };
          };
        }
      );

      pre-commit-check = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          rust = rustToolchainFor.${system};
        in
        git-hooks.lib.${system}.run {
          src = ./.;
          package = pkgs.prek;
          hooks = {
            check-added-large-files.enable = true;
            end-of-file-fixer = {
              enable = true;
              excludes = [ "^benches/corpus/" ];
            };
            check-yaml.enable = true;
            check-toml.enable = true;
            treefmt = {
              enable = true;
              package = treefmtFor.${system}.config.build.wrapper;
            };
            statix.enable = true;
            deadnix.enable = true;
            ruff.enable = true;
            cargo-check = {
              enable = true;
              package = rust;
            };
            clippy = {
              enable = true;
              packageOverrides.cargo = rust;
              packageOverrides.clippy = rust;
            };
          };
        }
      );
    in
    {
      overlays.default =
        final: prev:
        let
          cmLib = crane-maturin.mkLib crane final;

          assetFilter =
            path: _type: builtins.match ".*/benches(/corpus)?(/[^/]+\\.(txt|c|json|log))?$" path != null;
          pyFilter =
            path: _type: builtins.match ".*pyi?$|.*/py\.typed$|.*/README.md$|.*/LICENSE$" path != null;
          sourceFilter = path: type: (assetFilter path type) || (cmLib.filterCargoSources path type);
          testFilter = p: _t: builtins.match ".*/(pyproject\.toml|tests|tests/.*\.py)$" p != null;

        in
        {
          pythonPackagesExtensions = prev.pythonPackagesExtensions ++ [
            (_python-final: _python-prev: {
              lzallright = cmLib.buildMaturinPackage {
                src = final.lib.cleanSourceWith {
                  src = cmLib.path ./.;
                  filter = p: t: (pyFilter p t) || (sourceFilter p t);
                };
                testSrc = final.lib.cleanSourceWith {
                  src = ./.;
                  filter = p: t: (sourceFilter p t) || (testFilter p t) || (assetFilter p t);
                };
                inherit advisory-db;
              };
            })
          ];
        };
      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          inherit (pkgs.python3Packages) lzallright;
          pytestWith = python: (lzallright.override { inherit python; }).passthru.tests.pytest;
        in
        lzallright.passthru.tests
        // {
          pytest-py314t = pytestWith pkgs.python314FreeThreading;
          pytest-py315t = pytestWith pkgs.python315FreeThreading;
        }
      );

      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          inherit (pkgs.python3Packages) lzallright;
        in
        {
          inherit lzallright;
          default = lzallright;
          bench-corpus = pkgs.callPackage ./benches/corpus { };

          silesia =
            pkgs.runCommand "silesia"
              {
                nativeBuildInputs = [ pkgs.unzip ];
                src = pkgs.fetchurl {
                  url = "https://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip";
                  sha256 = "0626e25f45c0ffb5dc801f13b7c82a3b75743ba07e3a71835a41e3d9f63c77af";
                };
              }
              ''
                mkdir -p $out
                unzip -q $src -d $out/silesia
              '';
        }
      );

      apps = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          corpus = self.packages.${system}.bench-corpus;
        in
        {
          update-bench-corpus = {
            type = "app";
            meta.description = "Regenerate benches/corpus from pinned upstream sources";
            program = pkgs.lib.getExe (
              pkgs.writeShellApplication {
                name = "update-bench-corpus";
                text = "install -m 0644 -t benches/corpus ${corpus}/*";
              }
            );
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
        in
        {
          default = pkgs.mkShell {
            inherit (pre-commit-check.${system}) shellHook;
            packages =
              with pkgs;
              [
                cargo-msrv
                cargo-fuzz
                cargo-nextest
                rustToolchainFor.${system}
                rust-analyzer
                gnuplot
                python3Packages.uvVenvShellHook
                python3Packages.maturinImportShellHook
                python3Packages.autoPatchelfVenvShellHook
                gdb
                lzo
              ]
              ++ pre-commit-check.${system}.enabledPackages;
            uvExtraArgs = [
              "--group"
              "test"
              "--group"
              "docs"
            ];
          };
        }
      );

      formatter = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          inherit (pre-commit-check.${system}.config) package configFile;
        in
        pkgs.runCommand "pre-commit-run"
          {
            nativeBuildInputs = [ pkgs.makeWrapper ];
            meta.mainProgram = "pre-commit-run";
          }
          ''
            makeWrapper ${pkgs.lib.getExe package} $out/bin/pre-commit-run \
              --suffix PATH : ${
                pkgs.lib.makeBinPath [
                  rustToolchainFor.${system}
                  pkgs.stdenv.cc
                  pkgs.python3
                ]
              } \
              --add-flags "run --all-files --config ${configFile}"
          ''
      );
    };
}
