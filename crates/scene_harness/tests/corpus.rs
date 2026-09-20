//! golden-output regression tests over the shared `.mcs` scene corpus.
//!
//! each scene has a committed `.expected` file recording its transcript, runtime
//! errors, and end timestamp. run with `UPDATE_EXPECT=1` to rewrite them after an
//! intentional behaviour change, and read the diff before committing it.

use std::path::Path;

use executor::executor::SeekOptions;
use scene_harness::{
    SceneRun, TimelineSample, corpus_scenes, run_scene_file, sample_timeline, use_repo_assets,
};

fn render_timeline(samples: &[TimelineSample]) -> String {
    let mut out = String::new();
    out.push_str("\n# timeline\n");
    for sample in samples {
        out.push_str(&format!(
            "slide {} @ {:.2}\n",
            sample.slide, sample.fraction
        ));
        for line in &sample.scene {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        for leader in &sample.leaders {
            out.push_str("  ");
            out.push_str(leader);
            out.push('\n');
        }
        for error in &sample.errors {
            out.push_str("  !! ");
            out.push_str(error);
            out.push('\n');
        }
    }
    out
}

fn render(run: &SceneRun) -> String {
    let mut out = String::new();
    out.push_str("# transcript\n");
    for line in &run.transcript {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("\n# runtime errors\n");
    for error in &run.runtime_errors {
        out.push_str(error);
        out.push('\n');
    }
    out.push_str("\n# end timestamp\n");
    match run.end_timestamp {
        Some((slide, time)) if time.is_infinite() => {
            out.push_str(&format!("slide {slide}, end of slide\n"))
        }
        Some((slide, time)) => out.push_str(&format!("slide {slide}, t = {time:.4}\n")),
        None => out.push_str("(none)\n"),
    }
    out
}

fn check(scene: &Path, actual: String) {
    let expected_path = scene.with_extension("expected");

    if std::env::var_os("UPDATE_EXPECT").is_some() {
        std::fs::write(&expected_path, &actual).expect("failed to write expectation");
        return;
    }

    let expected = std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
        panic!(
            "missing expectation for {}; rerun with UPDATE_EXPECT=1",
            scene.display()
        )
    });

    assert_eq!(
        actual,
        expected,
        "\nscene {} no longer matches its expectation.\n\
         if this change is intentional, rerun with UPDATE_EXPECT=1 and review the diff.\n\
         --- actual ---\n{actual}\n--- expected ---\n{expected}",
        scene.display(),
    );
}

#[test]
fn corpus_scenes_match_expectations() {
    use_repo_assets();

    let scenes = corpus_scenes();
    assert!(!scenes.is_empty(), "scene corpus should not be empty");

    for scene in scenes {
        let source = std::fs::read_to_string(&scene).expect("scene should be readable");
        let rendered = match run_scene_file(&scene, SeekOptions::strict()) {
            Ok(run) => {
                let mut rendered = render(&run);
                match sample_timeline(&source, &scene, SeekOptions::strict()) {
                    Ok(samples) => rendered.push_str(&render_timeline(&samples)),
                    Err(error) => rendered.push_str(&format!("\n# timeline error\n{error}\n")),
                }
                rendered
            }
            Err(error) => format!("# pipeline error\n{error}\n"),
        };
        check(&scene, rendered);
    }
}
