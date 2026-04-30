use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use exporter::DEFAULT_VIDEO_FPS;

use crate::command::{
    CliAction, CliCommand, CommandKind, HelpTopic, ImageCommand, ResolutionPreset,
    TimestampSelection, TranscriptCommand, VideoCommand,
};

pub(crate) fn parse_cli(mut args: Vec<OsString>) -> Result<CliAction> {
    let mut use_system_latex = false;
    loop {
        let Some(arg) = args.first() else {
            break;
        };
        if is_flag(arg, "--system-latex") {
            use_system_latex = true;
            args.remove(0);
        } else {
            break;
        }
    }

    let Some(command) = args.first() else {
        return Ok(CliAction::Help(HelpTopic::General));
    };

    match command.to_string_lossy().as_ref() {
        "help" => parse_help(&args[1..]),
        "-h" | "--help" => Ok(CliAction::Help(HelpTopic::General)),
        "image" => parse_image_command(&args[1..], use_system_latex),
        "video" => parse_video_command(&args[1..], use_system_latex),
        "transcript" => parse_transcript_command(&args[1..], use_system_latex),
        "__style_trusted_html" => parse_style_trusted_html_command(&args[1..]),
        other => bail!("unknown command `{other}`"),
    }
}

fn parse_help(args: &[OsString]) -> Result<CliAction> {
    match args {
        [] => Ok(CliAction::Help(HelpTopic::General)),
        [topic] => match topic.to_string_lossy().as_ref() {
            "image" => Ok(CliAction::Help(HelpTopic::Image)),
            "video" => Ok(CliAction::Help(HelpTopic::Video)),
            "transcript" => Ok(CliAction::Help(HelpTopic::Transcript)),
            other => bail!("unknown help topic `{other}`"),
        },
        _ => bail!("help accepts at most one topic"),
    }
}

fn parse_style_trusted_html_command(args: &[OsString]) -> Result<CliAction> {
    match args {
        [] => Ok(CliAction::Run(CliCommand::StyleTrustedHtml)),
        [arg, ..] if looks_like_flag(arg) => bail!(
            "unknown option `{}` for `__style_trusted_html`",
            arg.to_string_lossy()
        ),
        [arg, ..] => bail!("unexpected positional argument `{}`", arg.to_string_lossy()),
    }
}

fn parse_image_command(args: &[OsString], mut use_system_latex: bool) -> Result<CliAction> {
    let mut scene_path = None;
    let mut output_path = None;
    let mut resolution = ResolutionPreset::default();
    let mut slide = None;
    let mut time = None;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if is_help_flag(arg) {
            return Ok(CliAction::Help(HelpTopic::Image));
        }

        if is_flag(arg, "-o") || is_flag(arg, "--output") {
            output_path = Some(PathBuf::from(required_value(args, &mut index, arg)?));
        } else if is_flag(arg, "-r") || is_flag(arg, "--resolution") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            resolution = ResolutionPreset::parse(&value).ok_or_else(|| {
                anyhow!("invalid resolution `{value}`; expected small, medium, or large")
            })?;
        } else if is_flag(arg, "--slide") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            slide = Some(
                value
                    .parse()
                    .with_context(|| format!("invalid slide index `{value}`"))?,
            );
        } else if is_flag(arg, "--time") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            let parsed_time = value
                .parse()
                .with_context(|| format!("invalid time `{value}`"))?;
            if parsed_time < 0.0 {
                bail!("image time must be non-negative");
            }
            time = Some(parsed_time);
        } else if is_flag(arg, "--system-latex") {
            use_system_latex = true;
        } else if looks_like_flag(arg) {
            bail!("unknown option `{}` for `image`", arg.to_string_lossy());
        } else if scene_path.is_none() {
            scene_path = Some(PathBuf::from(arg));
        } else {
            bail!("unexpected positional argument `{}`", arg.to_string_lossy());
        }
        index += 1;
    }

    let scene_path = scene_path.ok_or_else(|| anyhow!("missing scene path for `image`"))?;
    let output_path = normalize_output_path(
        output_path.unwrap_or_else(|| default_output_path(&scene_path, CommandKind::Image)),
        CommandKind::Image,
    );
    let timestamp = TimestampSelection::from_parts(slide, time);

    Ok(CliAction::Run(CliCommand::Image(ImageCommand {
        scene_path,
        output_path,
        resolution,
        timestamp,
        use_system_latex,
    })))
}

fn parse_video_command(args: &[OsString], mut use_system_latex: bool) -> Result<CliAction> {
    let mut scene_path = None;
    let mut output_path = None;
    let mut resolution = ResolutionPreset::default();
    let mut fps = DEFAULT_VIDEO_FPS;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if is_help_flag(arg) {
            return Ok(CliAction::Help(HelpTopic::Video));
        }

        if is_flag(arg, "-o") || is_flag(arg, "--output") {
            output_path = Some(PathBuf::from(required_value(args, &mut index, arg)?));
        } else if is_flag(arg, "-r") || is_flag(arg, "--resolution") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            resolution = ResolutionPreset::parse(&value).ok_or_else(|| {
                anyhow!("invalid resolution `{value}`; expected small, medium, or large")
            })?;
        } else if is_flag(arg, "--fps") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            fps = value
                .parse()
                .with_context(|| format!("invalid fps `{value}`"))?;
            if fps == 0 {
                bail!("video fps must be greater than zero");
            }
        } else if is_flag(arg, "--system-latex") {
            use_system_latex = true;
        } else if looks_like_flag(arg) {
            bail!("unknown option `{}` for `video`", arg.to_string_lossy());
        } else if scene_path.is_none() {
            scene_path = Some(PathBuf::from(arg));
        } else {
            bail!("unexpected positional argument `{}`", arg.to_string_lossy());
        }
        index += 1;
    }

    let scene_path = scene_path.ok_or_else(|| anyhow!("missing scene path for `video`"))?;
    let output_path = normalize_output_path(
        output_path.unwrap_or_else(|| default_output_path(&scene_path, CommandKind::Video)),
        CommandKind::Video,
    );

    Ok(CliAction::Run(CliCommand::Video(VideoCommand {
        scene_path,
        output_path,
        resolution,
        fps,
        use_system_latex,
    })))
}

fn parse_transcript_command(args: &[OsString], mut use_system_latex: bool) -> Result<CliAction> {
    let mut scene_path = None;
    let mut slide = None;
    let mut time = None;
    let mut fast_seek = false;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if is_help_flag(arg) {
            return Ok(CliAction::Help(HelpTopic::Transcript));
        }

        if is_flag(arg, "--slide") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            slide = Some(
                value
                    .parse()
                    .with_context(|| format!("invalid slide index `{value}`"))?,
            );
        } else if is_flag(arg, "--time") {
            let value = string_value(required_value(args, &mut index, arg)?, arg)?;
            let parsed_time = value
                .parse()
                .with_context(|| format!("invalid time `{value}`"))?;
            if parsed_time < 0.0 {
                bail!("transcript time must be non-negative");
            }
            time = Some(parsed_time);
        } else if is_flag(arg, "--system-latex") {
            use_system_latex = true;
        } else if is_fast_seek_flag(arg) {
            fast_seek = true;
        } else if looks_like_flag(arg) {
            bail!(
                "unknown option `{}` for `transcript`",
                arg.to_string_lossy()
            );
        } else if scene_path.is_none() {
            scene_path = Some(PathBuf::from(arg));
        } else {
            bail!("unexpected positional argument `{}`", arg.to_string_lossy());
        }
        index += 1;
    }

    let scene_path = scene_path.ok_or_else(|| anyhow!("missing scene path for `transcript`"))?;
    Ok(CliAction::Run(CliCommand::Transcript(TranscriptCommand {
        scene_path,
        timestamp: TimestampSelection::from_parts(slide, time),
        use_system_latex,
        fast_seek,
    })))
}

fn required_value<'a>(args: &'a [OsString], index: &mut usize, flag: &OsStr) -> Result<&'a OsStr> {
    *index += 1;
    args.get(*index)
        .map(OsString::as_os_str)
        .ok_or_else(|| anyhow!("missing value for `{}`", flag.to_string_lossy()))
}

fn string_value(value: &OsStr, flag: &OsStr) -> Result<String> {
    value
        .to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("value for `{}` must be valid UTF-8", flag.to_string_lossy()))
}

fn is_flag(arg: &OsStr, expected: &str) -> bool {
    arg == OsStr::new(expected)
}

fn is_help_flag(arg: &OsStr) -> bool {
    is_flag(arg, "-h") || is_flag(arg, "--help")
}

fn is_fast_seek_flag(arg: &OsStr) -> bool {
    is_flag(arg, "--fast-seek") || is_flag(arg, "-fast-seek")
}

fn looks_like_flag(arg: &OsStr) -> bool {
    arg.to_string_lossy().starts_with('-')
}

fn default_output_path(scene_path: &Path, kind: CommandKind) -> PathBuf {
    let mut output = scene_path.to_path_buf();
    if let Some(extension) = kind.extension() {
        output.set_extension(extension);
    }
    output
}

fn normalize_output_path(mut path: PathBuf, kind: CommandKind) -> PathBuf {
    if let Some(extension) = kind.extension() {
        path.set_extension(extension);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use executor::time::Timestamp;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_default_image_command() {
        let parsed = parse_cli(args(&["image", "scene.mcs"])).unwrap();
        let CliAction::Run(CliCommand::Image(command)) = parsed else {
            panic!("expected image command");
        };

        assert_eq!(command.scene_path, PathBuf::from("scene.mcs"));
        assert_eq!(command.output_path, PathBuf::from("scene.png"));
        assert_eq!(command.resolution, ResolutionPreset::Medium);
        assert_eq!(command.timestamp, TimestampSelection::SceneEnd);
        assert!(!command.use_system_latex);
    }

    #[test]
    fn parses_explicit_image_timestamp() {
        let parsed = parse_cli(args(&[
            "image",
            "scene.mcs",
            "--slide",
            "2",
            "--time",
            "1.25",
        ]))
        .unwrap();
        let CliAction::Run(CliCommand::Image(command)) = parsed else {
            panic!("expected image command");
        };

        assert_eq!(
            command.timestamp,
            TimestampSelection::Exact(Timestamp::new(3, 1.25))
        );
    }

    #[test]
    fn parses_video_command_with_options() {
        let parsed = parse_cli(args(&[
            "video",
            "scene.mcs",
            "--resolution",
            "large",
            "--fps",
            "30",
            "--output",
            "renders/out.mov",
        ]))
        .unwrap();

        let CliAction::Run(CliCommand::Video(command)) = parsed else {
            panic!("expected video command");
        };

        assert_eq!(command.scene_path, PathBuf::from("scene.mcs"));
        assert_eq!(command.output_path, PathBuf::from("renders/out.mp4"));
        assert_eq!(command.resolution, ResolutionPreset::Large);
        assert_eq!(command.fps, 30);
        assert!(!command.use_system_latex);
    }

    #[test]
    fn parses_system_latex_option() {
        let parsed = parse_cli(args(&["--system-latex", "image", "scene.mcs"])).unwrap();
        let CliAction::Run(CliCommand::Image(command)) = parsed else {
            panic!("expected image command");
        };
        assert!(command.use_system_latex);

        let parsed = parse_cli(args(&["video", "scene.mcs", "--system-latex"])).unwrap();
        let CliAction::Run(CliCommand::Video(command)) = parsed else {
            panic!("expected video command");
        };
        assert!(command.use_system_latex);

        let parsed = parse_cli(args(&["transcript", "scene.mcs", "--system-latex"])).unwrap();
        let CliAction::Run(CliCommand::Transcript(command)) = parsed else {
            panic!("expected transcript command");
        };
        assert!(command.use_system_latex);
        assert!(!command.fast_seek);
    }

    #[test]
    fn parses_transcript_fast_seek_option() {
        let parsed = parse_cli(args(&["transcript", "scene.mcs", "-fast-seek"])).unwrap();
        let CliAction::Run(CliCommand::Transcript(command)) = parsed else {
            panic!("expected transcript command");
        };
        assert!(command.fast_seek);
    }

    #[test]
    fn rejects_fast_seek_for_export_commands() {
        let error = parse_cli(args(&["image", "scene.mcs", "--fast-seek"])).unwrap_err();
        assert!(error.to_string().contains("unknown option"));

        let error = parse_cli(args(&["video", "scene.mcs", "-fast-seek"])).unwrap_err();
        assert!(error.to_string().contains("unknown option"));
    }

    #[test]
    fn parses_transcript_command_with_timestamp() {
        let parsed = parse_cli(args(&[
            "transcript",
            "scene.mcs",
            "--slide",
            "1",
            "--time",
            "0.75",
        ]))
        .unwrap();
        let CliAction::Run(CliCommand::Transcript(command)) = parsed else {
            panic!("expected transcript command");
        };

        assert_eq!(command.scene_path, PathBuf::from("scene.mcs"));
        assert_eq!(
            command.timestamp,
            TimestampSelection::Exact(Timestamp::new(2, 0.75))
        );
    }

    #[test]
    fn parses_help_topics() {
        assert_eq!(
            parse_cli(args(&["help", "image"])).unwrap(),
            CliAction::Help(HelpTopic::Image)
        );
        assert_eq!(
            parse_cli(args(&["--help"])).unwrap(),
            CliAction::Help(HelpTopic::General)
        );
        assert_eq!(
            parse_cli(args(&["help", "transcript"])).unwrap(),
            CliAction::Help(HelpTopic::Transcript)
        );
    }

    #[test]
    fn parses_internal_style_trusted_html_command() {
        assert_eq!(
            parse_cli(args(&["__style_trusted_html"])).unwrap(),
            CliAction::Run(CliCommand::StyleTrustedHtml)
        );

        let error = parse_cli(args(&["help", "__style_trusted_html"])).unwrap_err();
        assert!(error.to_string().contains("unknown help topic"));
    }

    #[test]
    fn rejects_unknown_option() {
        let error = parse_cli(args(&["image", "scene.mcs", "--wat"])).unwrap_err();
        assert!(error.to_string().contains("unknown option"));
    }
}
