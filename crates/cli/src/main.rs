use std::{
    collections::HashMap,
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, Result, anyhow, bail};
use executor::time::Timestamp;
use exporter::{
    DEFAULT_EXPORT_SIZE, DEFAULT_VIDEO_FPS, EXPORT_CANCELLED_MESSAGE, ExportKind, ExportOutcome,
    ExportProgress, ExportRequest, ExportSettings, ImageExportTimestamp, export_scene,
};
use renderer::RenderSize;

const PROGRESS_BAR_WIDTH: usize = 28;

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    warn_if_system_latex_unavailable();

    match parse_cli(env::args_os().skip(1).collect()) {
        Ok(CliAction::Help(topic)) => {
            println!("{}", help_text(topic));
        }
        Ok(CliAction::Run(command)) => {
            if let Err(error) = run_command(command) {
                eprintln!("error: {error:#}");
                process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!();
            eprintln!("Use `monocurl help` for usage.");
            process::exit(2);
        }
    }
}

fn warn_if_system_latex_unavailable() {
    let status = latex::system_backend_status();
    if status.is_available() {
        return;
    }

    eprintln!("warning: system LaTeX tools not found");
    eprintln!(
        "Missing on PATH: {}. Monocurl will use a limited MathJax fallback for Tex(...); Text(...) and Latex(...) still require the system LaTeX toolchain.",
        missing_latex_tools(status),
    );
    eprintln!("Install LaTeX: {}", latex_install_url());
    eprintln!();
}

fn latex_install_url() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "https://tug.org/mactex/"
    }

    #[cfg(target_os = "windows")]
    {
        "https://miktex.org/download"
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        "https://www.latex-project.org/get/"
    }
}

fn missing_latex_tools(status: latex::SystemBackendStatus) -> &'static str {
    match (status.latex, status.dvisvgm) {
        (true, true) => "",
        (false, true) => "latex",
        (true, false) => "dvisvgm",
        (false, false) => "latex and dvisvgm",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandKind {
    Image,
    Video,
}

impl CommandKind {
    fn progress_title(self) -> &'static str {
        match self {
            Self::Image => "Exporting Image",
            Self::Video => "Exporting Video",
        }
    }

    fn success_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Complete",
            Self::Video => "Video Export Complete",
        }
    }

    fn failure_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Failed",
            Self::Video => "Video Export Failed",
        }
    }

    fn canceled_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Canceled",
            Self::Video => "Video Export Canceled",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Image => "png",
            Self::Video => "mp4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpTopic {
    General,
    Image,
    Video,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ResolutionPreset {
    Small,
    #[default]
    Medium,
    Large,
}

impl ResolutionPreset {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    fn render_size(self) -> RenderSize {
        match self {
            Self::Small => RenderSize::new(1280, 720),
            Self::Medium => DEFAULT_EXPORT_SIZE,
            Self::Large => RenderSize::new(3840, 2160),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum CliAction {
    Help(HelpTopic),
    Run(CliCommand),
}

#[derive(Clone, Debug, PartialEq)]
enum CliCommand {
    Image(ImageCommand),
    Video(VideoCommand),
}

impl CliCommand {
    fn kind(&self) -> CommandKind {
        match self {
            Self::Image(_) => CommandKind::Image,
            Self::Video(_) => CommandKind::Video,
        }
    }

    fn scene_path(&self) -> &Path {
        match self {
            Self::Image(command) => &command.scene_path,
            Self::Video(command) => &command.scene_path,
        }
    }

    fn output_path(&self) -> &Path {
        match self {
            Self::Image(command) => &command.output_path,
            Self::Video(command) => &command.output_path,
        }
    }

    fn export_kind(&self) -> ExportKind {
        match self {
            Self::Image(command) => ExportKind::Image {
                timestamp: match command.timestamp {
                    ImageTimestampSelection::SceneEnd => ImageExportTimestamp::SceneEnd,
                    ImageTimestampSelection::Exact(timestamp) => {
                        ImageExportTimestamp::Exact(timestamp)
                    }
                },
            },
            Self::Video(_) => ExportKind::Video,
        }
    }

    fn export_settings(&self) -> ExportSettings {
        match self {
            Self::Image(command) => ExportSettings {
                render_size: command.resolution.render_size(),
                fps: DEFAULT_VIDEO_FPS,
            },
            Self::Video(command) => ExportSettings {
                render_size: command.resolution.render_size(),
                fps: command.fps,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ImageCommand {
    scene_path: PathBuf,
    output_path: PathBuf,
    resolution: ResolutionPreset,
    timestamp: ImageTimestampSelection,
}

#[derive(Clone, Debug, PartialEq)]
struct VideoCommand {
    scene_path: PathBuf,
    output_path: PathBuf,
    resolution: ResolutionPreset,
    fps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ImageTimestampSelection {
    SceneEnd,
    Exact(Timestamp),
}

fn parse_cli(args: Vec<OsString>) -> Result<CliAction> {
    let Some(command) = args.first() else {
        return Ok(CliAction::Help(HelpTopic::General));
    };

    match command.to_string_lossy().as_ref() {
        "help" => parse_help(&args[1..]),
        "-h" | "--help" => Ok(CliAction::Help(HelpTopic::General)),
        "image" => parse_image_command(&args[1..]),
        "video" => parse_video_command(&args[1..]),
        other => bail!("unknown command `{other}`"),
    }
}

fn parse_help(args: &[OsString]) -> Result<CliAction> {
    match args {
        [] => Ok(CliAction::Help(HelpTopic::General)),
        [topic] => match topic.to_string_lossy().as_ref() {
            "image" => Ok(CliAction::Help(HelpTopic::Image)),
            "video" => Ok(CliAction::Help(HelpTopic::Video)),
            other => bail!("unknown help topic `{other}`"),
        },
        _ => bail!("help accepts at most one topic"),
    }
}

fn parse_image_command(args: &[OsString]) -> Result<CliAction> {
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
    let timestamp = match (slide, time) {
        (None, None) => ImageTimestampSelection::SceneEnd,
        (slide, time) => {
            ImageTimestampSelection::Exact(Timestamp::new(slide.unwrap_or(0), time.unwrap_or(0.0)))
        }
    };

    Ok(CliAction::Run(CliCommand::Image(ImageCommand {
        scene_path,
        output_path,
        resolution,
        timestamp,
    })))
}

fn parse_video_command(args: &[OsString]) -> Result<CliAction> {
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

fn looks_like_flag(arg: &OsStr) -> bool {
    arg.to_string_lossy().starts_with('-')
}

fn default_output_path(scene_path: &Path, kind: CommandKind) -> PathBuf {
    let mut output = scene_path.to_path_buf();
    output.set_extension(kind.extension());
    output
}

fn normalize_output_path(mut path: PathBuf, kind: CommandKind) -> PathBuf {
    path.set_extension(kind.extension());
    path
}

fn run_command(command: CliCommand) -> Result<()> {
    let scene_path = command.scene_path().to_path_buf();
    let root_text = fs::read_to_string(&scene_path)
        .with_context(|| format!("failed to read scene {}", scene_path.display()))?;
    let request = ExportRequest {
        root_text,
        root_path: scene_path,
        open_documents: HashMap::new(),
        output_path: command.output_path().to_path_buf(),
        kind: command.export_kind(),
        settings: command.export_settings(),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let mut progress = TerminalProgress::new(command.kind());
    let result = export_scene(request, cancel_flag, |update| {
        let _ = progress.update(&update);
    });

    match result {
        Ok(outcome) => {
            progress.finish(command.kind(), &outcome)?;
            Ok(())
        }
        Err(error) => {
            progress.fail(
                command.kind(),
                error.to_string() == EXPORT_CANCELLED_MESSAGE,
            )?;
            Err(error)
        }
    }
}

struct TerminalProgress {
    title: &'static str,
    last_line_len: usize,
}

impl TerminalProgress {
    fn new(kind: CommandKind) -> Self {
        eprintln!("{}", kind.progress_title());
        Self {
            title: kind.progress_title(),
            last_line_len: 0,
        }
    }

    fn update(&mut self, progress: &ExportProgress) -> io::Result<()> {
        let ratio = if progress.total == 0 {
            0.35
        } else {
            progress.ratio()
        };
        let filled = (ratio * PROGRESS_BAR_WIDTH as f32).round() as usize;
        let filled = filled.min(PROGRESS_BAR_WIDTH);
        let bar = format!(
            "[{}{}]",
            "#".repeat(filled),
            "-".repeat(PROGRESS_BAR_WIDTH - filled)
        );

        let mut line = format!("{bar} {}", progress.message);
        if progress.total > 0 {
            line.push_str(&format!(" ({}/{})", progress.completed, progress.total));
        }

        let padding = self.last_line_len.saturating_sub(line.len());
        let mut stderr = io::stderr().lock();
        write!(stderr, "\r{line}{:padding$}", "", padding = padding)?;
        stderr.flush()?;
        self.last_line_len = line.len();
        Ok(())
    }

    fn finish(&mut self, kind: CommandKind, outcome: &ExportOutcome) -> io::Result<()> {
        self.clear_line()?;
        eprintln!("{}", kind.success_title());
        eprintln!("Saved to {}", outcome.output_path.display());
        if matches!(kind, CommandKind::Video) {
            eprintln!("Frames written: {}", outcome.frames_written);
        }
        if !outcome.transcript.is_empty() {
            eprintln!();
            eprintln!("Print output:");
            for entry in &outcome.transcript {
                eprintln!("  {}", entry.text());
            }
        }
        Ok(())
    }

    fn fail(&mut self, kind: CommandKind, cancelled: bool) -> io::Result<()> {
        self.clear_line()?;
        if cancelled {
            eprintln!("{}", kind.canceled_title());
        } else {
            eprintln!("{}", kind.failure_title());
        }
        Ok(())
    }

    fn clear_line(&mut self) -> io::Result<()> {
        if self.last_line_len == 0 {
            return Ok(());
        }

        let mut stderr = io::stderr().lock();
        write!(
            stderr,
            "\r{:width$}\r",
            "",
            width = self.last_line_len.max(self.title.len())
        )?;
        stderr.flush()?;
        self.last_line_len = 0;
        Ok(())
    }
}

fn help_text(topic: HelpTopic) -> String {
    match topic {
        HelpTopic::General => format!(
            "\
Monocurl CLI

Usage:
  monocurl help [image|video]
  monocurl image <scene path> [options]
  monocurl video <scene path> [options]

Commands:
  help                         show this message or subcommand help
  image                        export a still frame as PNG
  video                        export a full scene as MP4

Common options:
  -o, --output <path>          output path; extension is forced to .png or .mp4
  -r, --resolution <preset>    one of: {small}, {medium}, {large}
  -h, --help                   show command help

Image options:
  --slide <index>              slide to capture; if timestamp flags are used, missing values default to 0
  --time <seconds>             time within the slide; if neither timestamp flag is used, exports the final frame

Video options:
  --fps <number>               frames per second, default {fps}

Examples:
  monocurl image lesson.mcs
  monocurl image lesson.mcs --slide 2 --time 1.25 --resolution large
  monocurl video lesson.mcs --resolution medium --fps 30
",
            small = resolution_help(ResolutionPreset::Small),
            medium = resolution_help(ResolutionPreset::Medium),
            large = resolution_help(ResolutionPreset::Large),
            fps = DEFAULT_VIDEO_FPS,
        ),
        HelpTopic::Image => format!(
            "\
Usage:
  monocurl image <scene path> [options]

Options:
  -o, --output <path>          output path; extension is forced to .png
  -r, --resolution <preset>    one of: {small}, {medium}, {large}
  --slide <index>              slide to capture; if timestamp flags are used, missing values default to 0
  --time <seconds>             time within the slide; if neither timestamp flag is used, exports the final frame
  -h, --help                   show this message
",
            small = resolution_help(ResolutionPreset::Small),
            medium = resolution_help(ResolutionPreset::Medium),
            large = resolution_help(ResolutionPreset::Large),
        ),
        HelpTopic::Video => format!(
            "\
Usage:
  monocurl video <scene path> [options]

Options:
  -o, --output <path>          output path; extension is forced to .mp4
  -r, --resolution <preset>    one of: {small}, {medium}, {large}
  --fps <number>               frames per second, default {fps}
  -h, --help                   show this message
",
            small = resolution_help(ResolutionPreset::Small),
            medium = resolution_help(ResolutionPreset::Medium),
            large = resolution_help(ResolutionPreset::Large),
            fps = DEFAULT_VIDEO_FPS,
        ),
    }
}

fn resolution_help(preset: ResolutionPreset) -> String {
    let size = preset.render_size();
    format!("{} ({}x{})", preset.as_str(), size.width, size.height)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(command.timestamp, ImageTimestampSelection::SceneEnd);
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
            ImageTimestampSelection::Exact(Timestamp::new(2, 1.25))
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
    }

    #[test]
    fn rejects_unknown_option() {
        let error = parse_cli(args(&["image", "scene.mcs", "--wat"])).unwrap_err();
        assert!(error.to_string().contains("unknown option"));
    }
}
