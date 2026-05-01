use exporter::DEFAULT_VIDEO_FPS;

use crate::command::{HelpTopic, ResolutionPreset};

pub(crate) fn help_text(topic: HelpTopic) -> String {
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
  --fast-seek, -fast-seek      skip strict transcript seek validation

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
  --fast-seek, -fast-seek      skip strict transcript seek validation
  -h, --help                   show this message
"
        .into(),
    }
}

fn resolution_help(preset: ResolutionPreset) -> String {
    let size = preset.render_size();
    format!("{} ({}x{})", preset.as_str(), size.width, size.height)
}
