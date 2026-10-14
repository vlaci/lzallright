# SPDX-FileCopyrightText: 2026 László Vaskó <opensource@vlaci.email>
#
# SPDX-License-Identifier: MIT
#
# Managed by copier - do not edit.
# https://github.com/vlaci/project-template
rec {
  nightlyRust =
    {
      pkgs,
      extraExtensions ? [ ],
    }:
    pkgs.rust-bin.selectLatestNightlyWith (
      toolchain:
      toolchain.default.override {
        extensions = [
          "cargo"
          "clippy"
          "rust-src"
          "rustc"
          "rustfmt"
        ]
        ++ extraExtensions;
      }
    );

  treefmt =
    {
      pkgs,
      treefmt-nix,
      rust,
      rustEdition,
      module ? { },
    }:
    treefmt-nix.lib.evalModule pkgs {
      imports = [
        {
          projectRootFile = "flake.nix";
          programs = {
            nixfmt.enable = true;
            ruff-format.enable = true;
            rustfmt = {
              enable = true;
              package = rust;
              edition = rustEdition;
            };
            rumdl-format.enable = true;
            taplo.enable = true;
            prettier = {
              enable = true;
              excludes = [
                ".copier-answers.yml"
                "*.md"
                "flake.lock"
              ];
            };
          };
        }
        module
      ];
    };

  preCommit =
    {
      pkgs,
      git-hooks,
      src,
      rust,
      treefmt,
      hooks ? { },
    }:
    git-hooks.lib.${pkgs.stdenv.hostPlatform.system}.run {
      inherit src;
      package = pkgs.prek;
      hooks = pkgs.lib.recursiveUpdate {
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
        treefmt = {
          enable = true;
          package = treefmt.config.build.wrapper;
        };
        statix.enable = true;
        deadnix.enable = true;
        ruff.enable = true;
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
      } hooks;
    };

  devShell =
    {
      pkgs,
      preCommit,
      rust,
      python ? pkgs.python3,
      packages ? [ ],
      uvGroups ? [ ],
    }:
    pkgs.mkShell {
      inherit (preCommit) shellHook;
      packages = [
        rust
        pkgs.rust-analyzer
        python.pkgs.uvVenvShellHook
        python.pkgs.maturinImportShellHook
      ]
      ++ pkgs.lib.optional pkgs.stdenv.hostPlatform.isLinux python.pkgs.autoPatchelfVenvShellHook
      ++ preCommit.enabledPackages
      ++ packages;
      uvExtraArgs = pkgs.lib.concatMap (g: [
        "--group"
        g
      ]) uvGroups;
    };

  formatter =
    {
      pkgs,
      preCommit,
      rust,
      runtimeInputs ? [ ],
      wrapperArgs ? [ ],
    }:
    let
      inherit (preCommit.config) package configFile;
    in
    pkgs.runCommand "pre-commit-run"
      {
        nativeBuildInputs = [ pkgs.makeWrapper ];
        meta.mainProgram = "pre-commit-run";
      }
      ''
        makeWrapper ${pkgs.lib.getExe package} $out/bin/pre-commit-run \
          --suffix PATH : ${
            pkgs.lib.makeBinPath (
              [
                rust
                pkgs.stdenv.cc
                pkgs.python3
              ]
              ++ runtimeInputs
            )
          } \
          ${pkgs.lib.concatStringsSep " " wrapperArgs} \
          --add-flags "run --all-files --config ${configFile}"
      '';

  # Per-system outputs: `eachSystem ({ pkgs, pt }: …)`. nixpkgs and tooling are
  # instantiated once per system and shared by every output.
  eachSystem =
    {
      inputs,
      systems ? [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ],
      overlays ? [ ],
      src,
      rustEdition,
      treefmtModule ? { },
      # attrset, or `pkgs: { … }` when hooks reference packages
      hooks ? { },
    }:
    let
      ctxFor = inputs.nixpkgs.lib.genAttrs systems (
        system:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = overlays ++ [
              inputs.rust-overlay.overlays.default
              inputs.shell-hooks.overlays.default
            ];
          };
          rust = nightlyRust { inherit pkgs; };
          treefmtEval = treefmt {
            inherit pkgs rust rustEdition;
            inherit (inputs) treefmt-nix;
            module = treefmtModule;
          };
          pre = preCommit {
            inherit pkgs rust src;
            inherit (inputs) git-hooks;
            treefmt = treefmtEval;
            hooks = pkgs.lib.toFunction hooks pkgs;
          };
        in
        {
          inherit pkgs;
          pt = {
            inherit rust;
            treefmt = treefmtEval;
            preCommit = pre;
            formatter = formatter {
              inherit pkgs rust;
              preCommit = pre;
            };
            devShell =
              args:
              devShell (
                {
                  inherit pkgs rust;
                  preCommit = pre;
                }
                // args
              );
          };
        }
      );
    in
    f: builtins.mapAttrs (_: f) ctxFor;
}
