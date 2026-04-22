use super::*;

fn flatten_mesh_leaves(value: &Value, out: &mut Vec<std::sync::Arc<geo::mesh::Mesh>>) {
    match elide_value_for_assert(value) {
        Value::Mesh(mesh) => out.push(mesh.clone()),
        Value::List(list) => {
            for child in list.elements() {
                let value = with_heap(|h| h.get(child.key()).clone());
                flatten_mesh_leaves(&value, out);
            }
        }
        other => panic!("expected mesh tree, got {}", other.type_name()),
    }
}

fn mesh_signature(mesh: &geo::mesh::Mesh) -> String {
    let dots = mesh
        .dots
        .iter()
        .map(|dot| format!("{:.6},{:.6},{:.6}", dot.pos.x, dot.pos.y, dot.pos.z))
        .collect::<Vec<_>>()
        .join(";");
    let lins = mesh
        .lins
        .iter()
        .map(|lin| {
            format!(
                "{:.6},{:.6},{:.6}|{:.6},{:.6},{:.6}",
                lin.a.pos.x, lin.a.pos.y, lin.a.pos.z, lin.b.pos.x, lin.b.pos.y, lin.b.pos.z
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let tris = mesh
        .tris
        .iter()
        .map(|tri| {
            format!(
                "{:.6},{:.6},{:.6}|{:.6},{:.6},{:.6}|{:.6},{:.6},{:.6}",
                tri.a.pos.x,
                tri.a.pos.y,
                tri.a.pos.z,
                tri.b.pos.x,
                tri.b.pos.y,
                tri.b.pos.z,
                tri.c.pos.x,
                tri.c.pos.y,
                tri.c.pos.z
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    format!("dots[{dots}]|lins[{lins}]|tris[{tris}]")
}

// -- COW: list element independence after aliasing --

#[test]
fn test_cow_list_mutation_doesnt_affect_alias() {
    // a[0] = 99 must not bleed into b; they share Rc elements until the write triggers COW
    let r = run("
        var a = [1, 2, 3]
        var b = a
        a[0] = 99
        let result = b[0]
    ");
    r.assert_int(1);
}

#[test]
fn test_cow_list_alias_mutation_doesnt_affect_original() {
    let r = run("
        var a = [10, 20, 30]
        var b = a
        b[2] = 77
        let result = a[2]
    ");
    r.assert_int(30);
}

#[test]
fn test_cow_list_both_aliases_mutate_independently() {
    let r = run("
        var a = [1, 2, 3]
        var b = a
        a[0] = 100
        b[0] = 200
        let result = a[0] + b[0]
    ");
    r.assert_int(300);
}

#[test]
fn test_cow_list_nested_alias_chain() {
    // a → b → c all start sharing element Rcs; mutation to c must not affect a
    let r = run("
        var a = [5, 6, 7]
        var b = a
        var c = b
        c[1] = 99
        let result = a[1]
    ");
    r.assert_int(6);
}

// -- labeled function invocations --
#[test]
fn test_labeled_elide() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(myarg: 10, 30)
        let result = inv + 10
    ");
    r.assert_int(50);
}

#[test]
fn test_labeled_recompute() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(myarg: 10, 30)
        let org = 0 + inv
        inv.myarg = 30
        let full = org + inv
    ");
    r.assert_int(100);
}

#[test]
fn test_labeled_read_first_arg() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(myarg: 10, 30)
        let result = inv.myarg
    ");
    r.assert_int(10);
}

#[test]
fn test_labeled_read_second_arg() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(10, second: 30)
        let result = inv.second
    ");
    r.assert_int(30);
}

#[test]
fn test_labeled_both_args_readable() {
    let r = run("
        let f = |a, b| a - b
        let inv = f(lhs: 50, rhs: 8)
        let result = inv.lhs - inv.rhs
    ");
    r.assert_int(42);
}

#[test]
fn test_labeled_mutate_arg() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        inv.lbl = 5
        let result = inv.lbl
    ");
    r.assert_int(5);
}

#[test]
fn test_labeled_default_arg_is_readable() {
    let r = run("
        let f = |x, y = 100| x + y
        let inv = f(lbl: 7)
        let result = inv.lbl
    ");
    r.assert_int(7);
}

#[test]
fn test_labeled_error_on_unknown_label() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(known: 1, 2)
        let result = inv.unknown_label
    ");
    r.assert_error("no labeled argument");
}

// -- COW on InvokedFunction: mutating one copy must not affect the other --

#[test]
fn test_cow_invoked_function_mutation_leaves_alias_intact() {
    // alias and inv start sharing the same Rc<InvokedFunction>;
    // mutating inv.lbl triggers Rc::make_mut (COW) so alias is unchanged
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        let alias = inv
        inv.lbl = 99
        let result = alias.lbl
    ");
    r.assert_int(10);
}

#[test]
fn test_cow_invoked_function_mutated_copy_has_new_value() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        let _alias = inv
        inv.lbl = 77
        let result = inv.lbl
    ");
    r.assert_int(77);
}

#[test]
fn test_labeled_nested_live_elision_in_arithmetic() {
    let r = run("
        let inner = |x, y| x + y
        let outer = |seed| inner(lhs: seed * 2, rhs: 5)
        let result = outer(seed: 7) + 3
    ");
    r.assert_int(22);
}

#[test]
fn test_labeled_nested_mutation_recomputes_live_value() {
    let r = run("
        let inner = |x, y| x + y
        let outer = |seed| inner(lhs: seed * 2, rhs: 5)
        var inv = outer(seed: 7)
        inv.seed = 10
        let result = inv + 3
    ");
    r.assert_int(28);
}

#[test]
fn test_labeled_aliases_keep_independent_live_results() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        let alias = inv
        inv.lbl = 99
        let result = alias + inv
    ");
    r.assert_int(169);
}

#[test]
fn test_live_function_structural_equality() {
    // same labeled invocation is structurally equal
    let r = run("
        let f = |x, y| x + y
        let result = f(lhs: 8, rhs: 4) == f(lhs: 8, rhs: 4)
    ");
    r.assert_int(1);
}

#[test]
fn test_live_function_structural_inequality() {
    // different args → not equal, even if computed result would be the same
    let r = run("
        let f = |x| x * 2
        let result = f(a: 3) == f(a: 6)
    ");
    r.assert_int(0);
}

#[test]
fn test_live_function_not_equal_to_primitive() {
    // a live function invocation is structurally different from a plain integer
    let r = run("
        let f = |x, y| x + y
        let result = f(lhs: 8, rhs: 4) == 12
    ");
    r.assert_int(0);
}

#[test]
fn test_live_elision_supports_negation() {
    let r = run("
        let f = |x, y| x - y
        let result = -f(lhs: 5, rhs: 8)
    ");
    r.assert_int(3);
}

#[test]
fn test_live_elision_recomputes_defaulted_labeled_invocation() {
    let r = run("
        let f = |x, y = 100| x + y
        var inv = f(lbl: 7)
        inv.lbl = 20
        let result = inv + inv.lbl
    ");
    r.assert_int(140);
}

#[test]
fn test_util_attr_helpers_on_live_function() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        inv = set_attr(inv, \"lbl\", 25)
        let result = has_attr(inv, \"lbl\") * 100 + has_attr(inv, \"missing\") * 10 + get_attr(inv, \"lbl\")
    ",
        &["util"],
    );
    r.assert_int(125);
}

#[test]
fn test_util_attr_helpers_on_live_operator_delegate_to_operand() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        let passthrough = operator |target, amount| [target, target]
        let inv = passthrough{amount: 2} f(lbl: 40, 2)
        let updated = set_attr(inv, \"lbl\", 50)
        let result = has_attr(updated, \"lbl\") * 100 + get_attr(updated, \"lbl\")
    ",
        &["util"],
    );
    r.assert_int(150);
}

#[test]
fn test_util_type_predicates_cover_callable_variants() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        let op = operator |target| [target, target]
        let live_f = f(arg: 1, 2)
        let live_op = op{} 1
        let result = is_float(1.5)
            + is_number(2)
            + is_list([1, 2])
            + is_function(f)
            + is_function(live_f)
            + is_operator(op)
            + is_operator(live_op)
            + is_callable(f)
            + is_callable(live_op)
            + is_live_function(live_f)
            + is_live_operator(live_op)
    ",
        &["util"],
    );
    r.assert_int(11);
}

#[test]
fn test_util_type_predicates_cover_mesh_and_primitive_anim() {
    let r = run_with_stdlib(
        "
        let result = is_mesh(Dot()) + is_primitive_anim(PrimitiveAnim())
    ",
        &["util", "mesh", "anim"],
    );
    r.assert_int(2);
}

#[test]
fn test_util_type_of_and_runtime_error() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        let result = type_of(f(lbl: 1, 2))
    ",
        &["util"],
    );
    r.assert_string("live function");

    let err = run_with_stdlib("runtime_error(\"boom\")", &["util"]);
    err.assert_error("boom");
}

#[test]
fn test_mesh_stdlib_reports_named_bad_list_argument() {
    let r = run_with_stdlib(
        "
        let result = ColorGrid(|pos| [1, 0, 0, 1], 5)
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 'x_min_max_samples'");
    r.assert_error("expected list of length 3");
    r.assert_error("got int");
}

#[test]
fn test_mesh_operator_filter_applies_predicate_to_subset() {
    let r = run_with_stdlib(
        "
        let scene = [
            retag{1} Circle(1),
            retag{2} shift{delta: [4, 0, 0]} Circle(1)
        ]
        let shifted = shift{delta: 10 * 1r, filter: |tag| 1 in tag} scene
        let x1 = mesh_center(tag_filter{1} shifted)[0]
        let x2 = mesh_center(tag_filter{2} shifted)[0]
        let result = (abs(x1 - 10) < 0.001) + (abs(x2 - 4) < 0.001)
    ",
        &["mesh", "math"],
    );
    r.assert_int(2);
}

#[test]
fn test_subset_map_applies_mapping_to_matching_subset() {
    let r = run_with_stdlib(
        "
        let scene = [
            retag{1} Circle(1),
            retag{2} shift{delta: [4, 0, 0]} Circle(1)
        ]
        let shifted = subset_map{filter: |tag| 1 in tag, f: |m| shift{delta: 10 * 1r} m} scene
        let x1 = mesh_center(tag_filter{1} shifted)[0]
        let x2 = mesh_center(tag_filter{2} shifted)[0]
        let result = (abs(x1 - 10) < 0.001) + (abs(x2 - 4) < 0.001)
    ",
        &["mesh", "math"],
    );
    r.assert_int(2);
}

#[test]
fn test_tag_split_is_exposed_in_mesh_stdlib() {
    let r = run_with_stdlib(
        "
        let scene = [
            retag{1} Circle(1),
            retag{2} shift{delta: [4, 0, 0]} Circle(1)
        ]
        let all = tag_split(scene)
        let filtered = tag_split(scene, |tag| 2 in tag)
        let result =
            (mesh_contour_count(all[0]) == 2) +
            (mesh_contour_count(all[1]) == 0) +
            (mesh_center(filtered[0])[0] > 0) +
            (mesh_center(filtered[1])[0] < 1)
    ",
        &["mesh"],
    );
    r.assert_int(4);
}

#[test]
fn test_contour_separate_operator_numbers_output_tags() {
    let r = run_with_stdlib(
        "
        let scene = [
            retag{8} shift{delta: [-2, 0, 0]} Circle(1),
            retag{9} shift{delta: [2, 0, 0]} Circle(1)
        ]
        let separated = contour_separate{} scene
        let left = tag_filter{0} separated
        let right = tag_filter{1} separated
        let result =
            (mesh_center(left)[0] < 0) +
            (mesh_center(right)[0] > 0) +
            (len(mesh_tags(left)) == 1) +
            (len(mesh_tags(right)) == 1) +
            (0 in mesh_tags(left)) +
            (1 in mesh_tags(right))
    ",
        &["mesh", "util"],
    );
    r.assert_int(6);
}

#[test]
fn test_tag_filter_operator_reads_filtered_side() {
    let r = run_with_stdlib(
        "
        let scene = [
            retag{1} shift{delta: [-2, 0, 0]} Circle(1),
            retag{2} shift{delta: [2, 0, 0]} Circle(1)
        ]
        let filtered = tag_filter{|tag| 2 in tag} scene
        let result =
            (mesh_center(filtered)[0] > 0) +
            (len(mesh_tags(filtered)) == 1) +
            (2 in mesh_tags(filtered))
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_to_side_and_to_corner_smoke() {
    let r = run_with_stdlib(
        "
        let cam = Camera([0, 0, -10], [0, 0, 0], 1u)
        let side = mesh_center(to_side{cam, 1r} Circle(1))
        let corner = mesh_center(to_corner{cam, [1, 1, 0], 0.1} Circle(1))
        let result = (side[0] > 0) + (corner[0] > 0) + (corner[1] > 0)
    ",
        &["mesh", "scene"],
    );
    r.assert_int(3);
}

#[test]
fn test_to_side_and_to_corner_use_default_camera_when_omitted() {
    let r = run_with_stdlib(
        "
        let side = mesh_center(to_side{[1, 0, 0]} Circle(1))
        let side_buffered = mesh_center(to_side{[1, 0, 0], 0.1} Circle(1))
        let corner = mesh_center(to_corner{[1, 1, 0], 0.1} Circle(1))
        let result = (side[0] > 0) + (side_buffered[0] > 0) + (corner[0] > 0) + (corner[1] > 0)
    ",
        &["mesh"],
    );
    r.assert_int(4);
}

#[test]
fn test_label_places_latex_to_requested_side() {
    let r = run_with_stdlib(
        "
        let target = Circle(1)
        let right = Label(target, \"A\", 1, 1r)
        let up = Label(target, \"B\", 1, 1u)
        let result = (mesh_center(right)[0] > mesh_right(target)[0]) + (mesh_center(up)[1] > mesh_up(target)[1])
    ",
        &["mesh"],
    );
    r.assert_int(2);
}

#[test]
fn test_label_preserves_cross_axis_alignment() {
    let r = run_with_stdlib(
        "
        let target = shift{delta: [2, 3, 0]} Circle(1)
        let left = Label(target, \"C\", 1, 1l)
        let result = abs(mesh_center(left)[1] - mesh_center(target)[1]) < 0.001
    ",
        &["mesh", "math"],
    );
    r.assert_int(1);
}

#[test]
fn test_label_matches_latex_next_to_geometry() {
    let r = run_with_stdlib(
        "
        let target = Circle(1)
        let label = Label(target, \"C\", 1, 1l)
        let reference = next_to{target, 1l, 0.1} Latex(\"C\", 1)
        let result = [label, reference]
    ",
        &["mesh"],
    );
    r.assert_ok();

    let value = r.value.as_ref().expect("expected result value");
    let Value::List(pair) = value else {
        panic!("expected [label, reference], got {}", value.type_name());
    };
    assert_eq!(pair.elements().len(), 2, "expected exactly two mesh trees");

    let label_value = with_heap(|h| h.get(pair.elements()[0].key()).clone());
    let reference_value = with_heap(|h| h.get(pair.elements()[1].key()).clone());

    let mut label_meshes = Vec::new();
    flatten_mesh_leaves(&label_value, &mut label_meshes);
    let mut reference_meshes = Vec::new();
    flatten_mesh_leaves(&reference_value, &mut reference_meshes);

    assert_eq!(
        label_meshes.len(),
        reference_meshes.len(),
        "label/reference leaf count mismatch"
    );

    for (label_mesh, reference_mesh) in label_meshes.iter().zip(reference_meshes.iter()) {
        assert_eq!(
            mesh_signature(label_mesh),
            mesh_signature(reference_mesh),
            "label geometry diverged from latex next_to reference"
        );
    }
}

#[test]
fn test_tex_and_latex_accept_list_string_inputs() {
    let r = run_with_stdlib(
        "
        let tex = Tex([\"2\", \" + \", 4], 1)
        let latex = Latex([\"$\", \"x^2\", \"$\"], 1)
        let result = (len(mesh_triangle_set(tex)) > 0) + (len(mesh_triangle_set(latex)) > 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_fixed_in_frame_preserves_camera_space_under_translation() {
    let r = run_with_stdlib(
        "
        let original = Camera([0, 0, -10], [0, 0, 0], 1u)
        let live = Camera([2, 3, -10], [2, 3, 0], 1u)
        let result = mesh_center(fixed_in_frame{original, live} Dot([1, 0, 0]))
    ",
        &["mesh", "scene"],
    );
    r.assert_float_list_approx(&[3.0, 3.0, 0.0], 1e-6);
}

#[test]
fn test_fixed_in_frame_preserves_camera_space_under_orbit() {
    let r = run_with_stdlib(
        "
        let original = Camera([0, 0, -10], [0, 0, 0], 1u)
        let live = Camera([10, 0, 0], [0, 0, 0], 1u)
        let result = mesh_center(fixed_in_frame{original, live} Dot([1, 0, 0]))
    ",
        &["mesh", "scene"],
    );
    r.assert_float_list_approx(&[0.0, 0.0, 1.0], 1e-5);
}

#[test]
fn test_capsule_accepts_scalar_and_equal_pair_radii() {
    let r = run_with_stdlib(
        "
        let scalar = len(mesh_triangle_set(Capsule([0, 0, 0], [2, 0, 0], 0.4)))
        let pair = len(mesh_triangle_set(Capsule([0, 0, 0], [2, 0, 0], [0.4, 0.4])))
        let result = (scalar > 0) + (pair > 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_explicit_func_diff_accepts_custom_tags() {
    let r = run_with_stdlib(
        "
        let f = |x| 1
        let g = |x| 0
        let fill0 = [0.3, 0.8, 0.3, 0.5]
        let fill1 = [0.8, 0.3, 0.3, 0.5]
        let fills = [fill0, fill1]
        let custom_tags = [7, 9]
        let diff = ExplicitFuncDiff(f, g, [-1, 1, 16], fills, custom_tags)
        let tags = sort(mesh_tags(diff))
        let result = (len(tags) == 2) + (tags[0] == 7) + (tags[1] == 9)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_explicit_func_diff_connects_same_sign_strip() {
    let r = run_with_stdlib(
        "
        let diff = ExplicitFuncDiff(|x| 1, |x| 0, [-1, 1, 6])
        let pos = diff[0]
        let neg = diff[1]
        let result = (len(mesh_triangle_set(pos)) == 10) + (len(mesh_edge_set(pos)) == 21) + (len(mesh_triangle_set(neg)) == 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_extrude_square_authors_consistent_closed_surface() {
    let r = run_with_stdlib(
        "
        let solid = extrude{delta: [0, 0, 1]} Square(1)
        let result = (mesh_rank(solid) == 2) + (len(mesh_triangle_set(solid)) == 12)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_parametric_func_reports_named_bad_sample_range_argument() {
    let r = run_with_stdlib(
        "
        let result = ParametricFunc(|t| [t, 0, 0], 5)
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 't_min_max_samples'");
    r.assert_error("expected list of length 3");
    r.assert_error("got int");
}

#[test]
fn test_explicit_func_reports_named_bad_sample_range_argument() {
    let r = run_with_stdlib(
        "
        let result = ExplicitFunc(|x| x, 5)
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 'x_min_max_samples'");
    r.assert_error("expected list of length 3");
    r.assert_error("got int");
}

#[test]
fn test_mesh_stdlib_reports_named_bad_list_length() {
    let r = run_with_stdlib(
        "
        let result = Rect([1, 2, 3])
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 'size'");
    r.assert_error("expected list of length 2");
    r.assert_error("got list of length 3");
}

#[test]
fn test_color_stdlib_reports_named_bad_color_argument() {
    let r = run_with_stdlib(
        "
        let result = with_alpha(7, 0.5)
    ",
        &["color"],
    );
    r.assert_error("invalid argument 'color'");
    r.assert_error("expected list of length 4");
    r.assert_error("got int");
}

#[test]
fn test_field_uses_sample_counts_and_index_callback() {
    let r = run_with_stdlib(
        "
        let result = Field(|pos, idx| idx[0] * 10 + idx[1], [0, 1, 3], [0, 1, 2])
    ",
        &["mesh"],
    );
    r.assert_int_list(&[0, 1, 10, 11, 20, 21]);
}

#[test]
fn test_color_grid_uses_sample_counts() {
    let r = run_with_stdlib(
        "
        let result = len(mesh_triangle_set(ColorGrid(|pos, idx| [1, 0, 0, 1], [0, 1, 3], [0, 1, 4])))
    ",
        &["mesh", "util"],
    );
    r.assert_int(12);
}

#[test]
fn test_explicit_func2d_preserves_open_surface_boundary_topology() {
    let r = run_with_stdlib(
        "
        let surf = ExplicitFunc2d(|x, y| x * y, [0, 1, 4], [0, 1, 3])
        let result = (len(mesh_triangle_set(surf)) == 12) + (len(mesh_edge_set(surf)) > 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_parametric_func_sample_limit_is_reported() {
    let r = run_with_stdlib(
        "
        let result = ParametricFunc(|t| [t, 0, 0], [0, 1, 20000])
    ",
        &["mesh"],
    );
    r.assert_error("parametric samples is too large");
}

#[test]
fn test_mesh_collapse_flattens_tree_into_one_mesh() {
    let r = run_with_stdlib(
        "
        let result = mesh_center(mesh_collapse([Line([0, 0, 0], [1, 0, 0]), Line([2, 0, 0], [3, 0, 0])]))
    ",
        &["mesh"],
    );
    r.assert_float_list_approx(&[1.5, 0.0, 0.0], 1e-9);
}

#[test]
fn test_mesh_trans_helper_interpolates_without_animation_context() {
    let r = run_with_stdlib(
        "
        let result = mesh_center(trans(Dot([0, 0, 0]), Dot([2, 0, 0]), 0.5))
    ",
        &["mesh"],
    );
    r.assert_float_list_approx(&[1.0, 0.0, 0.0], 1e-9);
}

#[test]
fn test_rotate_operator_uses_angle_axis_and_optional_pivot() {
    let r = run_with_stdlib(
        "
        let result = mesh_center(rotate{1.5707963267948966, 1f, [0, 0, 0]} Dot([1, 0, 0]))
    ",
        &["mesh"],
    );
    r.assert_float_list_approx(&[0.0, 1.0, 0.0], 1e-5);
}

#[test]
fn test_camera_stdlib_uses_look_at_surface() {
    let r = run_with_stdlib(
        "
        let cam = Camera([1, 2, 3], [1, 2, 5], [0, 1, 0], 0.2, 50)
        let result = [cam[\"position\"], cam[\"look_at\"], cam[\"near\"], cam[\"far\"]]
    ",
        &["scene"],
    );
    r.assert_ok();
    match &r.value {
        Some(Value::List(list)) => {
            let elems = list.elements();
            match with_heap(|h| h.get(elems[0].key()).clone()) {
                Value::List(position) => {
                    let coords: Vec<_> = position
                        .elements()
                        .iter()
                        .map(|elem| with_heap(|h| h.get(elem.key()).clone()))
                        .collect();
                    assert!(matches!(coords[0], Value::Integer(1)));
                    assert!(matches!(coords[1], Value::Integer(2)));
                    assert!(matches!(coords[2], Value::Integer(3)));
                }
                other => panic!("expected camera position list, got {}", other.type_name()),
            }
            match with_heap(|h| h.get(elems[1].key()).clone()) {
                Value::List(look_at) => {
                    let coords: Vec<_> = look_at
                        .elements()
                        .iter()
                        .map(|elem| with_heap(|h| h.get(elem.key()).clone()))
                        .collect();
                    assert!(matches!(coords[0], Value::Integer(1)));
                    assert!(matches!(coords[1], Value::Integer(2)));
                    assert!(matches!(coords[2], Value::Integer(5)));
                }
                other => panic!("expected camera look_at list, got {}", other.type_name()),
            }
            assert!(matches!(
                with_heap(|h| h.get(elems[2].key()).clone()),
                Value::Float(f) if (f - 0.2).abs() < 1e-9
            ));
            assert!(matches!(
                with_heap(|h| h.get(elems[3].key()).clone()),
                Value::Float(f) if (f - 50.0).abs() < 1e-9
            ));
        }
        other => panic!(
            "expected camera surface list, got {}",
            other.as_ref().map(Value::type_name).unwrap_or("(empty)")
        ),
    }
}

// -- stack overflow --
