// animation test framework and tests
// covers: slide durations, leader values, multi-slide scenes, stdlib usage

use std::{f64, fs, path::Path, sync::Arc};

use compiler::cache::CompilerCache;
use executor::{
    executor::{Executor, SeekToResult},
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

    pub fn assert_target_float(&self, expected: f64, eps: f64) -> &Self {
        match &self.target {
            Value::Float(f) => assert!(
                (f - expected).abs() < eps,
                "leader target float mismatch: expected {}, got {}",
                expected,
                f
            ),
            other => panic!("expected Float({}), got {}", expected, other.type_name()),
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

    pub fn assert_leader_count(&self, n: usize) -> &Self {
        assert_eq!(self.leaders.len(), n, "leader count mismatch");
        self
    }

    pub fn state_leaders(&self) -> Vec<&LeaderInfo> {
        self.leaders
            .iter()
            .filter(|l| l.kind == LeaderKind::State)
            .collect()
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

fn load_stdlib_bundle(path: impl AsRef<Path>) -> Arc<SectionBundle> {
    load_stdlib_bundle_with_import_span(path, 0..0)
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
            let leader_val = entry.leader_cell_rc.borrow();
            let Value::Leader(leader) = &*leader_val else {
                panic!("leader entry is not a Leader value");
            };
            LeaderInfo {
                kind: entry.kind,
                target: leader.leader_rc.borrow().clone(),
                current: leader.follower_rc.borrow().clone(),
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

/// run a single Slide section, seek to a specific time.
pub fn run_anim_at(src: &str, time: f64) -> AnimResult {
    run_anim_impl(&[(src, SectionType::Slide)], 0, time, &[])
}

/// run a single Slide section with `anim.mcl` stdlib imported, seek to end.
pub fn run_anim_with_stdlib(src: &str) -> AnimResult {
    run_anim_with_stdlib_at(src, f64::INFINITY)
}

/// run a single Slide section with `anim.mcl` stdlib imported, seek to a specific time.
pub fn run_anim_with_stdlib_at(src: &str, time: f64) -> AnimResult {
    let anim_mcl = load_stdlib_bundle(Assets::std_lib().join("std/anim.mcl"));
    run_anim_impl(&[(src, SectionType::Slide)], 0, time, &[anim_mcl])
}

pub fn run_anim_with_stdlib_playback_at(src: &str, start_time: f64, dt: f64) -> AnimResult {
    let anim_mcl = load_stdlib_bundle(Assets::std_lib().join("std/anim.mcl"));
    run_anim_playback_impl(&[(src, SectionType::Slide)], 0, start_time, dt, &[anim_mcl])
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
    let anim_mcl = load_stdlib_bundle(Assets::std_lib().join("std/anim.mcl"));
    let section_slides: Vec<(&str, SectionType)> =
        slides.iter().map(|s| (*s, SectionType::Slide)).collect();
    run_anim_impl(&section_slides, target_slide, target_time, &[anim_mcl])
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
        r.state_leaders().len(),
        2,
        "expected camera and background from prelude"
    );
    assert_eq!(r.mesh_leaders().len(), 0);
    assert_eq!(r.param_leaders().len(), 0);
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
    assert_eq!(r.param_leaders().len(), 2);
    let params = r.param_leaders();
    params[0].assert_target_int(10);
    params[1].assert_target_int(20);
}

#[test]
fn test_param_leader() {
    let r = run_anim("param speed = 5");
    r.assert_ok();
    assert_eq!(r.param_leaders().len(), 1);
    r.param_leaders()[0]
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
    params[0].assert_target_int(10).assert_current_int(10);
    params[1].assert_target_int(20).assert_current_int(2);
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
    params[0]
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
    params[0]
        .assert_target_int(10)
        .assert_current_float(5.0, 1e-9);
    params[1].assert_target_int(20).assert_current_int(2);
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
    assert!(
        !params.is_empty(),
        "expected param leader to persist across slides"
    );
    params[0].assert_target_int(99);
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
    let anim_mcl = load_stdlib_bundle_with_import_span(
        Assets::std_lib().join("std/anim.mcl"),
        import_span.clone(),
    );
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
    let anim_mcl = load_stdlib_bundle(structs::assets::Assets::std_lib().join("std/anim.mcl"));
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
        &[anim_mcl],
    );
    r.assert_ok().assert_slide_time_approx(4.0, 1e-9);
}
