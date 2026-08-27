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
