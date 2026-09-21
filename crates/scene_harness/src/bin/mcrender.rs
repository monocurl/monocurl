//! headless render benchmark: steps a scene frame by frame and renders each one
//! offscreen, reporting how the frame budget splits between evaluating the scene
//! and drawing it.
//!
//! usage: mcrender [--fps N] [--size WxH] [scene.mcs ...]

use std::{path::PathBuf, time::Duration};

use executor::executor::{PlaybackAdvance, SeekOptions};
use renderer::{RenderOptions, RenderSize, Renderer, SceneRenderData};
use scene_harness::{bench_scenes, prepare, use_repo_assets};

struct Args {
    fps: u32,
    size: RenderSize,
    scenes: Vec<PathBuf>,
}

fn parse_size(raw: &str) -> Option<RenderSize> {
    let (width, height) = raw.split_once(['x', 'X'])?;
    Some(RenderSize {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
    })
}

fn parse_args() -> Args {
    let mut fps = 60;
    let mut size = RenderSize {
        width: 1280,
        height: 720,
    };
    let mut scenes = Vec::new();

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--fps" => fps = argv.next().and_then(|v| v.parse().ok()).expect("--fps N"),
            "--size" => {
                size = argv
                    .next()
                    .as_deref()
                    .and_then(parse_size)
                    .expect("--size WxH")
            }
            other => scenes.push(PathBuf::from(other)),
        }
    }

    if scenes.is_empty() {
        scenes = bench_scenes();
    }

    Args { fps, size, scenes }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

fn main() {
    use_repo_assets();
    let args = parse_args();

    let mut renderer = match Renderer::try_new(RenderOptions::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("cannot initialize the renderer: {error:#}");
            std::process::exit(1);
        }
    };

    println!(
        "{:<34} {:>8} {:>11} {:>11} {:>11} {:>11}",
        "scene", "frames", "eval ms", "render ms", "frame ms", "worst ms"
    );
    println!("{}", "-".repeat(92));

    for scene in &args.scenes {
        let name = scene
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(source) = std::fs::read_to_string(scene) else {
            continue;
        };
        let Ok((mut executor, _)) = prepare(&source, scene) else {
            eprintln!("{name}: failed to prepare");
            continue;
        };

        executor.update_aspect_ratio(args.size.width as f32 / args.size.height as f32);

        let frame_dt = 1.0 / f64::from(args.fps.max(1));
        let max_slide = executor.total_sections();
        let (mut eval, mut draw, mut worst) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
        let mut frames = 0usize;

        smol::block_on(async {
            let _ = executor
                .seek_to_with_options(
                    executor::time::Timestamp::new(0, 0.0),
                    SeekOptions::fast(),
                )
                .await;

            loop {
                let started = std::time::Instant::now();
                let produced = executor.produce_frame(max_slide, frame_dt).await;
                let evaluated = started.elapsed();

                let (advance, snapshot) = match produced {
                    Ok((PlaybackAdvance::Finished, _, _)) => break,
                    Ok((advance, snapshot, _)) => (advance, snapshot),
                    Err(error) => {
                        eprintln!("{name}: {error}");
                        break;
                    }
                };
                let _ = advance;

                let scene_data = SceneRenderData::from(snapshot);
                let started = std::time::Instant::now();
                if let Err(error) = renderer.render(&scene_data, args.size) {
                    eprintln!("{name}: render failed: {error:#}");
                    break;
                }
                let drawn = started.elapsed();

                eval += evaluated;
                draw += drawn;
                worst = worst.max(evaluated + drawn);
                frames += 1;

                if frames > 20_000 {
                    break;
                }
            }
        });

        let divisor = frames.max(1) as u32;
        println!(
            "{:<34} {:>8} {:>11.2} {:>11.2} {:>11.2} {:>11.2}",
            name,
            frames,
            millis(eval / divisor),
            millis(draw / divisor),
            millis((eval + draw) / divisor),
            millis(worst),
        );
    }
}
