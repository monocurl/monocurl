//! the result cache: seeking one executor back to a frame it has already
//! produced must reuse the sampled result instead of running the batch again

use std::path::Path;

use executor::{executor::SeekOptions, time::Timestamp};
use scene_harness::{prepare, use_repo_assets};

const SCENE: &str = "
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

let Plasma = |zoom|
    Shader(|x, y| [0.5 + 0.5 * sin(x * zoom), 0.5, 1 - 0.5 * y, 1], [-1, 1], [-1, 1], 24)

mesh wave = ExplicitFunc(|x| sin(x * 3), [-2, 2, 65])
mesh surface = Plasma(zoom: 1)

slide \"Zoom\"
    surface.zoom = 4
    play Lerp(1)
";

fn seek(executor: &mut executor::executor::Executor, time: f64) {
    let slide = executor.total_sections() - 1;
    smol::block_on(async {
        executor
            .seek_to_with_options(Timestamp::new(slide, time), SeekOptions::fast())
            .await;
    });
}

#[test]
fn revisited_frames_reuse_sampled_results() {
    use_repo_assets();
    let (mut executor, _) = prepare(SCENE, Path::new("result_cache_test.mcs")).unwrap();
    seek(&mut executor, 0.5);
    let after_first = executor.kernel_stats().calls;
    assert!(
        after_first > 0,
        "the shader and graph should sample as batches"
    );

    seek(&mut executor, 0.25);
    let after_second = executor.kernel_stats().calls;
    assert!(after_second > after_first, "a new frame samples again");

    seek(&mut executor, 0.5);
    assert_eq!(
        executor.kernel_stats().calls,
        after_second,
        "revisiting a frame must come from the result cache"
    );
    assert!(executor.state.errors.is_empty());
}

const PALETTE_SCENE: &str = "
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

let palette = [0 -> BLUE, 0.5 -> YELLOW, 1 -> RED]

let Plasma = |zoom|
    Shader(|x, y| keyframe_lerp(palette, 0.5 + 0.5 * sin(x * zoom + y)), [-1, 1], [-1, 1], 24)

mesh surface = Plasma(zoom: 1)

slide \"Zoom\"
    surface.zoom = 4
    play Lerp(1)
";

#[test]
fn shaders_capturing_a_palette_are_cached_too() {
    use_repo_assets();
    let (mut executor, _) = prepare(PALETTE_SCENE, Path::new("result_cache_palette_test.mcs")).unwrap();
    seek(&mut executor, 0.5);
    let after_first = executor.kernel_stats().calls;
    assert!(after_first > 0, "the palette shader should sample as a batch");

    seek(&mut executor, 0.25);
    let after_second = executor.kernel_stats().calls;
    assert!(after_second > after_first, "a new frame samples again");

    seek(&mut executor, 0.5);
    assert_eq!(
        executor.kernel_stats().calls,
        after_second,
        "a revisited frame must come from the result cache even though the callback captures a map"
    );
    assert!(executor.state.errors.is_empty());
}
