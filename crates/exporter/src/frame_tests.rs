//! a video export continues each frame from the previous one; these check that every frame
//! equals the frame the export would get by replaying its slide from the cached start

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use executor::{scene_snapshot::SceneSnapshot, time::Timestamp};

use super::*;

/// debug builds (CI) step coarser: replaying every frame is quadratic in a slide's length
const FPS: u32 = if cfg!(debug_assertions) { 10 } else { 30 };

/// a shader texture's id is unique per construction, like a mesh's version
fn without_texture_ids(debug: String) -> String {
    const MARK: &str = "PixelTexture { id: ";
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while let Some(at) = rest.find(MARK) {
        out.push_str(&rest[..at + MARK.len()]);
        rest = rest[at + MARK.len()..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// exact: every vertex, colour and uniform, but not the mesh's version stamp
fn fingerprint(scene: &SceneSnapshot) -> Vec<String> {
    std::iter::once(format!("{:?} {:?}", scene.camera, scene.background))
        .chain(scene.meshes.iter().map(|mesh| {
            without_texture_ids(format!(
                "{:?} {:?} {:?} {:?} {:?}",
                mesh.dots, mesh.lins, mesh.tris, mesh.uniform, mesh.tag
            ))
        }))
        .collect()
}

fn frames(path: &Path, range: VideoRange, seek: FrameSeek) -> Vec<(Timestamp, Vec<String>)> {
    // reused faces make a morph's triangulation depend on the frames before it
    stdlib::set_face_reuse(false);
    let text = std::fs::read_to_string(path).unwrap();
    let settings = ExportSettings {
        render_size: RenderSize::new(640, 360),
        fps: FPS,
    };
    smol::block_on(async {
        let mut prepared = prepare_scene(
            &text,
            path,
            &HashMap::new(),
            &AtomicBool::new(false),
            &mut |_| {},
            0,
            0,
        )
        .unwrap_or_else(|error| panic!("{}: {error:#}", path.display()));
        let plan = prepare_video_render_plan(&mut prepared, settings)
            .await
            .unwrap();
        let mut out = Vec::new();
        for target in build_frame_targets(&plan.slide_durations, FPS, range) {
            let scene = evaluate_frame(
                &mut prepared.executor,
                &prepared.root_text_rope,
                target,
                seek,
            )
            .await
            .unwrap_or_else(|error| panic!("{} at {target:?}: {error:#}", path.display()));
            out.push((target, fingerprint(&scene)));
        }
        out
    })
}

fn assert_same_frames(path: &Path, advanced: &[(Timestamp, Vec<String>)], reference: &[(Timestamp, Vec<String>)]) {
    assert_eq!(advanced.len(), reference.len(), "{}", path.display());
    for ((at, a), (bt, b)) in advanced.iter().zip(reference) {
        assert_eq!(at, bt, "{}", path.display());
        assert_eq!(a.len(), b.len(), "{} at {at:?}: mesh counts differ", path.display());
        for (index, (x, y)) in a.iter().zip(b).enumerate() {
            if x != y {
                let start = x.bytes().zip(y.bytes()).position(|(p, q)| p != q).unwrap_or(0);
                let window = |s: &str| s.get(start.saturating_sub(80)..(start + 160).min(s.len())).unwrap_or("").to_string();
                panic!(
                    "{} at {at:?}: entry {index} differs\n  advanced: ...{}\n  replayed: ...{}",
                    path.display(),
                    window(x),
                    window(y)
                );
            }
        }
    }
}

fn scenes() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut scenes = vec![root.join("tests/scenes/frame_stepping.mcs")];
    let mut defaults: Vec<_> = std::fs::read_dir(root.join("../../assets/default_scenes"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mcs"))
        .collect();
    defaults.sort();
    scenes.extend(defaults);
    // more scenes to check by hand, e.g. a long video: paths separated by ':'
    if let Some(extra) = std::env::var_os("MONOCURL_FRAME_TEST_SCENES") {
        scenes.extend(std::env::split_paths(&extra));
    }
    scenes
}

#[test]
fn advanced_frames_match_replayed_frames() {
    let mut checked = 0;
    // scenes without slides (the shader example, a language tour) export no frames
    for path in scenes() {
        let advanced = frames(&path, VideoRange::default(), FrameSeek::Advance);
        let replayed = frames(&path, VideoRange::default(), FrameSeek::Reseek);
        assert_same_frames(&path, &advanced, &replayed);
        checked += advanced.len();
    }
    assert!(checked > 0);
}

/// the parts `prender.py` cuts a scene into, each from its own process, join into the whole
#[test]
fn chunked_ranges_match_the_whole_export() {
    for path in scenes().into_iter().take(4) {
        let whole = frames(&path, VideoRange::default(), FrameSeek::Advance);
        let durations = {
            let text = std::fs::read_to_string(&path).unwrap();
            plan_video_slides(
                &text,
                &path,
                ExportSettings {
                    render_size: RenderSize::new(640, 360),
                    fps: FPS,
                },
                Default::default(),
                |_| {},
            )
            .unwrap()
        };
        // cut every slide into a whole first part and two pieces that meet mid-slide
        let mut chunked = Vec::new();
        for (slide, plan) in durations.iter().enumerate() {
            let mid = plan.frames / 2;
            let at = |frame: usize| Some(frame as f64 / FPS as f64);
            let ranges = if mid == 0 {
                vec![VideoRange {
                    first_slide: slide,
                    end_slide: Some(slide + 1),
                    ..Default::default()
                }]
            } else {
                vec![
                    VideoRange {
                        first_slide: slide,
                        end_slide: Some(slide + 1),
                        end_time: at(mid),
                        ..Default::default()
                    },
                    VideoRange {
                        first_slide: slide,
                        end_slide: Some(slide + 1),
                        start_time: at(mid),
                        ..Default::default()
                    },
                ]
            };
            for range in ranges {
                chunked.extend(frames(&path, range, FrameSeek::Advance));
            }
        }
        assert_same_frames(&path, &chunked, &whole);
    }
}

