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

        # https://github.com/monocurl/monocurl/issues/4
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
            cargoTextExtraArgs = let
              # Skipping these tests due to nixs sandboxed builds with no write
              # priviledges
              skippedTests = [
                "errors::test_scene_snapshot_accepts_mid_write_text_mesh"
                "sync::test_rearrangement_scene_final_slide_seek_scan_stays_stable"
                "sync::test_rearrangement_scene_seeks_and_plays_each_slide_without_planar_trans_panic"
                "sync::test_scale_scales_text_about_global_tree_center"
                "sync::test_tag_trans_preserves_colored_greek_tex_boundaries_after_write"
                "sync::test_tex_trans_between_hole_heavy_strings_stays_stable"
                "sync::test_tex_trans_between_strings_stays_stable"
                "sync::test_text_trans_between_hole_heavy_strings_stays_stable"
                "sync::test_text_trans_between_strings_stays_stable"
                "sync::test_text_trans_h_to_b_preserves_hole_winding_at_end"
              ];
              skippedTestsStr = pkgs.lib.concatStringsSep " " (pkgs.lib.map (testId: "--skip=${testId}") skippedTests);
            in "-- ${skippedTestsStr} ";

            installPhaseCommand = let
              iconSet = builtins.fromJSON (builtins.readFile ./assets/AppIcon.appiconset/Contents.json);

              # Native "by the book" installation of icons
              installIcons = builtins.map ({
                size,
                filename,
                ...
              }: "install -Dm444 ${./assets/AppIcon.appiconset}/${filename} $out/share/icons/hicolor/${size}/apps/monocurl.png")
              iconSet.images;
              # Copied the default from https://github.com/ipetkov/crane/blob/master/lib/buildPackage.nix
            in ''
              if [ -n "$postBuildInstallFromCargoBuildLogOut" -a -d "$postBuildInstallFromCargoBuildLogOut" ]; then
                echo "actually installing contents of $postBuildInstallFromCargoBuildLogOut to $out"
                mkdir -p $out
                find "$postBuildInstallFromCargoBuildLogOut" -mindepth 1 -maxdepth 1 | xargs -r mv -t $out
              else
                echo ${pkgs.lib.strings.escapeShellArg ''
                !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
                $postBuildInstallFromCargoBuildLogOut is either undefined or does not point to a
                valid location! By default `buildPackage` will expect that cargo's output was
                captured and the resulting binaries preinstalled in a temporary location to avoid
                interference by the check phase.

                If you are defining your own custom build step, you have two options:
                1. override `installPhaseCommand` with the appropriate installation steps
                2. ensure that cargo's build log is captured in a file and point
                  $postBuildInstallFromCargoBuildLogOut at it

                At a minimum, the latter option can be achieved with a build phase that runs:
                    cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)
                    cargo build --release --message-format json-render-diagnostics >"$cargoBuildLog"
                    postBuildInstallFromCargoBuildLogOut=$(mktemp -d cargoBuildTempOutXXXX)
                    installFromCargoBuildLog "$postBuildInstallFromCargoBuildLogOut" "$cargoBuildLog"
                !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
              ''}
                false
              fi

              ${builtins.concatStringsSep "\n" installIcons}
            '';

            # Make a wrapper for the binaty to be able to see the assets and the runtime dynamic library dependencies
            postInstall = ''
              wrapProgram $out/bin/monocurl \
                --prefix LD_LIBRARY_PATH : ${LD_LIBRARY_PATH} \
                --set MONOCURL_ASSETS_DIR ${./assets}
            '';
          });
      in {
        checks = {
          inherit monocurl;
        };

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
