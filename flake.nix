# SPDX-FileCopyrightText: 2023 László Vaskó <opensource@vlaci.email>
#
# SPDX-License-Identifier: MIT

{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";

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

    crane-maturin.url = "github:vlaci/crane-maturin";
    shell-hooks.url = "github:vlaci/nix-shell-hooks";
  };

  outputs =
    inputs:
    import ./mkFlake.nix inputs {
      package =
        {
          pkgs,
          craneLib,
          build,
          ...
        }:
        let
          assetFilter =
            path: _type: builtins.match ".*/benches(/corpus)?(/[^/]+\\.(txt|c|json|log))?$" path != null;
          pyFilter =
            path: _type:
            builtins.match ".*pyi?$|.*/py\.typed$|.*/README.md$|.*/LICENSES(/[^/]+\\.txt)?$" path != null;
          sourceFilter = path: type: (assetFilter path type) || (craneLib.filterCargoSources path type);
          testFilter = p: _t: builtins.match ".*/(pyproject\.toml|tests|tests/.*\.py)$" p != null;

        in
        build {
          src = pkgs.lib.cleanSourceWith {
            src = craneLib.path ./.;
            filter = p: t: (pyFilter p t) || (sourceFilter p t);
          };
          testSrc = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = p: t: (sourceFilter p t) || (testFilter p t) || (assetFilter p t);
          };
        };
      pre-commit.hooks = {
        end-of-file-fixer.excludes = [ "^benches/corpus/" ];
        trim-trailing-whitespace.excludes = [ "^benches/corpus/" ];
      };

      packages =
        { pkgs, ... }:
        {
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
        };

      apps =
        { self, pkgs, ... }:
        let
          corpus = self.packages.bench-corpus;
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
        };

      devShell =
        { pkgs, ... }:
        {
          packages = with pkgs; [
            gnuplot
            gdb
            lzo
          ];
        };
    };
}
