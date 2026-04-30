use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, Result, anyhow};
use executor::executor::SeekOptions;
use exporter::{
    DEFAULT_VIDEO_FPS, EXPORT_CANCELLED_MESSAGE, ExportKind, ExportRequest, ExportSettings,
    SceneInspectionOutcome, SceneInspectionRequest, export_scene, inspect_scene_with_seek_options,
};

use crate::{
    command::{CliCommand, CommandKind, TranscriptCommand},
    progress::TerminalProgress,
    style_trusted_html,
};

pub(crate) fn run_command(command: CliCommand) -> Result<()> {
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
        CliCommand::StyleTrustedHtml => style_trusted_html::run_command(),
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
    let seek_options = SeekOptions {
        fast_seek: command.fast_seek,
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let mut progress = TerminalProgress::new(CommandKind::Transcript);
    let result = inspect_scene_with_seek_options(request, seek_options, cancel_flag, |update| {
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
