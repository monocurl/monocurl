use super::*;

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
fn test_init_scene_snapshot_type_error_uses_entire_init_section_span() {
    let init_src = "
        background = [1, 1, 1]
        let keep = 1
    ";

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

        assert!(
            executor.capture_stable_scene_snapshot().await.is_err(),
            "expected scene snapshot to fail"
        );
    });

    let expected_start = init_src
        .find("background = [1, 1, 1]")
        .expect("missing background assignment");
    let expected_end =
        init_src.find("let keep = 1").expect("missing keep binding") + "let keep = 1".len();
    let expected = expected_start..expected_end;

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
        camera = Camera([0, 0, -16], [0, 0, 0], 1u)

        param radius = 1.1
        param spread = 2.5
        param spin = 0.25

        let mul = |x| x * 1r
        mesh reactive = shift{delta: mul($spread)}
            rotate{radians: $spin, axis: 1f}
            Circle(radius: $radius)

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
fn test_scene_snapshot_camera_accepts_look_at_surface() {
    let (mut executor, _user_slide_count) = match build_anim_executor(
        &[("play Set()", SectionType::Slide)],
        &stdlib_bundles(["anim"]),
    ) {
        Ok(data) => data,
        Err(result) => panic!("failed to build executor: {:?}", result.errors),
    };

    let list_value = |values: Vec<Value>| {
        Value::List(Rc::new(List::new_with(
            values.into_iter().map(VRc::new).collect(),
        )))
    };

    let mut map = Map::new();
    map.insert(
        HashableKey::String("kind".to_string()),
        VRc::new(Value::String("camera".to_string())),
    );
    map.insert(
        HashableKey::String("position".to_string()),
        VRc::new(list_value(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ])),
    );
    map.insert(
        HashableKey::String("look_at".to_string()),
        VRc::new(list_value(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(5),
        ])),
    );
    map.insert(
        HashableKey::String("up".to_string()),
        VRc::new(list_value(vec![
            Value::Integer(0),
            Value::Integer(1),
            Value::Integer(0),
        ])),
    );
    map.insert(
        HashableKey::String("near".to_string()),
        VRc::new(Value::Float(0.2)),
    );
    map.insert(
        HashableKey::String("far".to_string()),
        VRc::new(Value::Integer(50)),
    );
    let camera_value = Value::Map(Rc::new(map));

    smol::block_on(async {
        let camera = parse_camera_value(&mut executor, camera_value, "camera")
            .await
            .expect("camera parser should accept look_at surface");
        assert_eq!(camera.position.to_array(), [1.0, 2.0, 3.0]);
        assert_eq!(camera.look_at.to_array(), [1.0, 2.0, 5.0]);
        assert_eq!(camera.up.to_array(), [0.0, 1.0, 0.0]);
        assert!((camera.near - 0.2).abs() < 1e-6);
        assert!((camera.far - 50.0).abs() < 1e-6);
    });
}

#[test]
fn test_fade_accepts_stateful_live_mesh_targets() {
    let src = "
        param radius = 1.1
        param spread = 2.5

        let mul = |x| x * 1r
        mesh reactive = shift{delta: mul($spread)}
            Circle(radius: $radius)

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
            Circle(radius: $radius)

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
                shift{delta: center} Circle(2)
            )
            let r = 0.25
            let y = r * sin(theta)
            let x = r * cos(theta)
            let point = (
                fill{WHITE}
                shift{delta: center + [x, y, 0]} Circle(0.05)
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
                shift{delta: center} Circle(2)
            )

            if (theta >= 0.5) {
                let bad = sin("bad")
            }

            let r = 0.25
            let y = r * sin(theta)
            let x = r * cos(theta)
            let point = (
                fill{WHITE}
                shift{delta: center + [x, y, 0]} Circle(0.05)
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
                shift{delta: center} Circle(2)
            )
            let r = 0.25
            let y = r * sin(theta)
            let x = r * cos(theta)
            let point = (
                fill{WHITE}
                shift{delta: center + [x, y, 0]} Circle(0.05)
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

        x = Square(1)

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
fn test_transfer_subset_moves_only_matching_tags() {
    let r = run_anim_impl(
        &[
            (
                "
                mesh from = [
                    retag{1} Circle(0.5),
                    retag{2} shift{delta: [2, 0, 0]} Circle(0.5),
                    retag{3} shift{delta: [4, 0, 0]} Circle(0.5)
                ]
                mesh into = []
            ",
                SectionType::Init,
            ),
            (
                "play TransferSubset(&from, &into, [1, 3])",
                SectionType::Slide,
            ),
        ],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "mesh"]),
    );
    r.assert_ok();

    let mesh_leaders = r.mesh_leaders();
    assert_eq!(mesh_leaders.len(), 2, "expected from/into mesh leaders");
    let current_sets = sort_tag_sets(
        mesh_leaders
            .iter()
            .map(|leader| mesh_leaf_primary_tags(&leader.current))
            .collect(),
    );
    assert_eq!(current_sets, vec![vec![1, 3], vec![2]]);
}

#[test]
fn test_copy_subset_keeps_source_and_copies_predicate_match() {
    let r = run_anim_impl(
        &[
            (
                "
                mesh from = [
                    retag{1} Circle(0.5),
                    retag{2} shift{delta: [2, 0, 0]} Circle(0.5)
                ]
                mesh into = [retag{9} shift{delta: [4, 0, 0]} Circle(0.5)]
            ",
                SectionType::Init,
            ),
            (
                "play CopySubset(&from, &into, |tag| 2 in tag)",
                SectionType::Slide,
            ),
        ],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "mesh"]),
    );
    r.assert_ok();

    let mesh_leaders = r.mesh_leaders();
    assert_eq!(mesh_leaders.len(), 2, "expected from/into mesh leaders");
    let current_sets = sort_tag_sets(
        mesh_leaders
            .iter()
            .map(|leader| mesh_leaf_primary_tags(&leader.current))
            .collect(),
    );
    assert_eq!(current_sets, vec![vec![1, 2], vec![2, 9]]);
}

#[test]
fn test_grow_animates_insertions_and_deletions_symmetrically() {
    let src = r#"
        let line = |y| Polyline([[0, y, 0], [2, y, 0]])

        mesh x = [line(0), line(1)]
        play Set([&x])

        x = [line(0), line(2)]
        play Grow([&x], 1, linear)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.5,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let mut leaves = Vec::new();
    mesh_tree_leaves(&leader.current, &mut leaves);
    assert_eq!(leaves.len(), 3);

    let mut spans = leaves
        .into_iter()
        .map(|leaf| (mesh_center_y(&leaf), mesh_line_span(&leaf)))
        .collect::<Vec<_>>();
    spans.sort_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0));

    assert!((spans[0].0 - 0.0).abs() < 1e-3);
    assert!((spans[1].0 - 1.0).abs() < 1e-3);
    assert!((spans[2].0 - 2.0).abs() < 1e-3);
    assert!(
        (spans[0].1 - 2.0).abs() < 1e-3,
        "expected constant mesh to remain full length"
    );
    assert!(
        spans[1].1 > 0.05 && spans[1].1 < 1.95,
        "expected deleted mesh to be partially shrunk, got {}",
        spans[1].1
    );
    assert!(
        spans[2].1 > 0.05 && spans[2].1 < 1.95,
        "expected inserted mesh to be partially grown, got {}",
        spans[2].1
    );
}

#[test]
fn test_fade_animates_insertions_and_deletions_symmetrically() {
    let src = r#"
        let line = |y| Polyline([[0, y, 0], [2, y, 0]])

        mesh x = [line(0), line(1)]
        play Set([&x])

        x = [line(0), line(2)]
        play Fade([0, 0, 0], [&x], 1, linear)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.5,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let mut leaves = Vec::new();
    mesh_tree_leaves(&leader.current, &mut leaves);
    assert_eq!(leaves.len(), 3);

    let mut alphas = leaves
        .into_iter()
        .map(|leaf| (mesh_center_y(&leaf), mesh_max_alpha(&leaf)))
        .collect::<Vec<_>>();
    alphas.sort_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0));

    assert!((alphas[0].0 - 0.0).abs() < 1e-3);
    assert!((alphas[1].0 - 1.0).abs() < 1e-3);
    assert!((alphas[2].0 - 2.0).abs() < 1e-3);
    assert!(
        (alphas[0].1 - 1.0).abs() < 1e-3,
        "expected constant mesh to remain fully opaque"
    );
    assert!(
        alphas[1].1 > 0.2 && alphas[1].1 < 0.8,
        "expected deleted mesh to be partially faded, got {}",
        alphas[1].1
    );
    assert!(
        alphas[2].1 > 0.2 && alphas[2].1 < 0.8,
        "expected inserted mesh to be partially faded in, got {}",
        alphas[2].1
    );
}

#[test]
fn test_write_animates_insertions_and_deletions_symmetrically() {
    let src = r#"
        let line = |y| Polyline([[0, y, 0], [2, y, 0]])

        mesh x = [line(0), line(1)]
        play Set([&x])

        x = [line(0), line(2)]
        play Write([&x], 1, linear)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        0.5,
        &stdlib_bundles(["mesh", "anim"]),
    );
    r.assert_ok();

    let leader = r
        .mesh_leaders()
        .into_iter()
        .next()
        .expect("expected mesh leader");
    let mut leaves = Vec::new();
    mesh_tree_leaves(&leader.current, &mut leaves);
    assert_eq!(leaves.len(), 3);

    let mut spans = leaves
        .into_iter()
        .map(|leaf| (mesh_center_y(&leaf), mesh_line_span(&leaf)))
        .collect::<Vec<_>>();
    spans.sort_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0));

    assert!((spans[0].0 - 0.0).abs() < 1e-3);
    assert!((spans[1].0 - 1.0).abs() < 1e-3);
    assert!((spans[2].0 - 2.0).abs() < 1e-3);
    assert!(
        (spans[0].1 - 2.0).abs() < 1e-3,
        "expected constant mesh to remain full length"
    );
    assert!(
        spans[1].1 > 0.05 && spans[1].1 < 1.95,
        "expected deleted mesh to be partially unwritten, got {}",
        spans[1].1
    );
    assert!(
        spans[2].1 > 0.05 && spans[2].1 < 1.95,
        "expected inserted mesh to be partially written, got {}",
        spans[2].1
    );
}

#[test]
fn test_write_reveals_boundary_before_fill() {
    let src = r#"
        mesh x = Square(2)
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
    let mut leaves = Vec::new();
    mesh_tree_leaves(&leader.current, &mut leaves);
    assert!(!leaves.is_empty(), "expected current mesh leaves");

    let max_line_len = leaves.iter().map(mesh_line_span).fold(0.0, f32::max);
    let max_fill_alpha = leaves
        .iter()
        .map(|leaf| {
            let Value::Mesh(mesh) = leaf else {
                panic!("expected mesh leaf");
            };
            mesh.tris
                .iter()
                .map(|tri| tri.a.col.w.max(tri.b.col.w).max(tri.c.col.w))
                .fold(0.0, f32::max)
        })
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
