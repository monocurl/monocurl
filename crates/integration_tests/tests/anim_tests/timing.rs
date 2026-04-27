use super::*;

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
fn test_playback_prepares_new_slide_without_spending_frame_dt() {
    let bundles = stdlib_bundles(["anim"]);
    let (mut executor, _) = build_anim_executor(
        &[
            ("play Wait(0.1)", SectionType::Slide),
            ("play Wait(1.0)", SectionType::Slide),
        ],
        &bundles,
    )
    .unwrap_or_else(|result| panic!("executor should build, got errors: {:?}", result.errors));

    smol::block_on(async {
        let first_slide_end = executor.user_to_internal_timestamp(user_slide_end(0));
        match executor.seek_to(first_slide_end).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => panic!("seek failed: {e}"),
        }

        let result = executor
            .advance_playback(executor.total_sections(), 0.5)
            .await
            .expect("playback should not error");
        assert_eq!(result, PlaybackAdvance::PreparedSection);
    });

    let timestamp = executor.internal_to_user_timestamp(executor.state.timestamp);
    assert_eq!(timestamp, user_timestamp(1, 0.0));
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
