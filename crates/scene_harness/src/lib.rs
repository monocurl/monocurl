//! headless scene pipeline (lex -> parse -> compile -> execute) shared by the
//! benchmark binary and the scene corpus regression tests. deliberately free of
//! renderer / gpui dependencies so it stays cheap to build and run.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use compiler::{cache::CompilerCache, compiler::compile};
use executor::{
    executor::{Executor, SeekOptions, SeekToResult},
    heap::with_heap,
    scene_snapshot::SceneSnapshot,
    state::LeaderKind,
    time::Timestamp,
};
use lexer::lex_rope_from_str;
use parser::{import_context::ParseImportContext, parser::Parser};
use stdlib::registry::registry;
use structs::rope::Rope;

pub mod summary;

use crate::summary::{mesh_summary, value_summary};

#[derive(Debug)]
pub enum SceneError {
    Parse(Vec<String>),
    Compile(Vec<String>),
    Runtime(Vec<String>),
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (label, messages) = match self {
            Self::Parse(messages) => ("parse", messages),
            Self::Compile(messages) => ("compile", messages),
            Self::Runtime(messages) => ("runtime", messages),
        };
        write!(f, "{label} error: {}", messages.join("; "))
    }
}

impl std::error::Error for SceneError {}

/// outcome of running a scene to completion
#[derive(Clone, Debug, Default)]
pub struct SceneRun {
    pub transcript: Vec<String>,
    /// runtime errors, reported rather than returned so scenes may assert on them
    pub runtime_errors: Vec<String>,
    pub end_timestamp: Option<(usize, f64)>,
    pub timings: SceneTimings,
}

/// the live scene state at one sampled point on the timeline
#[derive(Clone, Debug)]
pub struct TimelineSample {
    pub slide: usize,
    pub fraction: f64,
    pub leaders: Vec<String>,
    /// the resolved on-screen state: what a viewer would actually see
    pub scene: Vec<String>,
    pub errors: Vec<String>,
}

/// where along each slide the timeline is sampled. the endpoints catch the
/// snapped keyframes, the interior points catch interpolation
const SAMPLE_FRACTIONS: [f64; 4] = [0.0, 0.35, 0.8, 1.0];

#[derive(Clone, Copy, Debug, Default)]
pub struct SceneTimings {
    pub parse: Duration,
    pub compile: Duration,
    pub execute: Duration,
}

impl SceneTimings {
    pub fn total(&self) -> Duration {
        self.parse + self.compile + self.execute
    }
}

/// compile `source` (resolving imports relative to `path`) into a ready executor
pub fn prepare(source: &str, path: &Path) -> Result<(Executor, SceneTimings), SceneError> {
    let mut timings = SceneTimings::default();

    let started = Instant::now();
    let text_rope = Rope::from_text(source);
    let lex_rope = lex_rope_from_str(source);
    let mut import_context = ParseImportContext::new(path.to_path_buf());
    let (bundles, artifacts) = Parser::parse(&mut import_context, lex_rope, text_rope, None);
    timings.parse = started.elapsed();

    if !artifacts.error_diagnostics.is_empty() {
        return Err(SceneError::Parse(
            artifacts
                .error_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect(),
        ));
    }

    let started = Instant::now();
    let result = compile(&mut CompilerCache::default(), None, &bundles);
    timings.compile = started.elapsed();

    if !result.errors.is_empty() {
        return Err(SceneError::Compile(
            result.errors.iter().map(|e| e.message.clone()).collect(),
        ));
    }

    Ok((
        Executor::new(result.bytecode, registry().func_table()),
        timings,
    ))
}

/// run a prepared executor to the end of the scene
pub fn execute(mut executor: Executor, options: SeekOptions) -> SceneRun {
    let mut run = SceneRun::default();

    let started = Instant::now();
    smol::block_on(async {
        let target = Timestamp::new(executor.total_sections(), f64::INFINITY);
        match executor.seek_to_with_options(target, options).await {
            SeekToResult::SeekedTo(timestamp) => {
                let user = executor.internal_to_user_timestamp(timestamp);
                run.end_timestamp = Some((user.slide, user.time));
            }
            SeekToResult::Error(error) => run.runtime_errors.push(error.to_string()),
        }
    });
    run.timings.execute = started.elapsed();

    run.runtime_errors.extend(
        executor
            .state
            .errors
            .iter()
            .map(|error| error.error.to_string()),
    );
    run.transcript = executor
        .state
        .transcript
        .iter_entries()
        .map(|entry| entry.text().to_string())
        .collect();

    run
}

/// step a prepared executor across the timeline, recording leader state at each
/// sample. this is what makes the corpus a real regression guard for animation:
/// the transcript alone says nothing about what is on screen
pub fn sample_timeline(
    source: &str,
    path: &Path,
    options: SeekOptions,
) -> Result<Vec<TimelineSample>, SceneError> {
    let (mut executor, _) = prepare(source, path)?;
    let mut samples = Vec::new();

    smol::block_on(async {
        let durations = resolve_slide_durations(&mut executor).await;

        // real slides are 1-based in user timestamps; slide 0 is the init section
        for (slide, duration) in durations.iter().copied().enumerate() {
            for fraction in SAMPLE_FRACTIONS {
                let time = if fraction >= 1.0 {
                    f64::INFINITY
                } else {
                    duration * fraction
                };
                let target =
                    executor.user_to_internal_timestamp(Timestamp::new(slide + 1, time));

                let mut errors = Vec::new();
                if let SeekToResult::Error(error) =
                    executor.seek_to_with_options(target, options).await
                {
                    errors.push(error.to_string());
                }

                let scene = match executor.stable_scene_snapshot().await {
                    Ok(snapshot) => scene_summary(&snapshot),
                    Err(error) => {
                        errors.push(error.to_string());
                        Vec::new()
                    }
                };

                samples.push(TimelineSample {
                    slide: slide + 1,
                    fraction,
                    leaders: leader_summaries(&executor),
                    scene,
                    errors,
                });
            }
        }
    });

    Ok(samples)
}

/// run the scene once to the end so every slide's duration is known, then report
/// them in real-slide order
pub async fn resolve_slide_durations(executor: &mut Executor) -> Vec<f64> {
    let end = Timestamp::new(executor.total_sections(), f64::INFINITY);
    let _ = executor.seek_to_with_options(end, SeekOptions::fast()).await;

    let durations = executor.real_slide_durations();
    let minimums = executor.real_minimum_slide_durations();
    (0..executor.real_slide_count())
        .map(|slide| {
            durations
                .get(slide)
                .copied()
                .flatten()
                .or_else(|| minimums.get(slide).copied().flatten())
                .unwrap_or_default()
        })
        .collect()
}

fn scene_summary(snapshot: &SceneSnapshot) -> Vec<String> {
    let mut lines = vec![format!(
        "camera pos {:?} look_at {:?}",
        round3(snapshot.camera.position),
        round3(snapshot.camera.look_at),
    )];
    lines.extend(
        snapshot
            .meshes
            .iter()
            .enumerate()
            .map(|(index, mesh)| format!("mesh[{index}] {}", mesh_summary(mesh))),
    );
    lines
}

fn round3(value: geo::simd::Float3) -> [f32; 3] {
    [value.x, value.y, value.z].map(|component| (component * 1e4).round() / 1e4)
}

/// scene-level params, reported as follower state with the target appended when
/// the two differ (mid-animation). meshes are omitted here because the scene
/// snapshot above already records their resolved geometry, and the camera and
/// background are omitted because they are reported as part of the snapshot
fn leader_summaries(executor: &Executor) -> Vec<String> {
    executor
        .state
        .leaders
        .iter()
        .filter(|entry| entry.kind == LeaderKind::Param)
        .filter(|entry| !matches!(entry.name.as_str(), "camera" | "background"))
        .map(|entry| {
            let follower = value_summary(&with_heap(|heap| heap.get(entry.follower_value).clone()));
            let leader = value_summary(&with_heap(|heap| heap.get(entry.leader_value).clone()));
            if follower == leader {
                format!("param {} = {follower}", entry.name)
            } else {
                format!("param {} = {follower} -> {leader}", entry.name)
            }
        })
        .collect()
}

/// full pipeline for a scene on disk
pub fn run_scene_file(path: &Path, options: SeekOptions) -> Result<SceneRun, SceneError> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read scene {}: {error}", path.display()));
    run_scene(&source, path, options)
}

pub fn run_scene(source: &str, path: &Path, options: SeekOptions) -> Result<SceneRun, SceneError> {
    let (executor, prepare_timings) = prepare(source, path)?;
    let mut run = execute(executor, options);
    run.timings.parse = prepare_timings.parse;
    run.timings.compile = prepare_timings.compile;
    Ok(run)
}

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// scenes covering language and stdlib behaviour, each with a committed
/// `.expected` transcript. kept small enough to run in a debug build
pub fn corpus_dir() -> PathBuf {
    crate_dir().join("scenes")
}

/// scenes sized for measurement rather than assertion; not part of the golden tests
pub fn bench_dir() -> PathBuf {
    crate_dir().join("benches")
}

fn scenes_in(directory: PathBuf) -> Vec<PathBuf> {
    let mut scenes: Vec<PathBuf> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mcs"))
        .collect();
    scenes.sort();
    scenes
}

/// every `.mcs` scene in the correctness corpus, sorted by name
pub fn corpus_scenes() -> Vec<PathBuf> {
    scenes_in(corpus_dir())
}

/// every `.mcs` scene in the benchmark set, sorted by name
pub fn bench_scenes() -> Vec<PathBuf> {
    scenes_in(bench_dir())
}

/// point asset lookups at the in-repo assets directory so stdlib imports resolve
/// regardless of the working directory the harness runs from
pub fn use_repo_assets() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate lives two levels below the repo root");
    // SAFETY: called before any worker threads touch the environment
    unsafe { std::env::set_var("MONOCURL_ASSETS_DIR", repo_root.join("assets")) };
}
