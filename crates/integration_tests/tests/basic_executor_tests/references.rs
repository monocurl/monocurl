use super::*;

// -- references --

#[test]
fn test_ref_basic_mutation() {
    // mutate increments its reference argument; x should be 1 after the call
    let r = run("
        param x = 0
        let mutate = |&y| {
            y = y + 1
            return []
        }
        mutate(&x)
        let result = x
    ");
    r.assert_int(1);
}

#[test]
fn test_ref_mutation_does_not_affect_unrelated_var() {
    let r = run("
        param x = 10
        param z = 99
        let inc = |&y| {
            y = y + 1
            return []
        }
        inc(&x)
        let result = z
    ");
    r.assert_int(99);
}

#[test]
fn test_ref_called_multiple_times() {
    let r = run("
        param x = 0
        let inc = |&y| {
            y = y + 1
            return []
        }
        inc(&x)
        inc(&x)
        inc(&x)
        let result = x
    ");
    r.assert_int(3);
}

#[test]
fn test_ref_chain_of_lambdas() {
    // inner passes its reference argument straight through to another lambda
    let r = run("
        param x = 0
        let add_two = |&y| {
            y = y + 2
            return []
        }
        let double_add = |&z| {
            add_two(&z)
            add_two(&z)
            return []
        }
        double_add(&x)
        let result = x
    ");
    r.assert_int(4);
}

#[test]
fn test_ref_two_distinct_references() {
    let r = run("
        param a = 1
        param b = 10
        let modify_both = |&x, &y| {
            x = x + 1
            y = y + 1
            return []
        }
        modify_both(&a, &b)
        let result = a + b
    ");
    // a=2, b=11, result=13
    r.assert_int(13);
}

#[test]
fn test_ref_reference_to_list_via_ref() {
    // pass the whole list by reference; subscript-assign inside the lambda
    let r = run("
        param arr = [0, 0, 0]
        let set_first = |&a| {
            a[0] = 42
            return []
        }
        set_first(&arr)
        let result = (arr)[0]
    ");
    r.assert_int(42);
}

#[test]
fn test_ref_destructure_list_references() {
    // pass a list of references using list destructure assignment inside the lambda
    let r = run("
        param a = 0
        param b = 0
        let set_both = |&x, &y| {
            x = 7
            y = 13
            return []
        }
        set_both(&a, &b)
        let result = a + b
    ");
    r.assert_int(20);
}

#[test]
fn test_ref_reference_in_closure_capture() {
    // lambda captures a var by value; separate reference arg must not alias the capture
    let r = run("
        let captured = 5
        param target = 0
        let f = |&r| {
            r = captured + 1
            return []
        }
        f(&target)
        let result = target
    ");
    r.assert_int(6);
}

#[test]
fn test_ref_lambda_called_with_value_reports_runtime_error_instead_of_panicking() {
    let r = run("
        let overwrite = |&y| {
            y = 2
            return []
        }
        overwrite(1)
    ");
    r.assert_error("cannot assign");
}

#[test]
fn test_ref_lambda_returning_anim_can_be_played_multiple_times() {
    let r = run("
        param cam = 0

        let View = |&camera_ref, at| anim {
            camera_ref = at
        }

        play View(&cam, 4)
        play View(&cam, 5)

        let result = cam
    ");
    r.assert_int(5);
}

#[test]
fn test_nested_ref_anim_invocation_can_be_played_multiple_times() {
    let r = run("
        param cam = 0

        let CameraLerp = |&camera_ref, time = 1| anim {
            camera_ref = time
        }

        let View = |&camera_ref, at| anim {
            camera_ref = at
            play CameraLerp(&camera_ref, 3)
        }

        play View(&cam, 4)
        play View(&cam, 5)

        let result = cam
    ");
    r.assert_int(3);
}

#[test]
fn test_camera_lerp_view_lambda_can_be_played_multiple_times() {
    let r = run_with_stdlib(
        "
        mesh cam = DEFAULT_CAMERA

        let View = |&camera_ref, at| anim {
            play CameraLerp(&camera_ref, 3)
        }

        play Set()
        play View(&cam, 4)
        play View(&cam, 5)

        let result = 1
    ",
        &["scene", "anim"],
    );
    r.assert_int(1);
}
