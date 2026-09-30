//! golden-output regression tests over the shared `.mcs` scene corpus.
//!
//! each scene has a committed `.expected` file recording its transcript, runtime
//! errors, and end timestamp. run with `UPDATE_EXPECT=1` to rewrite them after an
//! intentional behaviour change, and read the diff before committing it.
//!
//! comparison is tolerant so the goldens reproduce across platforms: lines are
//! split into numeric and textual tokens, text must match exactly and numbers
//! within `NUMBER_TOLERANCE`. meshes are recorded as aggregates (see
//! `summary.rs`) rather than hashes for the same reason, see
//! https://github.com/monocurl/monocurl/issues/68

use std::path::Path;

use executor::executor::SeekOptions;
use scene_harness::{
    SceneRun, TimelineSample, corpus_scenes, run_scene_file, sample_timeline, use_repo_assets,
};

/// absolute slack per number; goldens print mesh aggregates with 3 decimals
const NUMBER_TOLERANCE: f64 = 2e-3;

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

#[derive(Debug, PartialEq)]
enum Token<'a> {
    Text(&'a str),
    Number(f64),
}

fn tokenize(line: &str) -> Vec<Token<'_>> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut text_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let after_word = i > 0 && is_word_byte(bytes[i - 1]);
        let starts_number = !after_word
            && (bytes[i].is_ascii_digit()
                || (bytes[i] == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)));
        let Some(end) = starts_number.then(|| number_end(bytes, i)).flatten() else {
            i += 1;
            continue;
        };
        if text_start < i {
            tokens.push(Token::Text(&line[text_start..i]));
        }
        tokens.push(Token::Number(
            line[i..end].parse().expect("scanned a number"),
        ));
        i = end;
        text_start = end;
    }
    if text_start < bytes.len() {
        tokens.push(Token::Text(&line[text_start..]));
    }
    tokens
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// end of the numeric literal starting at `start`, provided it ends on a word
/// boundary so identifier-like runs such as `1x` stay text
fn number_end(bytes: &[u8], start: usize) -> Option<usize> {
    let digits = |mut i: usize| {
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        i
    };
    let mut end = digits(start + usize::from(bytes[start] == b'-'));
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end = digits(end + 1);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'-' | b'+')));
        if bytes.get(end + 1 + sign).is_some_and(u8::is_ascii_digit) {
            end = digits(end + 1 + sign);
        }
    }
    bytes
        .get(end)
        .is_none_or(|&byte| !is_word_byte(byte))
        .then_some(end)
}

fn lines_match(expected: &str, actual: &str) -> bool {
    let (expected, actual) = (tokenize(expected), tokenize(actual));
    expected.len() == actual.len()
        && expected.iter().zip(&actual).all(|pair| match pair {
            (Token::Number(e), Token::Number(a)) => (e - a).abs() <= NUMBER_TOLERANCE,
            (e, a) => e == a,
        })
}

/// the first line that differs beyond tolerance, rendered for a failure message
fn first_mismatch(expected: &str, actual: &str) -> Option<String> {
    let expected_lines: Vec<_> = expected.lines().collect();
    let actual_lines: Vec<_> = actual.lines().collect();
    let line = (0..expected_lines.len().max(actual_lines.len())).find(|&i| {
        match (expected_lines.get(i), actual_lines.get(i)) {
            (Some(e), Some(a)) => !lines_match(e, a),
            _ => true,
        }
    })?;
    let show = |lines: &[&str]| {
        lines
            .get(line)
            .map_or_else(|| "<missing>".to_string(), |text| format!("{text:?}"))
    };
    Some(format!(
        "line {}:\n  expected: {}\n  actual:   {}",
        line + 1,
        show(&expected_lines),
        show(&actual_lines),
    ))
}

/// a failure description, or `None` when the scene matches (or was rewritten)
fn check(scene: &Path, actual: String) -> Option<String> {
    let expected_path = scene.with_extension("expected");

    if std::env::var_os("UPDATE_EXPECT").is_some() {
        std::fs::write(&expected_path, &actual).expect("failed to write expectation");
        return None;
    }

    let Ok(expected) = std::fs::read_to_string(&expected_path) else {
        return Some(format!(
            "missing expectation for {}; rerun with UPDATE_EXPECT=1",
            scene.display()
        ));
    };

    first_mismatch(&expected, &actual).map(|mismatch| {
        format!(
            "scene {} differs at {mismatch}\n--- actual ---\n{actual}",
            scene.display(),
        )
    })
}

#[test]
fn corpus_scenes_match_expectations() {
    use_repo_assets();

    let scenes = corpus_scenes();
    assert!(!scenes.is_empty(), "scene corpus should not be empty");

    let mut failures = Vec::new();
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
        failures.extend(check(&scene, rendered));
    }

    assert!(
        failures.is_empty(),
        "{} scene(s) no longer match their expectations.\n\
         if this change is intentional, rerun with UPDATE_EXPECT=1 and review the diff.\n\n{}",
        failures.len(),
        failures.join("\n\n"),
    );
}

#[test]
fn comparison_tolerates_numeric_noise_only() {
    assert!(lines_match("box=(0.000, -1.250)", "box=(0.001, -1.249)"));
    assert!(!lines_match("box=(0.000, -1.250)", "box=(0.010, -1.250)"));
    assert!(!lines_match("mesh[0] dots=1", "mesh[0] lins=1"));
    assert!(!lines_match("print 3", "print 3 4"));
    assert!(lines_match("x2 = 1e-7", "x2 = 0.0"));
    assert!(!lines_match("x2 = 1", "x3 = 1"));
    assert_eq!(
        tokenize("a-1 [2.5, -3e2]"),
        [
            Token::Text("a-"),
            Token::Number(1.0),
            Token::Text(" ["),
            Token::Number(2.5),
            Token::Text(", "),
            Token::Number(-300.0),
            Token::Text("]"),
        ]
    );
}
