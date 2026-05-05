// animation test framework and tests
// covers: slide durations, leader values, multi-slide scenes, stdlib usage

use std::{cell::Cell, f64, path::PathBuf, sync::Arc};

use executor::{
    camera::parse_camera_value,
    error::ExecutorError,
    executor::{Executor, PlaybackAdvance, SeekToResult},
    heap::{VRc, with_heap},
    state::LeaderKind,
    time::Timestamp,
    value::{
        Value,
        container::{HashableKey, List, Map},
    },
};
use integration_tests::{
    compile_bundles, inspect_block, make_imported_bundle, make_section_bundle, parse_section,
    print_inspection, stdlib_bundle_with_import_span, stdlib_bundles, value_summary,
};
use parser::ast::{Section, SectionBundle, SectionType};
use stdlib::registry::registry;
use structs::text::Span8;

// ── snapshot types ────────────────────────────────────────────────────────────

pub struct LeaderInfo {
    pub kind: LeaderKind,
    /// the value the leader is animating toward (what the code last set it to)
    pub target: Value,
    /// the on-screen value at the seek point (interpolated/snapped by animations)
    pub current: Value,
}

impl LeaderInfo {
    fn inspection_lines(&self) -> Vec<String> {
        vec![
            format!("kind: {}", leader_kind_name(self.kind)),
            format!("target: {}", value_summary(&self.target)),
            format!("current: {}", value_summary(&self.current)),
        ]
    }

    fn inspection(&self) -> String {
        inspect_block("leader", self.inspection_lines())
    }

    #[allow(dead_code)]
    pub fn inspect(&self, label: &str) -> &Self {
        print_inspection(label, self.inspection_lines());
        self
    }

    pub fn assert_target_int(&self, expected: i64) -> &Self {
        match &self.target {
            Value::Integer(n) => assert_eq!(
                *n,
                expected,
                "leader target int mismatch\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected Integer({expected}), got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        }
        self
    }

    pub fn assert_current_int(&self, expected: i64) -> &Self {
        match &self.current {
            Value::Integer(n) => assert_eq!(
                *n,
                expected,
                "leader current int mismatch\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected Integer({expected}), got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        }
        self
    }

    pub fn assert_current_float(&self, expected: f64, eps: f64) -> &Self {
        match &self.current {
            Value::Float(f) => assert!(
                (f - expected).abs() < eps,
                "leader current float mismatch: expected {expected}, got {f}\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected Float({expected}), got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        }
        self
    }
}

// ── animation result ──────────────────────────────────────────────────────────

pub struct AnimResult {
    /// actual timestamp after seeking (user-visible, slide 0 = first user slide)
    pub timestamp: Timestamp,
    pub leaders: Vec<LeaderInfo>,
    /// number of user-visible slides in the compiled scene
    pub user_slide_count: usize,
    pub transcript: Vec<String>,
    pub errors: Vec<String>,
    pub error_spans: Vec<Span8>,
}

impl AnimResult {
    fn inspection_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "timestamp: slide={}, time={}",
                self.timestamp.slide, self.timestamp.time
            ),
            format!("user slide count: {}", self.user_slide_count),
            format!("transcript: {:?}", self.transcript),
            format!("errors: {:?}", self.errors),
            format!("error spans: {:?}", self.error_spans),
        ];
        lines.extend(self.leaders.iter().enumerate().map(|(idx, leader)| {
            format!(
                "leader[{idx}] {} target={} current={}",
                leader_kind_name(leader.kind),
                value_summary(&leader.target),
                value_summary(&leader.current)
            )
        }));
        lines
    }

    fn inspection(&self) -> String {
        inspect_block("anim result", self.inspection_lines())
    }

    #[allow(dead_code)]
    pub fn inspect(&self, label: &str) -> &Self {
        print_inspection(label, self.inspection_lines());
        self
    }

    pub fn assert_ok(&self) -> &Self {
        assert!(
            self.errors.is_empty(),
            "expected no errors, got: {:?}\n{}",
            self.errors,
            self.inspection()
        );
        self
    }

    pub fn assert_error(&self, fragment: &str) -> &Self {
        assert!(
            self.errors.iter().any(|e| e.contains(fragment)),
            "expected error containing {:?}, got: {:?}\n{}",
            fragment,
            self.errors,
            self.inspection()
        );
        self
    }

    pub fn assert_transcript(&self, expected: &[&str]) -> &Self {
        self.assert_ok();
        assert_eq!(
            self.transcript,
            expected
                .iter()
                .map(|entry| entry.to_string())
                .collect::<Vec<_>>(),
            "transcript mismatch\n{}",
            self.inspection()
        );
        self
    }

    pub fn assert_user_slide_count(&self, n: usize) -> &Self {
        assert_eq!(
            self.user_slide_count,
            n,
            "user slide count mismatch\n{}",
            self.inspection()
        );
        self
    }

    /// time offset within the current slide at the reached timestamp.
    /// when seeking to f64::INFINITY this equals the total animation duration
    /// of the slide.
    pub fn slide_time(&self) -> f64 {
        self.timestamp.time
    }

    pub fn assert_slide_time_approx(&self, expected: f64, eps: f64) -> &Self {
        let actual = self.slide_time();
        assert!(
            (actual - expected).abs() <= eps,
            "slide time mismatch: expected ~{expected}, got {actual}\n{}",
            self.inspection()
        );
        self
    }

    pub fn mesh_leaders(&self) -> Vec<&LeaderInfo> {
        self.leaders
            .iter()
            .filter(|l| l.kind == LeaderKind::Mesh)
            .collect()
    }

    pub fn scene_leaders(&self) -> Vec<&LeaderInfo> {
        self.leaders
            .iter()
            .filter(|l| l.kind == LeaderKind::Scene)
            .collect()
    }

    pub fn mesh_leader(&self, index: usize) -> &LeaderInfo {
        self.nth_leader(LeaderKind::Mesh, index)
    }

    #[allow(dead_code)]
    pub fn scene_leader(&self, index: usize) -> &LeaderInfo {
        self.nth_leader(LeaderKind::Scene, index)
    }

    pub fn assert_mesh_target_int(&self, index: usize, expected: i64) -> &Self {
        self.mesh_leader(index).assert_target_int(expected);
        self
    }

    fn nth_leader(&self, kind: LeaderKind, index: usize) -> &LeaderInfo {
        let mut seen = 0;
        for leader in &self.leaders {
            if leader.kind == kind {
                if seen == index {
                    return leader;
                }
                seen += 1;
            }
        }

        panic!(
            "expected {} leader at index {}, found {}\n{}",
            leader_kind_name(kind),
            index,
            seen,
            self.inspection()
        )
    }
}

// ── internal helpers ──────────────────────────────────────────────────────────

fn leader_kind_name(kind: LeaderKind) -> &'static str {
    match kind {
        LeaderKind::Mesh => "mesh",
        LeaderKind::Scene => "scene",
    }
}

fn mesh_tree_leaves(value: &Value, out: &mut Vec<Value>) {
    fn clone_cached(cell: &Cell<Option<Box<Value>>>) -> Option<Value> {
        let cached = cell.take();
        let cloned = cached.as_ref().map(|value| (**value).clone());
        cell.set(cached);
        cloned
    }

    match value {
        Value::Mesh(mesh) => out.push(Value::Mesh(mesh.clone())),
        Value::List(list) => {
            for elem in list.elements() {
                let elem = with_heap(|h| h.get(elem.key()).clone());
                mesh_tree_leaves(&elem, out);
            }
        }
        Value::InvokedOperator(inv) => {
            if let Some(value) = clone_cached(&inv.cache.cached_result) {
                mesh_tree_leaves(&value, out);
            }
        }
        Value::InvokedFunction(inv) => {
            if let Some(value) = clone_cached(&inv.cache.0) {
                mesh_tree_leaves(&value, out);
            }
        }
        Value::Lvalue(vrc) => {
            let value = with_heap(|h| h.get(vrc.key()).clone());
            mesh_tree_leaves(&value, out);
        }
        Value::WeakLvalue(vweak) => {
            let value = with_heap(|h| h.get(vweak.key()).clone());
            mesh_tree_leaves(&value, out);
        }
        Value::Leader(leader) => {
            let value = with_heap(|h| h.get(leader.leader_rc.key()).clone());
            mesh_tree_leaves(&value, out);
        }
        _ => {}
    }
}

fn mesh_leaf_primary_tags(value: &Value) -> Vec<isize> {
    let mut leaves = Vec::new();
    mesh_tree_leaves(value, &mut leaves);
    leaves
        .into_iter()
        .map(|leaf| {
            let Value::Mesh(mesh) = leaf else {
                panic!("expected mesh leaf");
            };
            *mesh.tag.first().unwrap_or(&-1)
        })
        .collect()
}

fn sort_tag_sets(mut sets: Vec<Vec<isize>>) -> Vec<Vec<isize>> {
    for set in &mut sets {
        set.sort_unstable();
    }
    sets.sort();
    sets
}

fn mesh_line_span(value: &Value) -> f32 {
    let Value::Mesh(mesh) = value else {
        panic!("expected mesh value");
    };
    mesh.lins
        .iter()
        .map(|lin| (lin.b.pos - lin.a.pos).len())
        .fold(0.0, f32::max)
}

fn mesh_max_alpha(value: &Value) -> f32 {
    let Value::Mesh(mesh) = value else {
        panic!("expected mesh value");
    };
    let vertex_alpha = mesh
        .dots
        .iter()
        .map(|dot| dot.col.w)
        .chain(mesh.lins.iter().flat_map(|lin| [lin.a.col.w, lin.b.col.w]))
        .chain(
            mesh.tris
                .iter()
                .flat_map(|tri| [tri.a.col.w, tri.b.col.w, tri.c.col.w]),
        )
        .fold(0.0, f32::max);
    vertex_alpha * mesh.uniform.alpha as f32
}

fn mesh_center_y(value: &Value) -> f32 {
    let Value::Mesh(mesh) = value else {
        panic!("expected mesh value");
    };

    let mut points = mesh
        .dots
        .iter()
        .map(|dot| dot.pos)
        .chain(mesh.lins.iter().flat_map(|lin| [lin.a.pos, lin.b.pos]))
        .chain(
            mesh.tris
                .iter()
                .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos]),
        );
    let first = points.next().expect("expected mesh geometry");
    let mut min_y = first.y;
    let mut max_y = first.y;
    for point in points {
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    (min_y + max_y) / 2.0
}

async fn current_mesh_leader_value(executor: &mut Executor) -> Value {
    let entry = executor
        .state
        .leaders
        .iter()
        .find(|entry| entry.kind == LeaderKind::Mesh)
        .expect("expected mesh leader");
    let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
    let Value::Leader(leader) = cell_val else {
        panic!("mesh leader entry is not a Leader value");
    };

    with_heap(|h| h.get(leader.follower_rc.key()).clone())
        .elide_wrappers_rec(executor)
        .await
        .expect("mesh leader wrapper elision should succeed")
}

fn build_anim_executor(
    slides: &[(&str, SectionType)],
    stdlib_bundles: &[Arc<SectionBundle>],
) -> Result<(Executor, usize), AnimResult> {
    build_anim_executor_with_file_path(slides, stdlib_bundles, PathBuf::from("scene.mcs"))
}

fn build_anim_executor_with_file_path(
    slides: &[(&str, SectionType)],
    stdlib_bundles: &[Arc<SectionBundle>],
    file_path: PathBuf,
) -> Result<(Executor, usize), AnimResult> {
    let mut all_errors: Vec<String> = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    for (src, section_type) in slides {
        let (section, errors) = parse_section(src, section_type.clone());
        all_errors.extend(errors);
        sections.push(section);
    }

    if !all_errors.is_empty() {
        return Err(AnimResult {
            timestamp: Timestamp::default(),
            leaders: vec![],
            user_slide_count: 0,
            transcript: vec![],
            errors: all_errors,
            error_spans: vec![],
        });
    }

    let imported_files: Vec<usize> = (0..stdlib_bundles.len()).collect();

    let user_bundle = make_section_bundle(file_path, 0, imported_files, sections, None);

    let mut bundles: Vec<Arc<SectionBundle>> = stdlib_bundles.to_vec();
    bundles.push(user_bundle);

    let result = compile_bundles(&bundles);

    let compile_errors: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
    if !compile_errors.is_empty() {
        return Err(AnimResult {
            timestamp: Timestamp::default(),
            leaders: vec![],
            user_slide_count: 0,
            transcript: vec![],
            errors: compile_errors,
            error_spans: vec![],
        });
    }

    let executor = Executor::new(result.bytecode, registry().func_table());
    let user_slide_count = executor.real_slide_count();

    Ok((executor, user_slide_count))
}

fn collect_anim_result(
    executor: Executor,
    user_slide_count: usize,
    mut runtime_errors: Vec<String>,
) -> AnimResult {
    runtime_errors.extend(
        executor
            .state
            .errors
            .iter()
            .map(|runtime_error| runtime_error.error.to_string()),
    );
    let error_spans = executor
        .state
        .errors
        .iter()
        .map(|runtime_error| runtime_error.span.clone())
        .collect();

    let leaders = executor
        .state
        .leaders
        .iter()
        .map(|entry| {
            let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
            let Value::Leader(leader) = cell_val else {
                panic!("leader entry is not a Leader value");
            };
            LeaderInfo {
                kind: entry.kind,
                target: with_heap(|h| h.get(leader.leader_rc.key()).clone()),
                current: with_heap(|h| h.get(leader.follower_rc.key()).clone()),
            }
        })
        .collect();

    let transcript = executor
        .state
        .transcript
        .iter_entries()
        .map(|entry| entry.text().to_string())
        .collect();

    let mut timestamp = executor.internal_to_user_timestamp(executor.state.timestamp);
    let visible_timestamp_slide = timestamp.slide.checked_sub(1);
    if timestamp.time.is_infinite() {
        timestamp.time = visible_timestamp_slide
            .and_then(|visible_slide| {
                executor
                    .real_slide_durations()
                    .get(visible_slide)
                    .copied()
                    .flatten()
                    .or_else(|| {
                        executor
                            .real_minimum_slide_durations()
                            .get(visible_slide)
                            .copied()
                            .flatten()
                    })
            })
            .unwrap_or_default();
    }
    timestamp.slide = visible_timestamp_slide.unwrap_or_default();

    AnimResult {
        timestamp,
        leaders,
        user_slide_count,
        transcript,
        errors: runtime_errors,
        error_spans,
    }
}

/// core runner: compiles and executes the given slides, seeking to the target
/// timestamp within the given user slide index.
///
/// `stdlib_bundles` are prepended before the user bundle; the user bundle
/// automatically imports all of them by index.
fn user_timestamp(slide: usize, time: f64) -> Timestamp {
    Timestamp::new(slide + 1, time)
}

fn user_slide_end(slide: usize) -> Timestamp {
    Timestamp::at_end_of_slide(slide + 1)
}

fn visible_slide(timestamp: Timestamp) -> Option<usize> {
    timestamp.slide.checked_sub(1)
}

fn run_anim_impl(
    slides: &[(&str, SectionType)],
    target_slide: usize,
    target_time: f64,
    stdlib_bundles: &[Arc<SectionBundle>],
) -> AnimResult {
    let (mut executor, user_slide_count) = match build_anim_executor(slides, stdlib_bundles) {
        Ok(data) => data,
        Err(result) => return result,
    };

    let internal_target =
        executor.user_to_internal_timestamp(user_timestamp(target_slide, target_time));

    let mut runtime_errors: Vec<String> = Vec::new();
    smol::block_on(async {
        match executor.seek_to(internal_target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => runtime_errors.push(e.to_string()),
        }
    });

    collect_anim_result(executor, user_slide_count, runtime_errors)
}

fn run_anim_playback_impl(
    slides: &[(&str, SectionType)],
    start_slide: usize,
    start_time: f64,
    dt: f64,
    stdlib_bundles: &[Arc<SectionBundle>],
) -> AnimResult {
    let (mut executor, user_slide_count) = match build_anim_executor(slides, stdlib_bundles) {
        Ok(data) => data,
        Err(result) => return result,
    };

    let internal_start =
        executor.user_to_internal_timestamp(user_timestamp(start_slide, start_time));

    let mut runtime_errors = Vec::new();
    smol::block_on(async {
        match executor.seek_to(internal_start).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => {
                runtime_errors.push(e.to_string());
                return;
            }
        }

        let max_slide = executor.total_sections();
        loop {
            match executor.advance_playback(max_slide, dt).await {
                Ok(PlaybackAdvance::Advanced | PlaybackAdvance::PreparedSection) => {}
                Ok(PlaybackAdvance::Finished) => break,
                Err(e) => {
                    runtime_errors.push(e.to_string());
                    break;
                }
            }
        }
    });

    collect_anim_result(executor, user_slide_count, runtime_errors)
}

// ── public runners ────────────────────────────────────────────────────────────

/// run a single Slide section, seek to end of slide.
pub fn run_anim(src: &str) -> AnimResult {
    run_anim_impl(&[(src, SectionType::Slide)], 0, f64::INFINITY, &[])
}

/// run a single Slide section with `anim.mcl` stdlib imported, seek to end.
pub fn run_anim_with_stdlib(src: &str) -> AnimResult {
    run_anim_with_stdlib_at(src, f64::INFINITY)
}

/// run a single Slide section with `anim.mcl` stdlib imported, seek to a specific time.
pub fn run_anim_with_stdlib_at(src: &str, time: f64) -> AnimResult {
    run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        time,
        &stdlib_bundles(["anim"]),
    )
}

pub fn run_anim_with_stdlib_playback_at(src: &str, start_time: f64, dt: f64) -> AnimResult {
    run_anim_playback_impl(
        &[(src, SectionType::Slide)],
        0,
        start_time,
        dt,
        &stdlib_bundles(["anim"]),
    )
}

/// run multiple Slide sections, seeking to the given user slide and time.
pub fn run_multi_anim(slides: &[&str], target_slide: usize, target_time: f64) -> AnimResult {
    let section_slides: Vec<(&str, SectionType)> =
        slides.iter().map(|s| (*s, SectionType::Slide)).collect();
    run_anim_impl(&section_slides, target_slide, target_time, &[])
}

/// run multiple Slide sections with `anim.mcl` stdlib, seeking to the given user slide and time.
pub fn run_multi_anim_with_stdlib(
    slides: &[&str],
    target_slide: usize,
    target_time: f64,
) -> AnimResult {
    let section_slides: Vec<(&str, SectionType)> =
        slides.iter().map(|s| (*s, SectionType::Slide)).collect();
    run_anim_impl(
        &section_slides,
        target_slide,
        target_time,
        &stdlib_bundles(["anim"]),
    )
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[path = "anim_tests/errors.rs"]
mod errors;
#[path = "anim_tests/multislide.rs"]
mod multislide;
#[path = "anim_tests/regressions.rs"]
mod regressions;
#[path = "anim_tests/state_vars.rs"]
mod state_vars;
#[path = "anim_tests/sync.rs"]
mod sync;
#[path = "anim_tests/timing.rs"]
mod timing;
