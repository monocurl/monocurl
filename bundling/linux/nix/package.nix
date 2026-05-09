{
  # Util
  lib,
  craneLib,
  makeWrapper,
  # Static libraries
  pkg-config,
  libpng,
  graphite2,
  freetype,
  icu,
  openssl,
  fontconfig,
  # Runtime libraries
  libxkbcommon,
  libxcb,
  vulkan-loader,
  libGL,
  wayland,
  libx11,
  ...
}: let
  root = ../../..;

  # https://github.com/ipetkov/crane/issues/400#issuecomment-1739612918
  dummySrc = craneLib.mkDummySrc {
    src = craneLib.path root;
    extraDummyScript = ''
      set -exuo pipefail
      cp -rf --no-target-directory ${root}/vendor $out/vendor
    '';
  };

  # https://github.com/monocurl/monocurl/issues/4
  CFLAGS = "-Wno-int-conversion";
  CXXFLAGS = "-std=c++17";

  baseArgs = {
    installCargoArtifactsMode = "use-zstd";

    inherit CFLAGS CXXFLAGS;
  };

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
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
      doCheck = false;
    });

  runtimeLibs = [
    libxkbcommon
    libxcb

    vulkan-loader
    libGL

    wayland
    libx11
  ];

  LD_LIBRARY_PATH =
    lib.makeLibraryPath runtimeLibs;
in
  craneLib.buildPackage (baseArgs
    // {
      src = root;
      strictDeps = true;

      inherit cargoArtifacts;

      nativeBuildInputs =
        nativeBuildInputs
        ++ [
          makeWrapper
        ];

      buildInputs =
        buildInputs
        ++ runtimeLibs;
      # The default --locked breaks tests
      cargoExtraArgs = "";

      cargoTestCommand = let
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

          "live_values::test_axis2d_grid_spans_plot_area"
          "live_values::test_axis2d_infers_scale_from_basis_vectors"
          "live_values::test_axis2d_separates_axis_and_grid_color"
          "live_values::test_axis2d_uses_leading_optional_axis_labels"
          "live_values::test_axis3d_draws_axis_arrows_after_grid"
          "live_values::test_axis3d_label_up_controls_title_orientation"
          "live_values::test_axis_large_ticks_have_larger_stroke_radius"

          "live_values::test_axis_style_arrow_extrusion_controls_bounds"
          "live_values::test_axis_style_nil_label_map_suppresses_tick_labels"
          "live_values::test_axis_style_updates_axis_defaults"
          "live_values::test_label_buffer_controls_offset_distance"
          "live_values::test_label_matches_latex_next_to_geometry"
          "live_values::test_label_places_latex_to_requested_side"
          "live_values::test_label_preserves_cross_axis_alignment"
          "live_values::test_number_constructor_accepts_decimal_and_sign_options"

          "live_values::test_tex_and_latex_accept_list_string_inputs"
          "live_values::test_text_tag_operator_tags_text_backends"

          "number::tests::number_renderer_lays_out_cached_glyphs"
          "render::tests::tex_and_text_have_similar_scale"
          "render::tests::tex_digits_and_letters_keep_expected_bounds"
          "render::tests::text_monocurl_has_consistent_topology"
        ];
        skippedTestsStr = lib.concatStringsSep " " (lib.map (testId: "--skip=${testId}") skippedTests);
      in "MONOCURL_ASSETS_DIR=${root}/assets cargo test --profile release -j $NIX_BUILD_CORES --offline -- --test-threads=$NIX_BUILD_CORES ${skippedTestsStr}";

      installPhaseCommand = let
        iconSet = builtins.fromJSON (builtins.readFile "${root}/assets/AppIcon.appiconset/Contents.json");

        # Native "by the book" installation of icons
        installIcons = builtins.map ({
          size,
          filename,
          ...
        }: "install -Dm444 ${root}/assets/AppIcon.appiconset/${filename} $out/share/icons/hicolor/${size}/apps/monocurl.png")
        iconSet.images;
        # Copied the default from https://github.com/ipetkov/crane/blob/master/lib/buildPackage.nix
      in ''
        if [ -n "$postBuildInstallFromCargoBuildLogOut" -a -d "$postBuildInstallFromCargoBuildLogOut" ]; then
          echo "actually installing contents of $postBuildInstallFromCargoBuildLogOut to $out"
          mkdir -p $out
          find "$postBuildInstallFromCargoBuildLogOut" -mindepth 1 -maxdepth 1 | xargs -r mv -t $out
        else
          echo ${lib.strings.escapeShellArg ''
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
          --set MONOCURL_ASSETS_DIR ${root}/assets
      '';

      passthru = {
        runtimeLibsPath = LD_LIBRARY_PATH;
      };

      meta = {
        description = "A desktop application used for creating math-based videos and slideshows ";
        homepage = "https://github.com/monocurl/monocurl";
        license = lib.licenses.mit;
        maintainers = with lib.maintainers; [
          tukanoidd
          enigmurl
        ];
        mainProgram = "monocurl";
      };
    })
