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
        description = "Package for monocurl";
        default = packages.${pkgs.stdenv.hostPlatform.system}.default;
        type = types.package;
      };
    };

    config = mkIf cfg.enable {
      home = {
        packages = [cfg.package];

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
                src = ../assets/AppIcon.appiconset + "/${filename}";
              };
            })
            iconSet.images);
      };
      xdg = let
        mimeType = ["text/x-monocurl-scene" "text/x-monocurl-library"];
      in {
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
        mimeApps.associations.added = builtins.listToAttrs (builtins.map (mt: {
            name = mt;
            value = "monocurl.desktop";
          })
          mimeType);
      };
    };
  }
