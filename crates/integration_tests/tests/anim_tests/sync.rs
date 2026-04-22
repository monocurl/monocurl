use super::*;

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
            mesh x = fill{CLEAR} stroke{RED} shift{label: ORIGIN} Circle(1)

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
        mesh x = fill{CLEAR} stroke{RED} shift{label: ORIGIN} Circle(1)

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

            mutate(shift{label: ORIGIN} Circle(1))
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
            let x = fill{CLEAR} stroke{RED} shift{label: ORIGIN} Circle(1)

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
        mesh x = shift{1r} Circle(1)
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
        play PrimitiveAnim(2, [&x], nil, |a, b, state, t| a + (b - a) * t * t, linear)
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
            ("mesh x = Circle(1)", SectionType::Init),
            (
                "
                x = Square(1)
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
                mesh x = Square(2)
                x = Circle(1)
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
                mesh x = fill{WHITE} Square(2)
                x = stroke{WHITE} fill{CLEAR} Circle(1)
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
                x = Circle(1)
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
                    shift{delta: [-1, 0, 0]} Circle(0.5),
                    shift{delta: [1, 0, 0]} Circle(0.5)
                ]
            ",
                SectionType::Init,
            ),
            (
                "
                x = [Square(1)]
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
                mesh x = Sphere(1, 0)
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
fn test_tag_trans_handles_everything_intro_badges() {
    let src = r#"
        let soft = |c, a = 0.22| with_alpha(c, a)
        let badge = |shape, color, tag = 0| retag{tag} fill{soft(color)} stroke{color} shape

        mesh intro = [
            badge(shift{delta: [-5.5, 2.6, 0]} Circle(radius: 0.7), RED, 1),
            badge(shift{delta: [-3.5, 2.6, 0]} Square(width: 1.2), BLUE, 2),
            badge(Triangle([1.5, 1.8, 0], [2.5, 3.4, 0], [3.3, 1.7, 0]), GREEN, 3),
            badge(shift{delta: [5.3, 2.6, 0]} RegularPolygon(n: 6, circumradius: 0.8), PURPLE, 4),
            retag{5} stroke{ORANGE} Arrow([-6.0, -2.6, 0], [-3.4, -2.6, 0]),
            retag{6} stroke{TEAL} shift{delta: [0, -2.6, 0]} Arc(radius: 1.15, theta: [0, 3.141592653589793]),
            retag{7} stroke{MAGENTA} Capsule([3.6, -3.0, 0], [6.2, -2.2, 0], [0.22, 0.22])
        ]

        play Set([&intro])

        intro = [
            badge(shift{delta: [-5.5, 2.6, 0]} Circle(radius: 0.78), PURPLE, 4),
            badge(shift{delta: [-1.9, 2.5, 0]} RegularPolygon(n: 5, circumradius: 0.9), RED, 1),
            badge(Capsule([0.8, 1.8, 0], [3.2, 3.0, 0], [0.28, 0.54]), BLUE, 2),
            badge(shift{delta: [5.2, 2.6, 0]} Annulus(inner: 0.34, outer: 0.82), GREEN, 3),
            retag{5} stroke{ORANGE} Arrow([-6.0, -2.4, 0], [-2.8, -2.0, 0]),
            retag{6} stroke{TEAL} shift{delta: [0, -2.5, 0]} Arc(radius: 1.3, theta: [0.2, 3.2]),
            retag{7} stroke{MAGENTA} Capsule([3.6, -3.1, 0], [6.1, -2.1, 0], [0.18, 0.55])
        ]

        play TagTrans(1.2, [&intro], 0.6 * 1u, smoother)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_tag_trans_handles_everything_intro_after_operator_rewrite() {
    let src = r#"
        let soft = |c, a = 0.22| with_alpha(c, a)
        let badge = |shape, color, tag = 0| retag{tag} fill{soft(color)} stroke{color} shape

        mesh intro = [
            badge(shift{[5.5, 2.6, 0]} Circle(0.7), RED, 1),
            badge(shift{[-3.5, 2.6, 0]} Square(1.2), BLUE, 2),
            badge(Triangle([1.5, 1.8, 0], [2.5, 3.4, 0], [3.3, 1.7, 0]), GREEN, 3),
            badge(shift{[5.3, 2.6, 0]} RegularPolygon(6, 0.8), PURPLE, 4),
            retag{5} stroke{ORANGE} Arrow([-6.0, -2.6, 0], [-3.4, -2.6, 0]),
            retag{6} stroke{TEAL} shift{[0, -2.6, 0]} Arc(1.15, [0, 3.141592653589793]),
            retag{7} stroke{MAGENTA} Capsule([3.6, -3.0, 0], [6.2, -2.2, 0], [0.22, 0.22])
        ]

        intro = point_map{|p| [p[0], p[1] + 0.25 * sin(1.7 * p[0]), p[2]]}
            color_map{|c| WHITE}
            rotate{0.35}
            scale{[1.05, 0.9, 1]}
            intro

        play Set([&intro])

        intro = [
            badge(shift{[-5.5, 2.6, 0]} Circle(0.78), PURPLE, 4),
            badge(shift{[-1.9, 2.5, 0]} RegularPolygon(5, 0.9), RED, 1),
            badge(Capsule([0.8, 1.8, 0], [3.2, 3.0, 0], [0.28, 0.54]), BLUE, 2),
            badge(shift{[5.2, 2.6, 0]} Annulus(0.34, 0.82), GREEN, 3),
            retag{5} stroke{ORANGE} Arrow([-6.0, -2.4, 0], [-2.8, -2.0, 0]),
            retag{6} stroke{TEAL} shift{[0, -2.5, 0]} Arc(1.3, [0.2, 3.2]),
            retag{7} stroke{MAGENTA} Capsule([3.6, -3.1, 0], [6.1, -2.1, 0], [0.18, 0.55])
        ]

        play TagTrans(1.2, [&intro], 0.6 * 1u, smoother)
    "#;

    let r = run_anim_impl(
        &[(src, SectionType::Slide)],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "math", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_trans_handles_square_to_capsule_badge_pair() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = fill{with_alpha(BLUE, 0.22)} stroke{BLUE} Square(width: 1.2)
                x = fill{with_alpha(BLUE, 0.22)} stroke{BLUE} Capsule([0.8, 1.8, 0], [3.2, 3.0, 0], [0.28, 0.54])
                play Trans()
            ",
            SectionType::Slide,
        )],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_trans_handles_triangle_to_annulus_badge_pair() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = fill{with_alpha(GREEN, 0.22)} stroke{GREEN} Triangle([1.5, 1.8, 0], [2.5, 3.4, 0], [3.3, 1.7, 0])
                x = shift{delta: [5.2, 2.6, 0]} fill{with_alpha(GREEN, 0.22)} stroke{GREEN} Annulus(inner: 0.34, outer: 0.82)
                play Trans()
            ",
            SectionType::Slide,
        )],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    r.assert_ok();
}

#[test]
fn test_trans_handles_capsule_to_capsule_badge_pair() {
    let r = run_anim_impl(
        &[(
            "
                mesh x = stroke{MAGENTA} Capsule([3.6, -3.0, 0], [6.2, -2.2, 0], [0.22, 0.22])
                x = stroke{MAGENTA} Capsule([3.6, -3.1, 0], [6.1, -2.1, 0], [0.18, 0.55])
                play Trans()
            ",
            SectionType::Slide,
        )],
        0,
        f64::INFINITY,
        &stdlib_bundles(["anim", "color", "mesh"]),
    );
    r.assert_ok();
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
                mesh x = RegularPolygon(9000, 1)
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
            ("mesh x = Circle(1)", SectionType::Init),
            (
                "
                x = shift{1r} Circle(1)
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
