use exporter::DEFAULT_VIDEO_FPS;

use crate::command::{HelpTopic, ResolutionPreset};

pub(crate) fn help_text(topic: HelpTopic, version: &str) -> String {
    match topic {
        HelpTopic::General => format!(
            "\
Monocurl v{version}

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
  --slides <A..B>              render only zero-based slides A to B-1 (also A.. or A)
  --start-time <seconds>       start this far into the first slide of the range
  --end-time <seconds>         stop before this time in the last slide of the range
  --list-slides                print `index<TAB>frames<TAB>seconds<TAB>name` per slide and exit

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
  --slides <A..B>              render only zero-based slides A to B-1 (also A.. or A)
  --start-time <seconds>       start this far into the first slide of the range
  --end-time <seconds>         stop before this time in the last slide of the range
  --list-slides                print `index<TAB>frames<TAB>seconds<TAB>name` per slide and exit
  --system-latex               use latex and dvisvgm from PATH instead of bundled Tectonic
  -h, --help                   show this message

Ranges keep the full export's frame grid: each slide starts on a frame and its frame
times are multiples of 1/fps, with range times rounded up onto that grid. Rendering
adjacent ranges and joining them (e.g. `ffmpeg -f concat -c copy`) gives the same
frames as one full export, so long scenes can be rendered in parallel processes.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_help_includes_the_application_version() {
        assert!(help_text(HelpTopic::General, "0.3.2").starts_with("Monocurl v0.3.2"));
    }
}

fn resolution_help(preset: ResolutionPreset) -> String {
    let size = preset.render_size();
    format!("{} ({}x{})", preset.as_str(), size.width, size.height)
}
