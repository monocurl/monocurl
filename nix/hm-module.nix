packages: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.programs.monocurl;
in
  with lib; {
    options.programs.monocurl = {
      enable = mkEnableOption "monocurl";
      package = mkOption {
        description = "Package derivation for monocurl";
        default = packages.${pkgs.stdenv.hostPlatform.system}.default;
        type = types.package;
      };
    };

    config = mkIf cfg.enable {
      home = {
        packages = [cfg.package];

        # Also copying icons to ~/.local/share/icons in case some application launchers dont look for system paths
        # or dont use XDG_DATA_DIRS
        file = let
          iconSet = builtins.fromJSON (builtins.readFile ../assets/AppIcon.appiconset/Contents.json);
        in
          builtins.listToAttrs (builtins.map ({
              size,
              filename,
              ...
            }: {
              name = ".local/share/icons/hicolor/${size}/apps/monocurl.png";
              value = {
                source = ../assets/AppIcon.appiconset + "/${filename}";
              };
            })
            iconSet.images);
      };
      xdg = let
        mimeType = ["text/x-monocurl-scene" "text/x-monocurl-library"];
      in {
        # More "native" way of creating a desktop entry compared to just copying the existing one
        desktopEntries.monocurl = {
          type = "Application";
          name = "Monocurl";
          comment = "Mathematical animation editor";
          exec = "${cfg.package}/bin/monocurl %F";
          icon = "monocurl";
          terminal = false;
          categories = ["Education"];

          inherit mimeType;
        };
        # Global declaration of mime types
        mimeApps.associations.added = builtins.listToAttrs (builtins.map (mt: {
            name = mt;
            value = "monocurl.desktop";
          })
          mimeType);
      };
    };
  }
