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
      };

      xdg.desktopEntries.monocurl = {
        type = "Application";
        name = "Monocurl";
        comment = "Mathematical animation editor";
        exec = "${cfg.package}/bin/monocurl %F";
        icon = "monocurl";
        terminal = false;
        categories = ["Education"];
        mimeType = ["text/x-monocurl-scene" "text/x-monocurl-library"];
      };
    };
  }
