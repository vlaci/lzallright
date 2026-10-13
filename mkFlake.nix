# SPDX-FileCopyrightText: 2026 László Vaskó <opensource@vlaci.email>
#
# SPDX-License-Identifier: MIT

# Generated file, do not edit
# https://github.com/vlaci/project-template
{
  self,
  nixpkgs,
  crane,
  crane-maturin,
  advisory-db,
  git-hooks,
  shell-hooks,
  rust-overlay,
  ...
}:

{
  package,
  name ? (builtins.fromTOML (builtins.readFile "${self}/pyproject.toml")).project.name,
  systems ? nixpkgs.lib.systems.flakeExposed,
  rustToolchain ?
    pkgs:
    pkgs.rust-bin.selectLatestNightlyWith (
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
    ),
  packages ? _: { },
  apps ? _: { },
  checks ? _: { },
  devShell ? _: { },
  pre-commit ? { },
}:

let
  inherit (nixpkgs) lib;

  overlay = final: prev: {
    pythonPackagesExtensions = prev.pythonPackagesExtensions ++ [
      (pyFinal: _: {
        ${name} =
          let
            craneLib = crane-maturin.mkLib crane final;
          in
          package {
            pkgs = final;
            build =
              args:
              craneLib.buildMaturinPackage (
                {
                  inherit (pyFinal) python;
                  inherit advisory-db;
                }
                // args
              );
            inherit craneLib;
            inherit (pyFinal) python;
          };
      })
    ];
  };

  flakeForSystem =
    system:
    let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          overlay
          rust-overlay.overlays.default
          shell-hooks.overlays.default
        ];
      };
      rust = rustToolchain pkgs;
      hooks = git-hooks.lib.${system}.run {
        src = self;
        package = pkgs.prek;
        imports = [ pre-commit ];
        hooks = {
          check-added-large-files.enable = true;
          end-of-file-fixer.enable = true;
          rumdl.enable = true;
          trim-trailing-whitespace = {
            enable = true;
            excludes = [ ".*\\.md" ];
          };
          check-yaml.enable = true;
          check-toml.enable = true;
          check-github-workflows = {
            enable = true;
            name = "check-github-workflows";
            entry = "${pkgs.check-jsonschema}/bin/check-jsonschema --builtin-schema vendor.github-workflows";
            files = "^\\.github/workflows/.*\\.ya?ml$";
          };
          renovate-config-validator = {
            enable = true;
            name = "renovate-config-validator";
            entry = "${pkgs.renovate}/bin/renovate-config-validator --strict";
            files = "^renovate\\.json$";
          };
          taplo.enable = true;
          nixfmt.enable = true;
          statix.enable = true;
          deadnix.enable = true;
          ruff.enable = true;
          ruff-format.enable = true;
          rustfmt = {
            enable = true;
            packageOverrides.cargo = rust;
            packageOverrides.rustfmt = rust;
          };
          reuse = {
            enable = true;
            entry = "${pkgs.reuse}/bin/reuse --no-multiprocessing lint";
          };
          cargo-check = {
            enable = true;
            package = rust;
            entry = ''${rust}/bin/cargo --config "build.rustc='${rust}/bin/rustc'" check'';
          };
          clippy = {
            enable = true;
            settings.denyWarnings = true;
            packageOverrides.cargo = rust;
            packageOverrides.clippy = rust;
          };
        };
      };

      ctx = {
        inherit pkgs rust;
        self = self';
      };
      pkg = pkgs.python3Packages.${name};
      pytestWith = python: python.pkgs.${name}.passthru.tests.pytest;
      ds = devShell ctx;

      self' = {
        packages = {
          default = pkg;
          ${name} = pkg;
        }
        // packages ctx;

        apps = apps ctx;

        checks =
          pkg.passthru.tests
          // {
            pytest-py314t = pytestWith pkgs.python314FreeThreading;
            pytest-py315t = pytestWith pkgs.python315FreeThreading;
          }
          // checks ctx;

        devShells.default = pkgs.mkShell (
          ds
          // {
            shellHook = hooks.shellHook + ds.shellHook or "";
            packages =
              with pkgs;
              [
                rust
                rust-analyzer
                cargo-msrv
                cargo-fuzz
                cargo-nextest
                python3Packages.uvVenvShellHook
                python3Packages.maturinImportShellHook
              ]
              ++ lib.optional stdenv.hostPlatform.isLinux python3Packages.autoPatchelfVenvShellHook
              ++ hooks.enabledPackages
              ++ ds.packages or [ ];
            uvExtraArgs = [ "--all-groups" ] ++ ds.uvExtraArgs or [ ];
          }
        );

        formatter =
          pkgs.runCommand "pre-commit-run"
            {
              nativeBuildInputs = [ pkgs.makeWrapper ];
              meta.mainProgram = "pre-commit-run";
            }
            ''
              makeWrapper ${lib.getExe hooks.config.package} $out/bin/pre-commit-run \
                --suffix PATH : ${
                  lib.makeBinPath [
                    rust
                    pkgs.stdenv.cc
                    pkgs.python3
                  ]
                } \
                --add-flags "run --all-files --config ${hooks.config.configFile}"
            '';
      };
    in
    self';

  perSystem = lib.genAttrs systems flakeForSystem;
in
{
  overlays.default = overlay;
}
// lib.genAttrs [ "packages" "apps" "checks" "devShells" "formatter" ] (
  output: lib.mapAttrs (_: o: o.${output}) perSystem
)
