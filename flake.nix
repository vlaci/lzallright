{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    crane-maturin.url = "github:vlaci/crane-maturin";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      crane-maturin,
      advisory-db,
      ...
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "x86_64-darwin"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      nixpkgsFor = forAllSystems (
        system:
        import nixpkgs {
          inherit system;
          overlays = [ self.overlays.default ];
        }
      );
    in
    {
      overlays.default =
        final: prev:
        let
          cmLib = crane-maturin.mkLib crane final;

          assetFilter = path: _type: builtins.match ".*(benches|benches/.*\.txt)$" path != null;
          cppFilter = path: _type: builtins.match ".*(h|c)pp$" path != null;
          pyFilter =
            path: _type: builtins.match ".*pyi?$|.*/py\.typed$|.*/README.md$|.*/LICENSE$" path != null;
          sourceFilter =
            path: type:
            (cppFilter path type) || (assetFilter path type) || (cmLib.filterCargoSources path type);
          testFilter = p: t: builtins.match ".*/(pyproject\.toml|tests|tests/.*\.py)$" p != null;

        in
        {
          pythonPackagesExtensions = prev.pythonPackagesExtensions ++ [
            (python-final: python-prev: {
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
          inherit (nixpkgsFor.${system}.python3Packages) lzallright;
        in
        lzallright.passthru.tests
      );

      packages = forAllSystems (
        system:
        let
          inherit (nixpkgsFor.${system}.python3Packages) lzallright;
        in
        {
          inherit lzallright;
          default = lzallright;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
        in
        {
          default = pkgs.mkShell {
            inputsFrom = builtins.attrValues self.checks.${system};

            # Extra inputs can be added here
            nativeBuildInputs = with pkgs; [
              maturin
              pdm
              cargo-msrv
              cargo
              rustc
            ];
          };
        }
      );

      formatter = forAllSystems (system: nixpkgsFor.${system}.nixpkgs-fmt);
    };
}
