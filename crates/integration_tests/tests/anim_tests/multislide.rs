use super::*;

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
    // mesh variables declared in slide 0 remain visible in slide 1
    let r = run_multi_anim(&["mesh counter = 99", "let check = 1"], 1, f64::INFINITY);
    r.assert_ok();
    let meshes = r.mesh_leaders();
    meshes[0].assert_target_int(99);
}

// -- error cases --
