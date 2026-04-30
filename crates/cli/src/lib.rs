use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, Result, anyhow, bail};
use executor::time::Timestamp;
use exporter::{
    DEFAULT_EXPORT_SIZE, DEFAULT_VIDEO_FPS, EXPORT_CANCELLED_MESSAGE, ExportKind, ExportOutcome,
    ExportProgress, ExportRequest, ExportSettings, ImageExportTimestamp, SceneInspectionOutcome,
    SceneInspectionRequest, SceneInspectionTimestamp, export_scene, inspect_scene,
};
use renderer::RenderSize;

const PROGRESS_BAR_WIDTH: usize = 28;

pub fn run(args: Vec<OsString>) -> i32 {
    clean_latex_file_cache();

    match parse_cli(args) {
        Ok(CliAction::Help(topic)) => {
            println!("{}", help_text(topic));
            0
        }
        Ok(CliAction::Run(command)) => {
            if let Err(error) = run_command(command) {
                eprintln!("error: {error:#}");
                1
            } else {
                0
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!();
            eprintln!("Use `monocurl help` for usage.");
            2
        }
    }
}

fn clean_latex_file_cache() {
    if let Err(error) = latex::clean_stale_file_cache() {
        log::warn!("unable to clean stale LaTeX SVG cache: {error:#}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandKind {
    Image,
    Video,
    Transcript,
}

impl CommandKind {
    fn progress_title(self) -> &'static str {
        match self {
            Self::Image => "Exporting Image",
            Self::Video => "Exporting Video",
            Self::Transcript => "Inspecting Scene",
        }
    }

    fn success_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Complete",
            Self::Video => "Video Export Complete",
            Self::Transcript => "Transcript Ready",
        }
    }

    fn failure_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Failed",
            Self::Video => "Video Export Failed",
            Self::Transcript => "Transcript Failed",
        }
    }

    fn canceled_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Canceled",
            Self::Video => "Video Export Canceled",
            Self::Transcript => "Transcript Canceled",
        }
    }

    fn extension(self) -> Option<&'static str> {
        match self {
            Self::Image => Some("png"),
            Self::Video => Some("mp4"),
            Self::Transcript => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpTopic {
    General,
    Image,
    Video,
    Transcript,
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
    Transcript(TranscriptCommand),
}

impl CliCommand {
    fn use_system_latex(&self) -> bool {
        match self {
            Self::Image(command) => command.use_system_latex,
            Self::Video(command) => command.use_system_latex,
            Self::Transcript(command) => command.use_system_latex,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ImageCommand {
    scene_path: PathBuf,
    output_path: PathBuf,
    resolution: ResolutionPreset,
    timestamp: TimestampSelection,
    use_system_latex: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct VideoCommand {
    scene_path: PathBuf,
    output_path: PathBuf,
    resolution: ResolutionPreset,
    fps: u32,
    use_system_latex: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct TranscriptCommand {
    scene_path: PathBuf,
    timestamp: TimestampSelection,
    use_system_latex: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TimestampSelection {
    SceneEnd,
    Exact(Timestamp),
}

impl TimestampSelection {
    fn from_parts(slide: Option<usize>, time: Option<f64>) -> Self {
        match (slide, time) {
            (None, None) => Self::SceneEnd,
            (slide, time) => {
                Self::Exact(Timestamp::new(slide.unwrap_or(0) + 1, time.unwrap_or(0.0)))
            }
        }
    }

    fn image_export_timestamp(self) -> ImageExportTimestamp {
        match self {
            Self::SceneEnd => ImageExportTimestamp::SceneEnd,
            Self::Exact(timestamp) => ImageExportTimestamp::Exact(timestamp),
        }
    }

    fn scene_inspection_timestamp(self) -> SceneInspectionTimestamp {
        match self {
            Self::SceneEnd => SceneInspectionTimestamp::SceneEnd,
            Self::Exact(timestamp) => SceneInspectionTimestamp::Exact(timestamp),
        }
    }
}

fn parse_cli(mut args: Vec<OsString>) -> Result<CliAction> {
    let mut use_system_latex = false;
    while args
        .first()
        .is_some_and(|arg| is_flag(arg, "--system-latex"))
    {
        use_system_latex = true;
        args.remove(0);
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

fn run_command(command: CliCommand) -> Result<()> {
    configure_latex_backend(command.use_system_latex())?;

    match command {
        CliCommand::Image(command) => run_export_command(
            CommandKind::Image,
            command.scene_path,
            command.output_path,
            ExportKind::Image {
                timestamp: command.timestamp.image_export_timestamp(),
            },
            ExportSettings {
                render_size: command.resolution.render_size(),
                fps: DEFAULT_VIDEO_FPS,
            },
        ),
        CliCommand::Video(command) => run_export_command(
            CommandKind::Video,
            command.scene_path,
            command.output_path,
            ExportKind::Video,
            ExportSettings {
                render_size: command.resolution.render_size(),
                fps: command.fps,
            },
        ),
        CliCommand::Transcript(command) => run_transcript_command(command),
    }
}

fn run_export_command(
    kind: CommandKind,
    scene_path: PathBuf,
    output_path: PathBuf,
    export_kind: ExportKind,
    settings: ExportSettings,
) -> Result<()> {
    let root_text = fs::read_to_string(&scene_path)
        .with_context(|| format!("failed to read scene {}", scene_path.display()))?;
    let request = ExportRequest {
        root_text,
        root_path: scene_path,
        open_documents: HashMap::new(),
        output_path,
        kind: export_kind,
        settings,
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let mut progress = TerminalProgress::new(kind);
    let result = export_scene(request, cancel_flag, |update| {
        let _ = progress.update(&update);
    });

    match result {
        Ok(outcome) => {
            progress.finish_export(kind, &outcome)?;
            Ok(())
        }
        Err(error) => {
            progress.fail(kind, error.to_string() == EXPORT_CANCELLED_MESSAGE)?;
            Err(error)
        }
    }
}

fn run_transcript_command(command: TranscriptCommand) -> Result<()> {
    let scene_path = command.scene_path;
    let root_text = fs::read_to_string(&scene_path)
        .with_context(|| format!("failed to read scene {}", scene_path.display()))?;
    let request = SceneInspectionRequest {
        root_text,
        root_path: scene_path,
        open_documents: HashMap::new(),
        timestamp: command.timestamp.scene_inspection_timestamp(),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let mut progress = TerminalProgress::new(CommandKind::Transcript);
    let result = inspect_scene(request, cancel_flag, |update| {
        let _ = progress.update(&update);
    });

    match result {
        Ok(outcome) => {
            progress.finish_transcript(&outcome)?;
            print_transcript(&outcome)?;
            Ok(())
        }
        Err(error) => {
            progress.fail(
                CommandKind::Transcript,
                error.to_string() == EXPORT_CANCELLED_MESSAGE,
            )?;
            Err(error)
        }
    }
}

fn configure_latex_backend(use_system_latex: bool) -> Result<()> {
    if !use_system_latex {
        latex::set_backend_config(latex::LatexBackendConfig::Bundled);
        return Ok(());
    }

    let tools = latex::discover_system_backend();
    let config = tools.into_config().ok_or_else(|| {
        anyhow!("--system-latex requires both `latex` and `dvisvgm` to be available on PATH")
    })?;
    latex::set_backend_config(latex::LatexBackendConfig::System(config));
    Ok(())
}

fn print_transcript(outcome: &SceneInspectionOutcome) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    for entry in &outcome.transcript {
        writeln!(stdout, "{}", entry.text())?;
    }
    Ok(())
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

    fn finish_export(&mut self, kind: CommandKind, outcome: &ExportOutcome) -> io::Result<()> {
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

    fn finish_transcript(&mut self, outcome: &SceneInspectionOutcome) -> io::Result<()> {
        self.clear_line()?;
        eprintln!("{}", CommandKind::Transcript.success_title());
        eprintln!(
            "Reached slide {} at {}",
            outcome.timestamp.slide.saturating_sub(1),
            format_time(outcome.timestamp.time)
        );
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
  monocurl
  monocurl help [image|video|transcript]
  monocurl image <scene path> [options]
  monocurl video <scene path> [options]
  monocurl transcript <scene path> [options]

Running `monocurl` with no arguments launches the desktop app. Running the same
binary with arguments uses the CLI.

Commands:
  help                         show this message or subcommand help
  image                        export a still frame as PNG
  video                        export a full scene as MP4
  transcript                   seek a scene and print `print` output to stdout

Global options:
  --system-latex               use latex and dvisvgm from PATH instead of bundled Tectonic
  -h, --help                   show command help

Export options:
  -o, --output <path>          output path; extension is forced to .png or .mp4
  -r, --resolution <preset>    one of: {small}, {medium}, {large}

Image options:
  --slide <index>              zero-based slide to capture; if timestamp flags are used, missing values default to 0
  --time <seconds>             time within the slide; if neither timestamp flag is used, exports the final frame

Video options:
  --fps <number>               frames per second, default {fps}

Transcript options:
  --slide <index>              zero-based slide to seek; missing timestamp values default to 0
  --time <seconds>             time within the slide; if neither timestamp flag is used, seeks scene end

Examples:
  monocurl image lesson.mcs
  monocurl image lesson.mcs --slide 2 --time 1.25 --resolution large
  monocurl video lesson.mcs --resolution medium --fps 30
  monocurl transcript lesson.mcs --slide 0 --time 0.5
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
  --slide <index>              zero-based slide to capture; if timestamp flags are used, missing values default to 0
  --time <seconds>             time within the slide; if neither timestamp flag is used, exports the final frame
  --system-latex               use latex and dvisvgm from PATH instead of bundled Tectonic
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
  --system-latex               use latex and dvisvgm from PATH instead of bundled Tectonic
  -h, --help                   show this message
",
            small = resolution_help(ResolutionPreset::Small),
            medium = resolution_help(ResolutionPreset::Medium),
            large = resolution_help(ResolutionPreset::Large),
            fps = DEFAULT_VIDEO_FPS,
        ),
        HelpTopic::Transcript => "\
Usage:
  monocurl transcript <scene path> [options]

Options:
  --slide <index>              zero-based slide to seek; missing timestamp values default to 0
  --time <seconds>             time within the slide; if neither timestamp flag is used, seeks scene end
  --system-latex               use latex and dvisvgm from PATH instead of bundled Tectonic
  -h, --help                   show this message
"
        .into(),
    }
}

fn resolution_help(preset: ResolutionPreset) -> String {
    let size = preset.render_size();
    format!("{} ({}x{})", preset.as_str(), size.width, size.height)
}

fn format_time(time: f64) -> String {
    if time.is_infinite() {
        "end".into()
    } else {
        format!("{time:.3}s")
    }
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
    fn rejects_unknown_option() {
        let error = parse_cli(args(&["image", "scene.mcs", "--wat"])).unwrap_err();
        assert!(error.to_string().contains("unknown option"));
    }
}
