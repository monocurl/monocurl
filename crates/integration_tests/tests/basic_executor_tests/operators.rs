use super::*;

#[test]
fn test_exec_operator_creation_and_invocation() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let x = 40
        let result = add{2} x
        print result
    ");
    r.assert_transcript(&["42"]);
}

#[test]
fn test_exec_operator_creation() {
    let r = run("
        let result = operator |target, amount| {
            return [target, target + amount]
        }
    ");
    r.assert_ok();
    match &r.value {
        Some(Value::Operator(_)) => {}
        other => panic!(
            "expected operator, got {}",
            other.as_ref().map(Value::type_name).unwrap_or("(empty)")
        ),
    }
}

#[test]
fn test_exec_operator_chain_invocation() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        let x = 10
        let result = add{2} mul{3} x
        print result
    ");
    r.assert_transcript(&["32"]);
}

#[test]
fn test_exec_operator_chain_with_aliases() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        let outer = add
        let inner = mul
        let x = 10
        let result = outer{2} inner{3} x
        print result
    ");
    r.assert_transcript(&["32"]);
}

#[test]
fn test_exec_operator_chain_same_operator_multiple_times() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let x = 10
        let result = add{2} add{3} add{4} x
        print result
    ");
    r.assert_transcript(&["19"]);
}

#[test]
fn test_exec_operator_may_return_live_operator() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let wrap = operator |target, amount| {
            return add{amount} target
        }
        let x = 40
        let result = wrap{2} x
        print result
    ");
    r.assert_transcript(&["42"]);
}

#[test]
fn test_exec_labeled_operator_arg_readable() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let inv = add{amount: 2} 40
        let result = inv.amount
    ");
    r.assert_int(2);
}

#[test]
fn test_exec_labeled_operator_arg_mutable() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        var inv = add{amount: 2} 40
        inv.amount = 5
        let result = inv.amount
    ");
    r.assert_int(5);
}

#[test]
fn test_exec_labeled_operator_repeated_attribute_alias_uses_last_assignment() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        var inv = add{amount: 2} 40
        [inv.amount, inv.amount] = [5, 7]
        let result = inv.amount
    ");
    r.assert_int(7);
}

#[test]
fn test_exec_labeled_operator_retains_attribute_lvalue_invalidated_by_base_assignment() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        var inv = add{amount: 2} 40
        [inv.amount, inv, inv.amount] = [5, add{amount: 7} 40, 9]
        let result = inv.amount
    ");
    r.assert_int(7);
}

#[test]
fn test_exec_mesh_leader_labeled_attribute_mutable() {
    let r = run_section(
        "
        let hello = |origin, radius| origin + radius
        mesh base = hello(origin: 10, radius: 2)
        base.origin = 45
        let result = base.origin
    ",
        SectionType::Slide,
    );
    r.assert_int(45);
}

#[test]
fn test_exec_mesh_leader_labeled_attribute_lvalue_survives_base_assignment() {
    let r = run_section(
        "
        let hello = |origin, radius| origin + radius
        mesh base = hello(origin: 10, radius: 2)
        base.origin = base = hello(origin: 20, radius: 6)
        let result = [base.origin, base.radius]
    ",
        SectionType::Slide,
    );
    r.assert_int_list(&[20, 6]);
}

#[test]
fn test_exec_mesh_leader_labeled_attribute_binary_ops_elide_leader() {
    let r = run_section(
        "
        let hello = |origin, radius| origin + radius
        mesh base = hello(origin: 10, radius: 2)
        let result = base.origin + 5
    ",
        SectionType::Slide,
    );
    r.assert_int(15);
}

#[test]
fn test_exec_mesh_leader_labeled_attribute_power_elide_leader() {
    let r = run_section(
        "
        let hello = |origin, radius| origin + radius
        mesh base = hello(origin: 4, radius: 2)
        let result = base.origin ^ 2
    ",
        SectionType::Slide,
    );
    r.assert_float(16.0);
}

#[test]
fn test_exec_labeled_operator_mutation_updates_downstream_value() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        var inv = add{amount: 2} 40
        inv.amount = 5
        let result = mul{2} inv
        print result
    ");
    r.assert_transcript(&["90"]);
}

#[test]
fn test_exec_labeled_operator_error_on_unknown_label() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        let inv = passthrough{amount: 2} f(lbl: 40, 2)
        let result = inv.unknown_label
    ");
    r.assert_error("no labeled argument");
}

#[test]
fn test_exec_labeled_operator_delegates_read_to_operand_attribute() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        let inv = passthrough{amount: 2} f(lbl: 40, 2)
        let result = inv.lbl
    ");
    r.assert_int(40);
}

#[test]
fn test_exec_unlabeled_operator_delegates_read_to_operand_attribute() {
    let r = run("
        let f = |origin = 10, radius = 2| origin + radius
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        let inv = passthrough{2} f(origin: 40, radius: 2)
        let result = inv.origin
    ");
    r.assert_int(40);
}

#[test]
fn test_exec_labeled_operator_delegates_mutation_to_operand_attribute() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        var inv = passthrough{amount: 2} f(lbl: 40, 2)
        inv.lbl = 50
        let result = inv.lbl
    ");
    r.assert_int(50);
}

#[test]
fn test_exec_labeled_operator_operand_mutation_invalidates_cache() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        var inv = passthrough{amount: 2} f(lbl: 40, 2)
        inv.lbl = 50
        let result = inv + 0
    ");
    r.assert_int(52);
}

#[test]
fn test_exec_native_lerp_numbers() {
    let r = run_section(
        "
        let result = __monocurl__native__ lerp(10, 20, 0.25)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(12.5);
}

#[test]
fn test_exec_native_lerp_list_element() {
    let r = run_section(
        "
        let xs = __monocurl__native__ lerp([0, 10], [10, 20], 0.5)
        let result = xs[1]
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(15.0);
}

#[test]
fn test_exec_native_lerp_vector() {
    let r = run_section(
        "
        let result = __monocurl__native__ lerp([0, 10, 20], [10, 20, 30], 0.25)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float_list(&[2.5, 12.5, 22.5]);
}

#[test]
fn test_exec_native_lerp_nested_vector() {
    let r = run_section(
        "
        let rows = __monocurl__native__ lerp([[0, 10], [20, 30]], [[10, 20], [30, 40]], 0.5)
        let result = rows[1]
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float_list(&[25.0, 35.0]);
}

#[test]
fn test_exec_native_lerp_labeled_function_result_value() {
    let r = run_section(
        "
        let f = |x, y| x + y
        let result = __monocurl__native__ lerp(f(lbl: 0, 10), f(lbl: 8, 10), 0.25) + 0
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(12.0);
}

#[test]
fn test_exec_native_lerp_labeled_function_preserves_label() {
    let r = run_section(
        "
        let f = |x, y| x + y
        let result = (__monocurl__native__ lerp(f(lbl: 0, 10), f(lbl: 8, 10), 0.25)).lbl
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(2.0);
}

#[test]
fn test_exec_native_lerp_labeled_function_rejects_unlabeled_difference() {
    let r = run_section(
        "
        let f = |x, y| x + y
        let result = __monocurl__native__ lerp(f(1, lbl: 10), f(2, lbl: 20), 0.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_error("unlabeled argument at index 0 differs");
}

#[test]
fn test_exec_native_lerp_operator_rhs_uses_operand() {
    let r = run_section(
        "
        let shift = operator |target, delta| {
            return [target + 100, target + delta]
        }
        let result = __monocurl__native__ lerp(10, shift{delta: 4} 20, 0.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(67.0);
}

#[test]
fn test_exec_native_lerp_labeled_operator_rhs() {
    let r = run_section(
        "
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let result = __monocurl__native__ lerp(10, add{amount: 8} 20, 0.25)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(14.5);
}

#[test]
fn test_exec_native_lerp_copied_labeled_operator_preserves_label() {
    let r = run_section(
        "
        let shift = operator |target, lbl| {
            return [target, target + lbl]
        }
        var x = shift{lbl: 1} 10
        var y = x
        y.lbl = 10
        let result = (__monocurl__native__ lerp(x, y, 0.5)).lbl
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(5.5);
}

#[test]
fn test_exec_native_lerp_copied_labeled_operator_value() {
    let r = run_section(
        "
        let shift = operator |target, lbl| {
            return [target, target + lbl]
        }
        var x = shift{lbl: 1} 10
        var y = x
        y.lbl = 10
        let result = (__monocurl__native__ lerp(x, y, 0.5)) + 0
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(15.5);
}

#[test]
fn test_exec_native_lerp_nested_labeled_operator_rhs() {
    let r = run_section(
        "
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        let result = __monocurl__native__ lerp(10, add{amount: 2} mul{factor: 3} 4, 0.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(15.0);
}

#[test]
fn test_exec_native_lerp_copied_nested_labeled_operator() {
    let r = run_section(
        "
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        var x = add{amount: 2} mul{factor: 3} 4
        var y = x
        y.amount = 6
        y.factor = 5
        let z = __monocurl__native__ lerp(x, y, 0.5)
        let result = z.amount + z.factor + (z + 0)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(28.0);
}
