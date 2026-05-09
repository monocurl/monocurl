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
        pkgs = nixpkgs.legacyPackages.${system};

        craneLib = crane.mkLib pkgs;

        # https://github.com/ipetkov/crane/issues/400#issuecomment-1739612918
        dummySrc = craneLib.mkDummySrc {
          src = craneLib.path ./.;
          extraDummyScript = ''
            set -exuo pipefail
            cp -rf --no-target-directory ${./vendor} $out/vendor
          '';
        };

        CFLAGS = "-Wno-int-conversion";
        CXXFLAGS = "-std=c++17";

        baseArgs = {
          installCargoArtifactsMode = "use-zstd";

          inherit CFLAGS CXXFLAGS;
        };

        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        buildInputs = with pkgs; [
          libpng
          graphite2
          freetype
          icu
          openssl
          fontconfig
        ];

        cargoArtifacts = craneLib.buildDepsOnly (baseArgs
          // {
            inherit dummySrc nativeBuildInputs buildInputs;

            pname = "monocurl-deps";
          });

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

        monocurl = craneLib.buildPackage (baseArgs
          // {
            src = ./.;
            strictDeps = true;
            # A lot of tests are failing, need to figure out why
            doCheck = false;

            inherit cargoArtifacts;

            nativeBuildInputs =
              nativeBuildInputs
              ++ (with pkgs; [
                makeWrapper
              ]);

            buildInputs =
              buildInputs
              ++ runtimeLibs;

            cargoTestCommand = ''
              MONOCURL_ASSETS_DIR=${./assets} cargo test --profile release
            '';

            installPhaseCommand = let
              iconSet = builtins.fromJSON (builtins.readFile ./assets/AppIcon.appiconset/Contents.json);

              installIcons = builtins.map ({
                size,
                filename,
                ...
              }: "install -Dm444 ${./assets/AppIcon.appiconset}/${filename} $out/share/monocurl/icons/hicolor/${size}/apps/monocurl.png")
              iconSet.images;
            in ''
              mkdir -p $out

              ${builtins.concatStringsSep "\n" installIcons}
            '';

            postInstall = ''
              wrapProgram $out/bin/monocurl \
                --prefix LD_LIBRARY_PATH : ${LD_LIBRARY_PATH}
                --set MONOCURL_ASSETS_DIR ${./assets}
            '';
          });
      in {
        checks = {
          inherit monocurl;
        };

        packages.default = monocurl;

        apps.default = flake-utils.lib.mkApp {
          drv = monocurl;
        };

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
        monocurl = {
          config,
          pkgs,
          ...
        }:
          with pkgs.lib; {
            options = {
              programs.monocurl = {
                enable = mkEnableOption "monocurl";
                package = mkOpion {
                  description = "Package for monocurl";
                  default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
                  type = types.package;
                };
              };
            };

            config = mkIf config.programs.monocurl.enable {
              home = {
                xdg.desktopEntries.monocurl = {
                  type = "Application";
                  name = "Monocurl";
                  comment = "Mathematical animation editor";
                  exec = "${config.programs.monocurl.package}/bin/monocurl %F";
                  icon = "monocurl";
                  terminal = false;
                  categories = ["Education"];
                  mimeType = ["text/x-monocurl-scene" "text/x-monocurl-library"];
                };

                packages = [config.programs.monocurl.package];
              };
            };
          };
      };
    };
}
