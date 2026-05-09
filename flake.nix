{
  description = "Monocurl - A desktop application used for creating math-based videos and slideshows";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
        };

        craneLib = crane.mkLib pkgs;

        # https://github.com/monocurl/monocurl/issues/4
        CFLAGS = "-Wno-int-conversion";
        CXXFLAGS = "-std=c++17";

        runtimeLibs = with pkgs; [
          libxkbcommon
          libxcb

          vulkan-loader
          libGL

          wayland
          libx11
        ];

        LD_LIBRARY_PATH =
          pkgs.lib.makeLibraryPath runtimeLibs;

        monocurl = pkgs.callPackage ./nix/package.nix {inherit craneLib;};
      in {
        formatter = pkgs.alejandra;
        checks = {inherit monocurl;};

        packages = {
          inherit monocurl;
          default = monocurl;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = monocurl;
        };

        # Basic devshell setup for working with the codebase
        devShells.default = craneLib.devShell {
          checks = self.checks.${system};

          inherit LD_LIBRARY_PATH CFLAGS CXXFLAGS;

          packages = with pkgs; [
            rust-analyzer
          ];
        };
      }
    )
    // {
      homeModules = {
        default = self.homeModules.monocurl;
        monocurl = import ./nix/hm-module.nix self.packages;
      };
    };
}
