use std::path::PathBuf;

use executor::time::Timestamp;
use exporter::{DEFAULT_EXPORT_SIZE, ImageExportTimestamp, SceneInspectionTimestamp};
use renderer::RenderSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandKind {
    Image,
    Video,
    Transcript,
}

impl CommandKind {
    pub(crate) fn progress_title(self) -> &'static str {
        match self {
            Self::Image => "Exporting Image",
            Self::Video => "Exporting Video",
            Self::Transcript => "Inspecting Scene",
        }
    }

    pub(crate) fn success_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Complete",
            Self::Video => "Video Export Complete",
            Self::Transcript => "Transcript Ready",
        }
    }

    pub(crate) fn failure_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Failed",
            Self::Video => "Video Export Failed",
            Self::Transcript => "Transcript Failed",
        }
    }

    pub(crate) fn canceled_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Canceled",
            Self::Video => "Video Export Canceled",
            Self::Transcript => "Transcript Canceled",
        }
    }

    pub(crate) fn extension(self) -> Option<&'static str> {
        match self {
            Self::Image => Some("png"),
            Self::Video => Some("mp4"),
            Self::Transcript => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HelpTopic {
    General,
    Image,
    Video,
    Transcript,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ResolutionPreset {
    Small,
    #[default]
    Medium,
    Large,
}

impl ResolutionPreset {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    pub(crate) fn render_size(self) -> RenderSize {
        match self {
            Self::Small => RenderSize::new(1280, 720),
            Self::Medium => DEFAULT_EXPORT_SIZE,
            Self::Large => RenderSize::new(3840, 2160),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CliAction {
    Help(HelpTopic),
    Run(CliCommand),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CliCommand {
    Image(ImageCommand),
    Video(VideoCommand),
    Transcript(TranscriptCommand),
    StyleTrustedHtml,
}

impl CliCommand {
    pub(crate) fn use_system_latex(&self) -> bool {
        match self {
            Self::Image(command) => command.use_system_latex,
            Self::Video(command) => command.use_system_latex,
            Self::Transcript(command) => command.use_system_latex,
            Self::StyleTrustedHtml => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageCommand {
    pub(crate) scene_path: PathBuf,
    pub(crate) output_path: PathBuf,
    pub(crate) resolution: ResolutionPreset,
    pub(crate) timestamp: TimestampSelection,
    pub(crate) use_system_latex: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VideoCommand {
    pub(crate) scene_path: PathBuf,
    pub(crate) output_path: PathBuf,
    pub(crate) resolution: ResolutionPreset,
    pub(crate) fps: u32,
    pub(crate) use_system_latex: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptCommand {
    pub(crate) scene_path: PathBuf,
    pub(crate) timestamp: TimestampSelection,
    pub(crate) use_system_latex: bool,
    pub(crate) fast_seek: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TimestampSelection {
    SceneEnd,
    Exact(Timestamp),
}

impl TimestampSelection {
    pub(crate) fn from_parts(slide: Option<usize>, time: Option<f64>) -> Self {
        match (slide, time) {
            (None, None) => Self::SceneEnd,
            (slide, time) => {
                Self::Exact(Timestamp::new(slide.unwrap_or(0) + 1, time.unwrap_or(0.0)))
            }
        }
    }

    pub(crate) fn image_export_timestamp(self) -> ImageExportTimestamp {
        match self {
            Self::SceneEnd => ImageExportTimestamp::SceneEnd,
            Self::Exact(timestamp) => ImageExportTimestamp::Exact(timestamp),
        }
    }

    pub(crate) fn scene_inspection_timestamp(self) -> SceneInspectionTimestamp {
        match self {
            Self::SceneEnd => SceneInspectionTimestamp::SceneEnd,
            Self::Exact(timestamp) => SceneInspectionTimestamp::Exact(timestamp),
        }
    }
}
