//! headless benchmark driver for the Monocurl pipeline.
//!
//! usage: mcbench [--iterations N] [--warmup N] [--strict] [--transcript] [scene.mcs ...]
//!
//! with no scene arguments the shared corpus under crates/scene_harness/scenes is used.

use std::{path::PathBuf, time::Duration};

use executor::executor::SeekOptions;
use scene_harness::{
    SceneTimings, bench_scenes, corpus_scenes, measure_edit_cycle, measure_playback,
    run_scene_file, use_repo_assets,
};

struct Args {
    iterations: usize,
    warmup: usize,
    options: SeekOptions,
    print_transcript: bool,
    playback: bool,
    edit: bool,
    fps: u32,
    scenes: Vec<PathBuf>,
}

fn parse_args() -> Args {
    let mut iterations = 3;
    let mut warmup = 1;
    let mut options = SeekOptions::fast();
    let mut print_transcript = false;
    let mut playback = false;
    let mut edit = false;
    let mut fps = 60;
    let mut scenes = Vec::new();

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--iterations" | "-n" => {
                iterations = argv
                    .next()
                    .and_then(|value| value.parse().ok())
                    .expect("--iterations expects a number")
            }
            "--warmup" => {
                warmup = argv
                    .next()
                    .and_then(|value| value.parse().ok())
                    .expect("--warmup expects a number")
            }
            "--strict" => options = SeekOptions::strict(),
            "--corpus" => scenes.extend(corpus_scenes()),
            "--transcript" => print_transcript = true,
            "--playback" => playback = true,
            "--edit" => edit = true,
            "--fps" => {
                fps = argv
                    .next()
                    .and_then(|value| value.parse().ok())
                    .expect("--fps expects a number")
            }
            other => scenes.push(PathBuf::from(other)),
        }
    }

    if scenes.is_empty() {
        scenes = bench_scenes();
    }

    Args {
        iterations,
        warmup,
        options,
        print_transcript,
        playback,
        edit,
        fps,
        scenes,
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

fn run_edit_cycle(args: &Args) {
    println!(
        "{:<34} {:>10} {:>12} {:>12}",
        "scene", "edits", "parse ms", "compile ms"
    );
    println!("{}", "-".repeat(72));

    for scene in &args.scenes {
        let name = scene
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(source) = std::fs::read_to_string(scene) else {
            continue;
        };

        let timings = measure_edit_cycle(&source, scene, args.iterations.max(1));
        println!(
            "{:<34} {:>10} {:>12.3} {:>12.3}",
            name,
            timings.edits,
            millis(timings.mean_parse()),
            millis(timings.mean_compile()),
        );
    }
}

fn run_playback(args: &Args) {
    println!(
        "{:<34} {:>8} {:>10} {:>10} {:>10} {:>10}",
        "scene", "frames", "total ms", "mean ms", "p95 ms", "worst ms"
    );
    println!("{}", "-".repeat(86));

    for scene in &args.scenes {
        let name = scene
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let source = match std::fs::read_to_string(scene) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{name}: {error}");
                continue;
            }
        };

        match measure_playback(&source, scene, args.fps) {
            Ok(timings) => {
                if !timings.errors.is_empty() {
                    eprintln!("{name}: {:?}", timings.errors);
                }
                println!(
                    "{:<34} {:>8} {:>10.1} {:>10.2} {:>10.2} {:>10.2}",
                    name,
                    timings.frames,
                    millis(timings.total),
                    millis(timings.mean()),
                    millis(timings.p95),
                    millis(timings.worst),
                );
            }
            Err(error) => eprintln!("{name}: {error}"),
        }
    }
}

fn main() {
    use_repo_assets();
    let args = parse_args();

    if args.edit {
        run_edit_cycle(&args);
        return;
    }

    if args.playback {
        run_playback(&args);
        return;
    }

    println!(
        "{:<34} {:>10} {:>10} {:>10} {:>10}",
        "scene", "parse ms", "compile ms", "exec ms", "total ms"
    );
    println!("{}", "-".repeat(78));

    let mut totals = SceneTimings::default();
    for scene in &args.scenes {
        let name = scene
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        for _ in 0..args.warmup {
            let _ = run_scene_file(scene, args.options);
        }

        let mut best: Option<SceneTimings> = None;
        let mut last_transcript = Vec::new();
        for _ in 0..args.iterations.max(1) {
            match run_scene_file(scene, args.options) {
                Ok(run) => {
                    last_transcript = run.transcript;
                    if !run.runtime_errors.is_empty() {
                        eprintln!("{name}: runtime errors: {:?}", run.runtime_errors);
                    }
                    best = Some(match best {
                        Some(previous) if previous.total() <= run.timings.total() => previous,
                        _ => run.timings,
                    });
                }
                Err(error) => {
                    eprintln!("{name}: {error}");
                    break;
                }
            }
        }

        let Some(timings) = best else { continue };
        totals.parse += timings.parse;
        totals.compile += timings.compile;
        totals.execute += timings.execute;

        println!(
            "{:<34} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",
            name,
            millis(timings.parse),
            millis(timings.compile),
            millis(timings.execute),
            millis(timings.total()),
        );

        if args.print_transcript {
            for line in &last_transcript {
                println!("    | {line}");
            }
        }
    }

    println!("{}", "-".repeat(78));
    println!(
        "{:<34} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",
        "TOTAL",
        millis(totals.parse),
        millis(totals.compile),
        millis(totals.execute),
        millis(totals.total()),
    );
}
