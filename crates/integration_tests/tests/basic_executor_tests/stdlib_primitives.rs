use super::*;

// Coverage for the mesh.mcl primitives added in feat/stdlib-primitives:
// Angle, RightAngle, Brace, DashedLine, NumberLine.

#[test]
fn test_angle_builds_stroke_arc() {
    let r = run_with_stdlib(
        "
        let a = Angle([0, 0, 0], [1, 0, 0], [0, 1, 0])
        let result = (mesh_rank(a) == 1) + (len(mesh_vertex_set(a)) > 8)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_angle_reflex_sweeps_the_long_way() {
    let r = run_with_stdlib(
        "
        let inner = Angle([0, 0, 0], [1, 0, 0], [0, 1, 0], 0.4, 64, 0)
        let outer = Angle([0, 0, 0], [1, 0, 0], [0, 1, 0], 0.4, 64, 1)
        let inner_len = len(mesh_edge_set(inner))
        let outer_len = len(mesh_edge_set(outer))
        # both are 64-sample arcs; the reflex arc spans a wider bounding box
        let inner_w = mesh_width(inner)
        let outer_w = mesh_width(outer)
        let result = (inner_len == outer_len) + (outer_w > inner_w)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_right_angle_is_three_point_square_corner() {
    let r = run_with_stdlib(
        "
        let m = RightAngle([0, 0, 0], [1, 0, 0], [0, 1, 0], 0.2)
        let result =
            (mesh_rank(m) == 1) +
            (len(mesh_edge_set(m)) == 2) +
            (abs(mesh_width(m) - 0.2) < 0.0001) +
            (abs(mesh_height(m) - 0.2) < 0.0001)
    ",
        &["mesh", "math", "util"],
    );
    r.assert_int(4);
}

#[test]
fn test_brace_spans_endpoints_and_bulges() {
    let r = run_with_stdlib(
        "
        let b = Brace([-1, 0, 0], [1, 0, 0], 0.3)
        let result =
            (mesh_rank(b) == 1) +
            (abs(mesh_width(b) - 2) < 0.001) +
            (mesh_height(b) > 0.15)
    ",
        &["mesh", "math"],
    );
    r.assert_int(3);
}

#[test]
fn test_brace_with_label_returns_pair() {
    let r = run_with_stdlib(
        "
        let b = Brace([-1, 0, 0], [1, 0, 0], 0.3, DOWN, 101, \"w\")
        let result = len(b)
    ",
        &["mesh", "math", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_dashed_line_splits_into_dash_segments() {
    let r = run_with_stdlib(
        "
        let d = DashedLine([0, 0, 0], [2, 0, 0], [0.2, 0.1])
        let result = (mesh_rank(d) == 1) + (len(mesh_edge_set(d)) > 2)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_dashed_line_scalar_lengths() {
    let r = run_with_stdlib(
        "
        let d = DashedLine([0, 0, 0], [2, 0, 0], 0.15)
        let result = mesh_rank(d) == 1
    ",
        &["mesh", "util"],
    );
    r.assert_int(1);
}

#[test]
fn test_number_line_labels_every_tick_by_default() {
    let r = run_with_stdlib(
        "
        let line = NumberLine(0, 4, 1)
        let ticks = NumberLine(0, 4, 1, 4)
        let result = len(line) > len(ticks)
    ",
        &["mesh", "util"],
    );
    r.assert_int(1);
}

#[test]
fn test_number_line_nil_label_map_drops_labels() {
    let r = run_with_stdlib(
        "
        let labelled = NumberLine(0, 4, 1)
        let bare = NumberLine(0, 4, 1, 1, nil, nil)
        let result = len(labelled) > len(bare)
    ",
        &["mesh", "util"],
    );
    r.assert_int(1);
}

#[test]
fn test_vector_field_draws_one_arrow_per_grid_point() {
    let r = run_with_stdlib(
        "
        let field = VectorField(|p| [0 - p[1], p[0], 0], [-1, 1, 3], [-1, 1, 3], \"normalized\", 0.2)
        let result = len(field)
    ",
        &["mesh", "util"],
    );
    r.assert_int(9);
}

#[test]
fn test_vector_field_normalized_mode_makes_equal_length_arrows() {
    // all arrows point +x (magnitude varies 3..9), so normalized width should be ~constant
    let r = run_with_stdlib(
        "
        let norm_field = VectorField(|p| [(p[0] + 2) * 3, 0, 0], [-1, 1, 3], [-1, 1, 3], \"normalized\", 0.2)
        let true_field = VectorField(|p| [(p[0] + 2) * 3, 0, 0], [-1, 1, 3], [-1, 1, 3], \"true\")
        let nw = map(norm_field, |a| mesh_width(a))
        let tw = map(true_field, |a| mesh_width(a))
        let result = ((max_of(nw) - min_of(nw)) < 0.03) + ((max_of(tw) - min_of(tw)) > 0.1)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_vector_field_color_at_recolors_arrows() {
    let r = run_with_stdlib(
        "
        let field = VectorField(|p| [1, 0, 0], [-1, 1, 2], [-1, 1, 2], \"true\", 1, |p, mag| [1, 0, 0, 1])
        let result = mesh_rank(field[0]) >= 1
    ",
        &["mesh", "util"],
    );
    r.assert_int(1);
}

#[test]
fn test_explicit_func_endpoint_dots_append_dots() {
    let r = run_with_stdlib(
        "
        let plain = ExplicitFunc(|x| x * x, [-1, 1, 21])
        let dotted = ExplicitFunc(|x| x * x, [-1, 1, 21], 1)
        let result = (mesh_rank(plain) == 1) + (len(dotted) == 3) + (mesh_rank(dotted[1]) == 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_explicit_func_fill_adds_shaded_region() {
    let r = run_with_stdlib(
        "
        let shaded = ExplicitFunc(|x| 1, [0, 2, 21], 0, [0.2, 0.6, 0.9, 0.4])
        let ranks = map(shaded, |m| mesh_rank(m))
        let result = (len(shaded) == 4) + (2 in ranks) + (1 in ranks)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_parametric_func_endpoint_dots() {
    let r = run_with_stdlib(
        "
        let curve = ParametricFunc(|t| [t, t, 0], [0, 1, 16], 1)
        let result = (len(curve) == 3) + (mesh_rank(curve[1]) == 0) + (mesh_rank(curve[2]) == 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_explicit_func_splits_at_nil_samples() {
    // whole curve = 80 edges; nil-ing out the middle third drops samples and
    // breaks the polyline, so far fewer edges and no giant jump segment.
    let r = run_with_stdlib(
        "
        let whole = ExplicitFunc(|x| x, [-2, 2, 81])
        let split = ExplicitFunc(|x| { if ((x * x) < 0.09) { return nil }; return x }, [-2, 2, 81])
        let we = len(mesh_edge_set(whole))
        let se = len(mesh_edge_set(split))
        let result = (we == 80) + (se < 78) + (se > 40) + (mesh_rank(split) == 1)
    ",
        &["mesh", "util"],
    );
    r.assert_int(4);
}

#[test]
fn test_explicit_func_splits_at_non_finite_samples() {
    let r = run_with_stdlib(
        "
        let branches = ExplicitFunc(|x| sqrt(x * x - 1), [-2, 2, 81])
        let e = len(mesh_edge_set(branches))
        let result = (e > 20) + (e < 60)
    ",
        &["mesh", "util", "math"],
    );
    r.assert_int(2);
}

#[test]
fn test_explicit_func_continuous_unchanged() {
    let r = run_with_stdlib(
        "
        let parabola = ExplicitFunc(|x| x * x, [-2, 2, 41])
        let result = (len(mesh_edge_set(parabola)) == 40) + (mesh_rank(parabola) == 1)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_parametric_func_splits_at_nil_samples() {
    let r = run_with_stdlib(
        "
        let whole = ParametricFunc(|t| [t, t * t, 0], [0, 1, 51])
        let split = ParametricFunc(|t| { if ((t > 0.4) and (t < 0.6)) { return nil }; return [t, t * t, 0] }, [0, 1, 51])
        let we = len(mesh_edge_set(whole))
        let se = len(mesh_edge_set(split))
        let result = (we == 50) + (se < 48) + (se > 20)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}
