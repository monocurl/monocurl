// animation test framework and tests
// covers: slide durations, leader values, multi-slide scenes, stdlib usage

use std::{f64, fs, path::Path, sync::Arc};

use compiler::cache::CompilerCache;
use executor::{
    error::ExecutorError,
    executor::{Executor, SeekToResult},
    heap::with_heap,
    state::LeaderKind,
    time::Timestamp,
    value::Value,
};
use lexer::{lexer::Lexer, token::Token};
use parser::{
    ast::{Section, SectionBundle, SectionType},
    parser::SectionParser,
};
use stdlib::registry::registry;
use structs::{
    assets::Assets,
    rope::{Rope, TextAggregate},
    text::Span8,
};

// ── snapshot types ────────────────────────────────────────────────────────────

pub struct LeaderInfo {
    pub kind: LeaderKind,
    /// the value the leader is animating toward (what the code last set it to)
    pub target: Value,
    /// the on-screen value at the seek point (interpolated/snapped by animations)
    pub current: Value,
}

impl LeaderInfo {
    pub fn assert_target_int(&self, expected: i64) -> &Self {
        match &self.target {
            Value::Integer(n) => assert_eq!(*n, expected, "leader target int mismatch"),
            other => panic!("expected Integer({}), got {}", expected, other.type_name()),
        }
        self
    }

    pub fn assert_current_int(&self, expected: i64) -> &Self {
        match &self.current {
            Value::Integer(n) => assert_eq!(*n, expected, "leader current int mismatch"),
            other => panic!("expected Integer({}), got {}", expected, other.type_name()),
        }
        self
    }

    pub fn assert_current_float(&self, expected: f64, eps: f64) -> &Self {
        match &self.current {
            Value::Float(f) => assert!(
                (f - expected).abs() < eps,
                "leader current float mismatch: expected {}, got {}",
                expected,
                f
            ),
            other => panic!("expected Float({}), got {}", expected, other.type_name()),
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
    pub errors: Vec<String>,
    pub error_spans: Vec<Span8>,
}

impl AnimResult {
    pub fn assert_ok(&self) -> &Self {
        assert!(
            self.errors.is_empty(),
            "expected no errors, got: {:?}",
            self.errors
        );
        self
    }

    pub fn assert_error(&self, fragment: &str) -> &Self {
        assert!(
            self.errors.iter().any(|e| e.contains(fragment)),
            "expected error containing {:?}, got: {:?}",
            fragment,
            self.errors
        );
        self
    }

    pub fn assert_user_slide_count(&self, n: usize) -> &Self {
        assert_eq!(self.user_slide_count, n, "user slide count mismatch");
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
            (actual - expected).abs() < eps,
            "slide time mismatch: expected ~{}, got {}",
            expected,
            actual
        );
        self
    }

    pub fn mesh_leaders(&self) -> Vec<&LeaderInfo> {
        self.leaders
            .iter()
            .filter(|l| l.kind == LeaderKind::Mesh)
            .collect()
    }

    pub fn param_leaders(&self) -> Vec<&LeaderInfo> {
        self.leaders
            .iter()
            .filter(|l| l.kind == LeaderKind::Param)
            .collect()
    }
}

// ── internal helpers ──────────────────────────────────────────────────────────

fn lex(src: &str) -> Vec<(Token, Span8)> {
    Lexer::token_stream(src.chars())
        .into_iter()
        .filter(|(t, _)| t != &Token::Whitespace && t != &Token::Comment)
        .collect()
}

fn parse_section(src: &str, section_type: SectionType) -> (Section, Vec<String>) {
    let tokens = lex(src);
    let rope: Rope<TextAggregate> = Rope::from_str(src);
    let mut parser = SectionParser::new(tokens, rope, section_type.clone(), None, None);
    let stmts = parser.parse_statement_list();
    let errors = parser
        .artifacts()
        .error_diagnostics
        .iter()
        .map(|e| e.message.clone())
        .collect();
    (
        Section {
            body: stmts,
            section_type,
        },
        errors,
    )
}

fn stdlib_path(name: &str) -> std::path::PathBuf {
    Assets::std_lib().join(format!("std/{name}.mcl"))
}

fn stdlib_bundle(name: &str) -> Arc<SectionBundle> {
    load_stdlib_bundle_with_import_span(stdlib_path(name), 0..0)
}

fn stdlib_bundle_with_import_span(name: &str, import_span: Span8) -> Arc<SectionBundle> {
    load_stdlib_bundle_with_import_span(stdlib_path(name), import_span)
}

fn stdlib_bundles<const N: usize>(names: [&str; N]) -> [Arc<SectionBundle>; N] {
    names.map(stdlib_bundle)
}

fn mesh_tree_leaves(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Mesh(mesh) => out.push(Value::Mesh(mesh.clone())),
        Value::List(list) => {
            for elem in list.elements() {
                let elem = with_heap(|h| h.get(elem.key()).clone());
                mesh_tree_leaves(&elem, out);
            }
        }
        _ => {}
    }
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
        .elide_wrappers(executor)
        .await
        .expect("mesh leader wrapper elision should succeed")
}

fn load_stdlib_bundle_with_import_span(
    path: impl AsRef<Path>,
    import_span: Span8,
) -> Arc<SectionBundle> {
    let src = fs::read_to_string(path).expect("failed to read stdlib file");
    let (section, errors) = parse_section(&src, SectionType::StandardLibrary);
    assert!(errors.is_empty(), "stdlib parse errors: {:?}", errors);
    Arc::new(SectionBundle {
        file_path: None,
        file_index: 0,
        imported_files: vec![],
        sections: vec![section],
        root_import_span: Some(import_span),
        was_cached: false,
    })
}

fn make_imported_bundle(
    src: &str,
    section_type: SectionType,
    import_span: Span8,
) -> Arc<SectionBundle> {
    let (section, errors) = parse_section(src, section_type);
    assert!(
        errors.is_empty(),
        "imported bundle parse errors: {:?}",
        errors
    );
    Arc::new(SectionBundle {
        file_path: None,
        file_index: 0,
        imported_files: vec![],
        sections: vec![section],
        root_import_span: Some(import_span),
        was_cached: false,
    })
}

fn build_anim_executor(
    slides: &[(&str, SectionType)],
    stdlib_bundles: &[Arc<SectionBundle>],
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
            errors: all_errors,
            error_spans: vec![],
        });
    }

    let imported_files: Vec<usize> = (0..stdlib_bundles.len()).collect();

    let user_bundle = Arc::new(SectionBundle {
        file_path: None,
        file_index: 0,
        imported_files,
        sections,
        root_import_span: None,
        was_cached: false,
    });

    let mut bundles: Vec<Arc<SectionBundle>> = stdlib_bundles.to_vec();
    bundles.push(user_bundle);

    let mut cache = CompilerCache::default();
    let result = compiler::compiler::compile(&mut cache, None, &bundles);

    let compile_errors: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
    if !compile_errors.is_empty() {
        return Err(AnimResult {
            timestamp: Timestamp::default(),
            leaders: vec![],
            user_slide_count: 0,
            errors: compile_errors,
            error_spans: vec![],
        });
    }

    let executor = Executor::new(result.bytecode, registry().func_table());
    let non_slide = executor
        .user_to_internal_timestamp(Timestamp::new(0, 0.0))
        .slide;
    let user_slide_count = executor.total_sections() - non_slide;

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

    let timestamp = executor.internal_to_user_timestamp(executor.state.timestamp);

    AnimResult {
        timestamp,
        leaders,
        user_slide_count,
        errors: runtime_errors,
        error_spans,
    }
}

/// core runner: compiles and executes the given slides, seeking to the target
/// timestamp within the given user slide index.
///
/// `stdlib_bundles` are prepended before the user bundle; the user bundle
/// automatically imports all of them by index.
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
        executor.user_to_internal_timestamp(Timestamp::new(target_slide, target_time));

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
        executor.user_to_internal_timestamp(Timestamp::new(start_slide, start_time));

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
                Ok(true) => {}
                Ok(false) => break,
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

// -- wait animation duration (via stdlib) --

#[test]
fn test_wait_duration_one_second() {
    let r = run_anim_with_stdlib("play Wait(1)");
    r.assert_ok().assert_slide_time_approx(1.0, 1e-9);
}

#[test]
fn test_wait_duration_fractional() {
    let r = run_anim_with_stdlib("play Wait(1.5)");
    r.assert_ok().assert_slide_time_approx(1.5, 1e-9);
}

#[test]
fn test_wait_default_duration() {
    // Wait default time = 1
    let r = run_anim_with_stdlib("play Wait()");
    r.assert_ok().assert_slide_time_approx(1.0, 1e-9);
}

#[test]
fn test_wait_three_seconds() {
    // positional arg — labeled calls return InvokedFunction which play doesn't accept
    let r = run_anim_with_stdlib("play Wait(3)");
    r.assert_ok().assert_slide_time_approx(3.0, 1e-9);
}

#[test]
fn test_wait_sequential_total_duration() {
    // two sequential waits: total = 1 + 2 = 3
    let r = run_anim_with_stdlib(
        "
        play Wait(1)
        play Wait(2)
    ",
    );
    r.assert_ok().assert_slide_time_approx(3.0, 1e-9);
}

#[test]
fn test_wait_sequential_playback_keeps_leftover_dt() {
    let r = run_anim_with_stdlib_playback_at(
        "
        play Wait(0.01)
        play Wait(0.02)
        play Wait(0.03)
    ",
        0.0,
        0.03,
    );
    r.assert_ok().assert_slide_time_approx(0.06, 1e-9);
}

#[test]
fn test_wait_sequential_playback_from_off_grid_start_keeps_true_end_time() {
    let r = run_anim_with_stdlib_playback_at(
        "
        play Wait(0.01)
        play Wait(0.02)
        play Wait(0.03)
    ",
        0.0234234,
        0.03,
    );
    r.assert_ok().assert_slide_time_approx(0.06, 1e-9);
}

#[test]
fn test_wait_nested_playback_keeps_leftover_dt_across_resumed_parent() {
    let r = run_anim_with_stdlib_playback_at(
        "
        let nested = anim {
            play Wait(0.01)
            play Wait(0.02)
        }
        play nested
        play Wait(0.03)
    ",
        0.0,
        0.04,
    );
    r.assert_ok().assert_slide_time_approx(0.06, 1e-9);
}

#[test]
fn test_wait_parallel_playback_keeps_leftover_dt_until_all_heads_finish() {
    let r = run_anim_with_stdlib_playback_at(
        "
        play [Wait(0.01), Wait(0.05)]
        play Wait(0.02)
    ",
        0.0,
        0.03,
    );
    r.assert_ok().assert_slide_time_approx(0.07, 1e-9);
}

#[test]
fn test_wait_parallel_playback_from_off_grid_start_keeps_true_end_time() {
    let r = run_anim_with_stdlib_playback_at(
        "
        play [Wait(0.01), Wait(0.05)]
        play Wait(0.02)
    ",
        0.0234234,
        0.03,
    );
    r.assert_ok().assert_slide_time_approx(0.07, 1e-9);
}

#[test]
fn test_no_animation_duration_is_zero() {
    let r = run_anim("let x = 42");
    r.assert_ok().assert_slide_time_approx(0.0, 1e-9);
}

// -- anim block --

#[test]
fn test_anim_block_duration() {
    let r = run_anim_with_stdlib(
        "
        play anim {
            play Wait(2.5)
        }
    ",
    );
    r.assert_ok().assert_slide_time_approx(2.5, 1e-9);
}

#[test]
fn test_anim_blocks_played_in_loop_are_sequential() {
    let r = run_anim_with_stdlib(
        "
        for (i in [1, 2, 3]) {
            play anim {
                play Wait(i)
            }
        }
    ",
    );
    r.assert_ok().assert_slide_time_approx(6.0, 1e-9);
}

#[test]
fn test_anim_block_list_built_in_loop_plays_in_parallel() {
    let r = run_anim_with_stdlib(
        "
        var blocks = []
        for (i in [1, 2, 3]) {
            blocks .= anim {
                play Wait(i)
            }
        }
        play blocks
    ",
    );
    r.assert_ok().assert_slide_time_approx(3.0, 1e-9);
}

#[test]
fn test_nested_anim_blocks_accumulate_duration() {
    let r = run_anim_with_stdlib(
        "
        play anim {
            play anim {
                play Wait(1)
                play anim {
                    play Wait(2)
                }
            }
            play Wait(3)
        }
    ",
    );
    r.assert_ok().assert_slide_time_approx(6.0, 1e-9);
}

#[test]
fn test_anim_blocks_generated_from_lambdas() {
    let r = run_anim_with_stdlib(
        "
        let make_wait = |t| anim {
            play Wait(t)
        }
        play make_wait(1.5)
        play make_wait(2.25)
    ",
    );
    r.assert_ok().assert_slide_time_approx(3.75, 1e-9);
}

// -- seeking to a specific time within a slide --

#[test]
fn test_seek_mid_wait() {
    // seek to 1s into a 3s wait — stopped at time 1
    let r = run_anim_with_stdlib_at("play Wait(3)", 1.0);
    r.assert_ok().assert_slide_time_approx(1.0, 1e-9);
}

#[test]
fn test_seek_past_end_clamps_to_last_event() {
    // seeking past the last animation snaps to its end
    let r = run_anim_with_stdlib_at("play Wait(2)", f64::INFINITY);
    r.assert_ok().assert_slide_time_approx(2.0, 1e-9);
}

// -- state variables (leaders) --

#[test]
fn test_state_leader_count_prelude_only() {
    // no user-defined leaders → only camera + background from prelude
    let r = run_anim("let x = 1");
    r.assert_ok();
    assert_eq!(
        r.param_leaders().len(),
        2,
        "expected camera and background from prelude"
    );
    assert_eq!(r.mesh_leaders().len(), 0);
}

#[test]
fn test_state_leader_initial_value() {
    let r = run_anim("param score = 42");
    r.assert_ok();
    let params = r.param_leaders();
    let user_leader = params.last().expect("no param leaders found");
    user_leader.assert_target_int(42).assert_current_int(42);
}

#[test]
fn test_multiple_state_leaders() {
    let r = run_anim(
        "
        param a = 10
        param b = 20
    ",
    );
    r.assert_ok();
    assert_eq!(r.param_leaders().len(), 4);
    let params = r.param_leaders();
    params[2].assert_target_int(10);
    params[3].assert_target_int(20);
}

#[test]
fn test_param_leader() {
    let r = run_anim("param speed = 5");
    r.assert_ok();
    assert_eq!(r.param_leaders().len(), 3);
    r.param_leaders()[2]
        .assert_target_int(5)
        .assert_current_int(5);
}

// -- set / lerp --

#[test]
fn test_set_syncs_only_explicit_candidates() {
    let r = run_anim_with_stdlib(
        "
        param a = 1
        param b = 2
        a = 10
        b = 20
        play Set([&a])
    ",
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2].assert_target_int(10).assert_current_int(10);
    params[3].assert_target_int(20).assert_current_int(2);
}

#[test]
fn test_set_has_minimum_positive_duration() {
    let r = run_anim_with_stdlib(
        "
        param x = 0
        x = 10
        play Set([&x])
    ",
    );
    r.assert_ok()
        .assert_slide_time_approx(f64::MIN_POSITIVE, f64::MIN_POSITIVE);
}

#[test]
fn test_set_slide_can_seek_back_to_zero_after_finishing() {
    let (mut executor, user_slide_count) = match build_anim_executor(
        &[(
            "
            param x = 0
            x = 10
            play Set([&x])
        ",
            SectionType::Slide,
        )],
        &stdlib_bundles(["anim"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("executor should build, got errors: {:?}", result.errors),
    };

    smol::block_on(async {
        let end = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(end).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }

        let start = executor.user_to_internal_timestamp(Timestamp::new(0, 0.0));
        match executor.seek_to(start).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
    });

    let r = collect_anim_result(executor, user_slide_count, vec![]);
    r.assert_ok()
        .assert_slide_time_approx(0.0, f64::MIN_POSITIVE);
    r.param_leaders()[2]
        .assert_target_int(10)
        .assert_current_int(0);
}

#[test]
fn test_mesh_label_mutation_after_set_then_lerp() {
    let src = "
            mesh x = fill{CLEAR} stroke{RED} Circle(label: ORIGIN, 1)

            play Set()

            x.label = 2l

            play Lerp()
        ";
    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "math", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_mesh_label_mutation_after_set_then_lerp_elides_wrappers() {
    let src = "
        mesh x = fill{CLEAR} stroke{RED} Circle(label: ORIGIN, 1)

        play Set()

        x.label = 2l

        play Lerp()
    ";

    let (mut executor, _) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["anim", "color", "math", "mesh"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("executor should build, got errors: {:?}", result.errors),
    };

    smol::block_on(async {
        let end = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(end).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }

        let entry = executor
            .state
            .leaders
            .iter()
            .find(|entry| entry.kind == executor::state::LeaderKind::Mesh)
            .expect("expected mesh leader");
        let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
        let Value::Leader(leader) = cell_val else {
            panic!("mesh leader entry is not a Leader value");
        };

        let _ = with_heap(|h| h.get(leader.leader_rc.key()).clone())
            .elide_wrappers(&mut executor)
            .await
            .expect("leader wrapper elision should succeed");
        let _ = with_heap(|h| h.get(leader.follower_rc.key()).clone())
            .elide_wrappers(&mut executor)
            .await
            .expect("follower wrapper elision should succeed");
    });
}

#[test]
fn test_ref_mutation_of_live_function_argument_does_not_panic() {
    let r = run_anim_impl(
        &[(
            "
            let mutate = |&y| {
                y.label = 2l
                return []
            }

            mutate(Circle(label: ORIGIN, 1))
        ",
            SectionType::Slide,
        )],
        0,
        f64::INFINITY,
        &stdlib_bundles(["color", "math", "mesh"]),
    );
    assert!(
        r.errors
            .iter()
            .all(|error| !error.contains("Expected Lvalue")),
        "executor should not panic with force_elide_lvalue: {:?}",
        r.errors
    );
}

#[test]
fn test_lerp_of_mesh_operator_variants_after_label_mutation() {
    let r = run_anim_impl(
        &[(
            "
            let x = fill{CLEAR} stroke{RED} Circle(label: ORIGIN, 1)

            var y = x
            y.label = 2l

            let z = lerp(x, y, 0.5)
        ",
            SectionType::Slide,
        )],
        0,
        f64::INFINITY,
        &stdlib_bundles(["color", "math", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_stroke_operator_lerp_blends_from_identity_embed() {
    let src = "
        mesh x = shift{1r} Circle([0, 0, 0], 1)
        x = stroke{RED} x
        play Lerp()
    ";

    let (mut executor, _) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["anim", "color", "mesh"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("executor should build, got errors: {:?}", result.errors),
    };

    let current = smol::block_on(async {
        let mid = executor.user_to_internal_timestamp(Timestamp::new(0, 0.5));
        match executor.seek_to(mid).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
        current_mesh_leader_value(&mut executor).await
    });

    let Value::Mesh(mesh) = &current else {
        panic!("expected current mesh value");
    };

    let sample = mesh.lins.first().expect("expected stroked line");
    assert!(
        sample.a.col.x > 0.05 && sample.a.col.x < 0.95,
        "expected interpolated stroke color, got {:?}",
        sample.a.col.to_array()
    );

    let avg_x = mesh
        .lins
        .iter()
        .flat_map(|lin| [lin.a.pos.x, lin.b.pos.x])
        .sum::<f32>()
        / (mesh.lins.len() as f32 * 2.0);
    assert!(
        avg_x > 0.5,
        "expected shifted geometry to stay materialized, got avg x {avg_x}"
    );
}

#[test]
fn test_point_map_operator_lerp_blends_from_identity_embed() {
    let src = "
        mesh x = shift{1r} Dot()
        x = point_map{|p| p + 2r} x
        play Lerp()
    ";

    let (mut executor, _) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["anim", "mesh"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("executor should build, got errors: {:?}", result.errors),
    };

    let current = smol::block_on(async {
        let mid = executor.user_to_internal_timestamp(Timestamp::new(0, 0.5));
        match executor.seek_to(mid).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
        current_mesh_leader_value(&mut executor).await
    });

    let Value::Mesh(mesh) = &current else {
        panic!("expected current mesh value");
    };

    let dot = mesh.dots.first().expect("expected mapped dot");
    assert!(
        (dot.pos.x - 2.0).abs() < 1e-3,
        "expected midpoint point-map x of 2, got {}",
        dot.pos.x
    );
}

#[test]
fn test_lerp_auto_deduces_detached_followers() {
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        x = 10
        play Lerp(2)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(10)
        .assert_current_float(5.0, 1e-9);
}

#[test]
fn test_lerp_flattens_nested_candidate_tree() {
    let r = run_anim_with_stdlib_at(
        "
        param a = 0
        param b = 2
        a = 10
        b = 20
        play Lerp(2, [[&a], []])
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(10)
        .assert_current_float(5.0, 1e-9);
    params[3].assert_target_int(20).assert_current_int(2);
}

#[test]
fn test_lerp_rate_lambda_shapes_progression() {
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        x = 10
        play Lerp(2, [&x], |t| t * t)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(10)
        .assert_current_float(2.5, 1e-9);
}

#[test]
fn test_lerp_custom_lerp_lambda_shapes_value_interpolation() {
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        x = 10
        play PrimitiveAnim(2, [&x], linear, nil, |a, b, state, t| a + (b - a) * t * t)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(10)
        .assert_current_float(2.5, 1e-9);
}

#[test]
fn test_trans_anim_interpolates_meshes_without_generic_mesh_lerp() {
    let r = run_anim_impl(
        &[
            ("mesh x = Circle()", SectionType::Init),
            (
                "
                x = Square()
                play Trans()
            ",
                SectionType::Slide,
            ),
        ],
        0,
        0.5,
        &stdlib_bundles(["anim", "color", "math", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_trans_square_to_circle_midpoint_keeps_boundary_off_origin() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = Square([0, 0, 0], 2)
                x = Circle([0, 0, 0], 1)
                play Trans()
            ",
            SectionType::Slide,
        )],
        0,
        0.5,
        &stdlib_bundles(["anim", "mesh"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh value");
    };
    let min_radius_sq = mesh
        .lins
        .iter()
        .flat_map(|lin| [lin.a.pos, lin.b.pos])
        .map(|point| point.x * point.x + point.y * point.y + point.z * point.z)
        .fold(f32::INFINITY, f32::min);
    assert!(
        min_radius_sq > 0.01,
        "boundary collapsed too close to the origin: {}",
        min_radius_sq.sqrt()
    );
}

#[test]
fn test_trans_filled_square_to_clear_circle_fades_fill() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = fill{WHITE} Square([0, 0, 0], 2)
                x = stroke{WHITE} fill{CLEAR} Circle([0, 0, 0], 1)
                play Trans()
            ",
            SectionType::Slide,
        )],
        0,
        0.5,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh value");
    };
    let alpha = mesh
        .tris
        .first()
        .expect("expected interpolated fill triangles")
        .a
        .col
        .w;
    assert!(
        alpha > 0.05 && alpha < 0.95,
        "expected mid-fade fill alpha, got {alpha}"
    );
}

#[test]
fn test_trans_from_empty_source_fades_target_in_place() {
    let r = run_anim_impl(
        &[
            ("mesh x = []", SectionType::Init),
            (
                "
                x = Circle([0, 0, 0], 1)
                play Trans()
            ",
                SectionType::Slide,
            ),
        ],
        0,
        0.5,
        &stdlib_bundles(["anim", "mesh"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let mut meshes = Vec::new();
    mesh_tree_leaves(&leader.current, &mut meshes);
    assert_eq!(meshes.len(), 1, "expected one interpolated target mesh");

    let Value::Mesh(mesh) = &meshes[0] else {
        panic!("expected mesh leaf");
    };
    let min_radius_sq = mesh
        .lins
        .iter()
        .flat_map(|lin| [lin.a.pos, lin.b.pos])
        .map(|point| point.x * point.x + point.y * point.y + point.z * point.z)
        .fold(f32::INFINITY, f32::min);
    assert!(
        min_radius_sq > 0.01,
        "target geometry collapsed instead of fading in place"
    );
    assert!(
        mesh.uniform.alpha > 0.05 && mesh.uniform.alpha < 0.95,
        "expected mid-fade alpha, got {}",
        mesh.uniform.alpha
    );
}

#[test]
fn test_trans_with_more_source_leaves_keeps_all_pairs_mid_animation() {
    let r = run_anim_impl(
        &[
            (
                "
                mesh x = [
                    Circle([-1, 0, 0], 0.5),
                    Circle([1, 0, 0], 0.5)
                ]
            ",
                SectionType::Init,
            ),
            (
                "
                x = [Square([0, 0, 0], 1)]
                play Trans()
            ",
                SectionType::Slide,
            ),
        ],
        0,
        0.5,
        &stdlib_bundles(["anim", "mesh"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let mut meshes = Vec::new();
    mesh_tree_leaves(&leader.current, &mut meshes);
    assert_eq!(
        meshes.len(),
        2,
        "old trans semantics keep one matched pair per larger-side leaf"
    );
}

#[test]
fn test_trans_keeps_larger_surface_topology_when_source_is_more_detailed() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = Sphere([0, 0, 0], 1, 0)
                x = Triangle([0, 0, 0], [1, 0, 0], [0, 1, 0])
                play Trans()
            ",
            SectionType::Slide,
        )],
        0,
        0.5,
        &stdlib_bundles(["anim", "mesh"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh value");
    };

    assert!(
        mesh.tris.len() > 100,
        "expected larger source surface topology to be retained, got {} triangles",
        mesh.tris.len()
    );
}

#[test]
fn test_color_grid_lambda_arity_error_is_reported_without_panicking() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = ColorGrid(|pos| {})
            ",
            SectionType::Slide,
        )],
        0,
        0.0,
        &stdlib_bundles(["mesh"]),
    );
    r.assert_error("too many positional arguments");
}

#[test]
fn test_color_grid_triangle_limit_is_reported() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = ColorGrid(|pos, idx| [1, 0, 0, 1], [-1, 1, 400], [-1, 1, 400])
            ",
            SectionType::Slide,
        )],
        0,
        0.0,
        &stdlib_bundles(["mesh"]),
    );
    r.assert_error("color grid cells is too large");
}

#[test]
fn test_regular_polygon_limit_is_reported() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = RegularPolygon([0, 0, 0], 9000, 1)
            ",
            SectionType::Slide,
        )],
        0,
        0.0,
        &stdlib_bundles(["mesh"]),
    );
    r.assert_error("regular polygon sides is too large");
}

#[test]
fn test_math_stdlib_no_longer_exports_inf() {
    let r = run_anim_impl(
        &[("let x = INF", SectionType::Slide)],
        0,
        0.0,
        &stdlib_bundles(["math"]),
    );
    r.assert_error("undefined");
}

#[test]
fn test_bend_anim_interpolates_polyline_meshes() {
    let r = run_anim_impl(
        &[
            (
                "mesh x = Polyline([[0, 0, 0], [1, 0, 0], [2, 0, 0]])",
                SectionType::Init,
            ),
            (
                "
                x = Polyline([[0, 0, 0], [0, 1, 0], [0, 2, 0]])
                play Bend()
            ",
                SectionType::Slide,
            ),
        ],
        0,
        0.5,
        &stdlib_bundles(["anim", "color", "math", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_fade_anim_materializes_live_operator_meshes() {
    let r = run_anim_impl(
        &[
            ("mesh x = Circle()", SectionType::Init),
            (
                "
                x = shift{1r} Circle()
                play Fade()
            ",
                SectionType::Slide,
            ),
        ],
        0,
        0.5,
        &stdlib_bundles(["anim", "color", "math", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_parallel_anim_blocks_auto_target_only_own_stack_lineage() {
    let r = run_anim_with_stdlib_at(
        "
        param a = 0
        param b = 0
        let a_anim = anim {
            a = 4
            play Lerp()
        }
        let b_anim = anim {
            b = 4
            play Set()
        }
        play [a_anim, b_anim]
    ",
        0.5,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(4)
        .assert_current_float(2.0, 1e-9);
    params[3].assert_target_int(4).assert_current_int(4);
}

#[test]
fn test_parallel_anim_blocks_with_shared_root_changes_leave_later_implicit_anim_empty() {
    let r = run_anim_with_stdlib_at(
        "
        param a = 0
        param b = 0

        a = 4
        b = 4

        let a_anim = anim {
            play Lerp()
        }
        let b_anim = anim {
            play Set()
        }
        play [a_anim, b_anim]
    ",
        0.5,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(4)
        .assert_current_float(2.0, 1e-9);
    params[3]
        .assert_target_int(4)
        .assert_current_float(2.0, 1e-9);
}

#[test]
fn test_anim_block_auto_targets_ancestor_and_local_changes() {
    let r = run_anim_with_stdlib_at(
        "
        param a = 0
        param b = 0

        a = 4

        let child = anim {
            b = 6
            play Lerp(2)
        }

        play child
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(4)
        .assert_current_float(2.0, 1e-9);
    params[3]
        .assert_target_int(6)
        .assert_current_float(3.0, 1e-9);
}

#[test]
fn test_concurrent_primitive_animation_lock_error() {
    let r = run_anim_with_stdlib(
        "
        param x = 0
        x = 10
        play [Lerp(1, [&x]), Set([&x])]
    ",
    );
    r.assert_error("concurrent animation");
}

// -- multi-slide --

#[test]
fn test_multi_slide_count() {
    let r = run_multi_anim(&["let x = 1", "let y = 2"], 0, f64::INFINITY);
    r.assert_ok().assert_user_slide_count(2);
}

#[test]
fn test_multi_slide_seek_first_slide() {
    let r = run_multi_anim_with_stdlib(&["play Wait(2)", "play Wait(5)"], 0, f64::INFINITY);
    r.assert_ok();
    assert_eq!(r.timestamp.slide, 0, "should remain on slide 0");
    r.assert_slide_time_approx(2.0, 1e-9);
}

#[test]
fn test_multi_slide_seek_second_slide() {
    let r = run_multi_anim_with_stdlib(&["play Wait(1)", "play Wait(3)"], 1, f64::INFINITY);
    r.assert_ok();
    assert_eq!(r.timestamp.slide, 1, "should be on slide 1");
    r.assert_slide_time_approx(3.0, 1e-9);
}

#[test]
fn test_multi_slide_state_persists_across_slides() {
    // param variables declared in slide 0 remain visible in slide 1
    let r = run_multi_anim(&["param counter = 99", "let check = 1"], 1, f64::INFINITY);
    r.assert_ok();
    let params = r.param_leaders();
    params[2].assert_target_int(99);
}

// -- error cases --

#[test]
fn test_wait_negative_time_error() {
    // Wait validates that time ≥ 0
    let r = run_anim_with_stdlib("play Wait(-1)");
    r.assert_error("non-negative");
}

#[test]
fn test_imported_stdlib_runtime_error_uses_root_callsite_span() {
    let import_span = 1000..1006;
    let src = "play Wait(-1)";
    let anim_mcl = stdlib_bundle_with_import_span("anim", import_span.clone());
    let r = run_anim_impl(&[(src, SectionType::Slide)], 0, f64::INFINITY, &[anim_mcl]);
    r.assert_error("non-negative");
    assert!(!r.error_spans.is_empty(), "expected runtime error span");
    assert_ne!(
        r.error_spans[0], import_span,
        "error should not use import span"
    );
    assert!(
        r.error_spans[0].end <= src.len(),
        "expected callsite span inside root source, got {:?}",
        r.error_spans[0]
    );
}

#[test]
fn test_runtime_error_prefers_innermost_root_callsite_span() {
    let src = "
        let create = || Wait(-1)
        play create()
    ";
    let r = run_anim_with_stdlib(src);
    r.assert_error("non-negative");
    let wait_start = src
        .find("Wait(-1)")
        .expect("missing Wait(-1) in test source");
    let expected = wait_start..wait_start + "Wait(-1)".len();
    assert!(!r.error_spans.is_empty(), "expected runtime error span");
    assert_eq!(r.error_spans[0], expected);
}

#[test]
fn test_imported_init_runtime_error_uses_import_span_when_no_root_frame_exists() {
    let import_span = 2000..2006;
    let imported = make_imported_bundle("let x = 1 / 0", SectionType::Init, import_span.clone());
    let r = run_anim_impl(
        &[("let y = 1", SectionType::Slide)],
        0,
        f64::INFINITY,
        &[imported],
    );
    r.assert_error("division by zero");
    assert!(!r.error_spans.is_empty(), "expected runtime error span");
    assert_eq!(r.error_spans[0], import_span);
}

#[test]
fn test_root_recorded_error_uses_latest_root_statement_span() {
    let src = "
        background = 0
    ";

    let (mut executor, _user_slide_count) =
        match build_anim_executor(&[(src, SectionType::Slide)], &[]) {
            Ok(data) => data,
            Err(result) => panic!("failed to build executor: {:?}", result.errors),
        };

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
    });

    executor.record_runtime_error_at_root(ExecutorError::invalid_operation("test"));

    let expected_start = src
        .find("background = 0")
        .expect("missing background assignment");
    let expected = expected_start..expected_start + "background = 0".len();

    let runtime_error = executor
        .state
        .errors
        .last()
        .expect("expected recorded runtime error");
    assert_eq!(runtime_error.span, expected);
}

#[test]
fn test_root_recorded_error_uses_latest_prior_root_section_span() {
    let init_src = "background = 0";

    let (mut executor, _user_slide_count) = match build_anim_executor(
        &[(init_src, SectionType::Init), ("", SectionType::Slide)],
        &[],
    ) {
        Ok(data) => data,
        Err(result) => panic!("failed to build executor: {:?}", result.errors),
    };

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
    });

    executor.record_runtime_error_at_root(ExecutorError::invalid_operation("test"));

    let expected_start = init_src
        .find("background = 0")
        .expect("missing background assignment");
    let expected = expected_start..expected_start + "background = 0".len();

    let runtime_error = executor
        .state
        .errors
        .last()
        .expect("expected recorded runtime error");
    assert_eq!(runtime_error.span, expected);
}

#[test]
fn test_scene_snapshot_error_after_play_uses_play_span() {
    let src = "
        background = 0
        play Set()
    ";

    let (mut executor, _user_slide_count) =
        match build_anim_executor(&[(src, SectionType::Slide)], &stdlib_bundles(["anim"])) {
            Ok(data) => data,
            Err(result) => panic!("failed to build executor: {:?}", result.errors),
        };

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }

        assert!(
            executor.capture_stable_scene_snapshot().await.is_err(),
            "expected scene snapshot to fail"
        );
    });

    let expected_start = src.find("play Set()").expect("missing play Set()");
    let expected = expected_start..expected_start + "play Set()".len();

    let runtime_error = executor
        .state
        .errors
        .last()
        .expect("expected recorded runtime error");
    assert_eq!(runtime_error.span, expected);
}

#[test]
fn test_scene_snapshot_materializes_stateful_live_mesh_values() {
    let src = "
        camera = Camera([0, 0, -16], 1f, 1u)

        param radius = 1.1
        param spread = 2.5
        param spin = 0.25

        let mul = |x| x * 1r
        mesh reactive = shift{delta: mul($spread)}
            rotate{radians: $spin, axis: 1f}
            Circle([0, 0, 0], radius: $radius)

        play Set([&reactive])

        radius = 1.75
        spread = 5.0
        spin = 1.8
        play Lerp(1.3, [&reactive])
    ";

    let (mut executor, _user_slide_count) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["anim", "math", "mesh", "scene"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("failed to build executor: {:?}", result.errors),
    };

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }

        let snapshot = executor
            .capture_stable_scene_snapshot()
            .await
            .expect("scene snapshot should succeed");
        assert!(
            !snapshot.meshes.is_empty(),
            "scene snapshot should include the reactive mesh"
        );
    });
}

#[test]
fn test_fade_accepts_stateful_live_mesh_targets() {
    let src = "
        param radius = 1.1
        param spread = 2.5

        let mul = |x| x * 1r
        mesh reactive = shift{delta: mul($spread)}
            Circle([0, 0, 0], radius: $radius)

        play Set([&reactive])

        radius = 1.75
        spread = 5.0
        play Fade([-1, 0, 0], [&reactive], 1.0, smooth)
    ";

    let (mut executor, _user_slide_count) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["anim", "math", "mesh"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("failed to build executor: {:?}", result.errors),
    };

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
    });
}

#[test]
fn test_custom_lerp_accepts_stateful_live_mesh_targets() {
    let src = "
        param radius = 1.1
        param spread = 2.5

        let mul = |x| x * 1r
        mesh reactive = shift{delta: mul($spread)}
            Circle([0, 0, 0], radius: $radius)

        play Set([&reactive])

        radius = 1.75
        spread = 5.0
        play PrimitiveAnim(1.0, [&reactive], smooth, nil, |a, b, state, t| b)
    ";

    let (mut executor, _user_slide_count) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["anim", "math", "mesh"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("failed to build executor: {:?}", result.errors),
    };

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("unexpected seek error: {e}"),
        }
    });
}

#[test]
fn test_anim_played_twice_error() {
    let r = run_anim_with_stdlib(
        "
        let w = anim { play Wait(1) }
        play w
        play w
    ",
    );
    r.assert_error("already played");
}

#[test]
fn test_lerp_of_live_mesh_lambda_survives_assignment_chain() {
    let src = r#"
        let entity = |center, theta| {
            let outline = (
                stroke{WHITE}
                fill{CLEAR}
                Circle(center, 2)
            )
            let r = 0.25
            let y = r * sin(theta)
            let x = r * cos(theta)
            let point = (
                fill{WHITE}
                Circle(center + [x, y, 0], 0.05)
            )
            return [outline, point]
        }

        mesh left = entity(2l, theta: 0)
        mesh right = entity(2r, theta: 0)

        let duration = 10
        # was causing issues before
        left.theta = right.theta = duration * 4

        play Lerp(duration, [], identity)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        f64::INFINITY,
        &stdlib_bundles(["util", "math", "mesh", "color", "anim"]),
    );
    r.assert_ok();
}

#[test]
fn test_lerp_live_mesh_lambda_error_callstack_starts_at_play_site() {
    let src = r#"
        let entity = |center, theta| {
            let outline = (
                stroke{WHITE}
                fill{CLEAR}
                Circle(center, 2)
            )

            if (theta >= 0.5) {
                let bad = sin("bad")
            }

            let r = 0.25
            let y = r * sin(theta)
            let x = r * cos(theta)
            let point = (
                fill{WHITE}
                Circle(center + [x, y, 0], 0.05)
            )
            return [outline, point]
        }

        mesh left = entity(2l, theta: 0)
        left.theta = 1

        play Lerp(1, [], identity)
    "#;

    let (mut executor, _user_slide_count) = match build_anim_executor(
        &[(src, SectionType::Slide)],
        &stdlib_bundles(["util", "math", "mesh", "color", "anim"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("failed to build executor: {:?}", result.errors),
    };

    let internal_target = executor.user_to_internal_timestamp(Timestamp::new(0, 0.0));
    smol::block_on(async {
        let _ = executor.seek_to(internal_target).await;
        let _ = executor
            .advance_playback(executor.total_sections(), 0.5)
            .await;
    });

    let runtime_error = executor
        .state
        .errors
        .first()
        .expect("expected runtime error");
    let play_start = src.find("play Lerp").expect("missing play Lerp");
    let expected = play_start..play_start + "play Lerp(1, [], identity)".len();

    assert!(
        !runtime_error.callstack.is_empty(),
        "expected recovered callstack"
    );
    assert!(
        runtime_error
            .callstack
            .iter()
            .any(|frame| frame.span == expected),
        "expected play site in recovered callstack, got {:?}",
        runtime_error
            .callstack
            .iter()
            .map(|frame| frame.span.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_lerp_live_mesh_length_mismatch_uses_play_span() {
    let src = r#"
        let entity = |center, theta| {
            let outline = (
                stroke{WHITE}
                fill{CLEAR}
                Circle(center, 2)
            )
            let r = 0.25
            let y = r * sin(theta)
            let x = r * cos(theta)
            let point = (
                fill{WHITE}
                Circle(center + [x, y, 0], 0.05)
            )
            return [outline, point]
        }

        mesh left = entity(2l, theta: 0)
        mesh right = entity(2r, theta: 0)

        let duration = 10
        left.theta = right.theta = duration * 4

        play Lerp(duration, [], identity)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        5.0,
        &stdlib_bundles(["util", "math", "mesh", "color", "anim"]),
    );

    let play_start = src.find("play Lerp").expect("missing play Lerp");
    let expected = play_start..play_start + "play Lerp(duration, [], identity)".len();
    r.assert_error("cannot lerp lists of different lengths");
    assert!(!r.error_spans.is_empty(), "expected runtime error span");
    assert_eq!(r.error_spans[0], expected);
}

#[test]
fn test_write_polyline_preserves_authored_line_links() {
    let src = r#"
        mesh x = Polyline([[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 0, 0]])
        play Write()
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.25,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();
}

#[test]
fn test_write_then_trans_polyline_to_square() {
    let src = r#"
        mesh x = Polyline([[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 0, 0]])

        play Write()

        x = Square()

        play Trans()
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        f64::INFINITY,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();
}

#[test]
fn test_write_staggers_across_mesh_leaves() {
    let src = r#"
        mesh x = [
            Polyline([[0, 0, 0], [2, 0, 0]]),
            Polyline([[0, 1, 0], [2, 1, 0]])
        ]
        play Write()
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.05,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let mut meshes = Vec::new();
    mesh_tree_leaves(&leader.current, &mut meshes);
    assert_eq!(meshes.len(), 2);

    let Value::Mesh(first) = &meshes[0] else {
        panic!("expected first mesh leaf");
    };
    let Value::Mesh(second) = &meshes[1] else {
        panic!("expected second mesh leaf");
    };

    let first_len = (first.lins[0].b.pos - first.lins[0].a.pos).len();
    let second_len = (second.lins[0].b.pos - second.lins[0].a.pos).len();
    assert!(
        first_len > 0.01,
        "expected first leaf to have started writing"
    );
    assert!(
        second_len < 1e-6,
        "expected second leaf to still be hidden, got length {second_len}"
    );
}

#[test]
fn test_write_reveals_boundary_before_fill() {
    let src = r#"
        mesh x = Square([0, 0, 0], 2)
        play Write()
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.15,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh");
    };

    let max_line_len = mesh
        .lins
        .iter()
        .map(|lin| (lin.b.pos - lin.a.pos).len())
        .fold(0.0, f32::max);
    let max_fill_alpha = mesh
        .tris
        .iter()
        .map(|tri| tri.a.col.w.max(tri.b.col.w).max(tri.c.col.w))
        .fold(0.0, f32::max);

    assert!(max_line_len > 0.01, "expected boundary to be visible");
    assert!(
        max_fill_alpha < 1e-6,
        "expected fill to remain hidden early in write, got {max_fill_alpha}"
    );
}

#[test]
fn test_delay_operator_wraps_animation_in_wait_block() {
    let r = run_anim_impl(
        &[(
            "
                play delay{0.5} Wait(1)
            ",
            SectionType::Slide,
        )],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim"]),
    );
    r.assert_ok().assert_slide_time_approx(1.5, 1e-9);
}

#[test]
fn test_highlight_composes_set_and_lerp_over_reference_target() {
    let src = "
        mesh x = stroke{RED} Line([0, 0, 0], [1, 0, 0])
        play Highlight(&x, BLUE, 1)
    ";

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.5,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh");
    };

    let line = mesh.lins.first().expect("expected highlighted line");
    assert!(
        line.a.col.x > 0.05 && line.a.col.x < 0.95,
        "expected red channel to be mid-fade, got {:?}",
        line.a.col.to_array()
    );
    assert!(
        line.a.col.z > 0.05 && line.a.col.z < 0.95,
        "expected blue channel to be mid-fade, got {:?}",
        line.a.col.to_array()
    );
}

#[test]
fn test_flash_composes_write_and_trailing_lerp_over_reference_target() {
    let src = "
        mesh x = stroke{RED} Line([0, 0, 0], [1, 0, 0])
        play Flash(&x, 1)
    ";

    let mid = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.75,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    mid.assert_ok();

    let leader = mid
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh");
    };
    let line = mesh.lins.first().expect("expected flashed line");
    assert!(
        line.a.col.w > 0.05 && line.a.col.w < 0.95,
        "expected flash trail to be partially faded, got {:?}",
        line.a.col.to_array()
    );

    let end = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    end.assert_ok();

    let leader = end
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let Value::Mesh(mesh) = &leader.current else {
        panic!("expected current mesh");
    };
    let line = mesh.lins.first().expect("expected flashed line");
    assert!(
        (line.a.col.w - 1.0).abs() < 1e-6,
        "expected flash to restore original alpha, got {:?}",
        line.a.col.to_array()
    );
}

// -- regression: while loop before play --

#[test]
fn test_wait_duration_after_while_loop() {
    // while loop before play should not affect animation timing
    // x(1) = 1 * 2 = 2; total = Wait(2) + Wait(2) = 4
    let r = run_anim_with_stdlib(
        "
        let x = |y| y * 2
        var i = 0
        while (i < 100) {
            i = i + 1
        }
        play Wait(x(1))
        play Wait(2)
    ",
    );
    r.assert_ok().assert_slide_time_approx(4.0, 1e-9);
}

#[test]
fn test_wait_duration_cross_section_lambda_with_while_loop() {
    // x is defined in an Init section; the Slide references it after a while loop
    let r = run_anim_impl(
        &[
            ("let x = |y| y * 2", SectionType::Init),
            (
                "
                var i = 0
                while (i < 100) {
                    i = i + 1
                }
                play Wait(x(1))
                play Wait(2)
            ",
                SectionType::Slide,
            ),
        ],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim"]),
    );
    r.assert_ok().assert_slide_time_approx(4.0, 1e-9);
}
