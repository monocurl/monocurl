use super::*;

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

#[test]
fn test_mesh_append_assign_preserves_snapshot_follower_slot() {
    let (mut executor, _) = build_anim_executor(
        &[(
            "
            mesh lins = []
            play Set()
            lins .= Line()
            ",
            SectionType::Slide,
        )],
        &stdlib_bundles(["mesh", "anim"]),
    )
    .unwrap_or_else(|result| panic!("executor should build, got errors: {:?}", result.errors));

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::at_end_of_slide(0));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("seek failed: {e}"),
        }

        executor
            .capture_stable_scene_snapshot()
            .await
            .unwrap_or_else(|e| panic!("snapshot failed: {e}"));
    });
}
