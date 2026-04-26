use std::{
    collections::HashMap,
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use compiler::{
    cache::CompilerCache,
    compiler::{CompileError, compile},
};
use executor::{
    error::{RuntimeCallFrame, RuntimeError},
    executor::{Executor, SeekToResult, TextRenderQuality},
    time::Timestamp,
};
use image::ImageFormat;
use lexer::{lexer::Lexer, token::Token};
use mp4::{
    AvcConfig, Bytes, FourCC, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType,
};
use openh264::{
    encoder::{
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod,
        QpRange, RateControlMode, UsageType,
    },
    formats::{BgraSliceU8, YUVBuffer},
};
use parser::{
    import_context::ParseImportContext,
    parser::{Diagnostic, Parser},
};
use renderer::{RenderOptions, RenderSize, Renderer, RgbaImage, SceneRenderData};
use stdlib::registry::registry;
use structs::rope::{Attribute, RLEData, Rope, TextAggregate};

pub const DEFAULT_EXPORT_SIZE: RenderSize = RenderSize::new(1920, 1080);
pub const DEFAULT_VIDEO_FPS: u32 = 60;
pub const EXPORT_CANCELLED_MESSAGE: &str = "Export canceled";

const MP4_TRACK_ID: u32 = 1;
const MP4_FRAME_DURATION: u32 = 1000;
const MP4_MINOR_VERSION: u32 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportSettings {
    pub render_size: RenderSize,
    pub fps: u32,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            render_size: DEFAULT_EXPORT_SIZE,
            fps: DEFAULT_VIDEO_FPS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageExportTimestamp {
    Exact(Timestamp),
    SceneEnd,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExportKind {
    Image { timestamp: ImageExportTimestamp },
    Video,
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub root_text: String,
    pub root_path: PathBuf,
    pub open_documents: HashMap<PathBuf, String>,
    pub output_path: PathBuf,
    pub kind: ExportKind,
    pub settings: ExportSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportProgress {
    pub message: String,
    pub completed: usize,
    pub total: usize,
}

impl ExportProgress {
    pub fn ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.completed as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportOutcome {
    pub output_path: PathBuf,
    pub frames_written: usize,
    pub transcript: Vec<executor::transcript::TranscriptEntry>,
}

struct PreparedScene {
    executor: Executor,
    root_text_rope: Rope<TextAggregate>,
}

struct PartialOutputCleanup {
    path: PathBuf,
    active: bool,
    keep: bool,
}

impl PartialOutputCleanup {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            active: false,
            keep: false,
        }
    }

    fn arm(&mut self) {
        self.active = true;
    }

    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for PartialOutputCleanup {
    fn drop(&mut self) {
        if self.active && !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct EncodedSample {
    bytes: Vec<u8>,
    is_sync: bool,
}

#[derive(Default)]
struct AvcParameterSets {
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
}

impl AvcParameterSets {
    fn ready(&self) -> bool {
        self.sps.is_some() && self.pps.is_some()
    }

    fn into_config(self, size: RenderSize) -> Result<AvcConfig> {
        Ok(AvcConfig {
            width: size
                .width
                .try_into()
                .context("video width does not fit into mp4 track config")?,
            height: size
                .height
                .try_into()
                .context("video height does not fit into mp4 track config")?,
            seq_param_set: self
                .sps
                .ok_or_else(|| anyhow!("missing SPS parameter set from encoder"))?,
            pic_param_set: self
                .pps
                .ok_or_else(|| anyhow!("missing PPS parameter set from encoder"))?,
        })
    }
}

pub fn export_scene(
    request: ExportRequest,
    cancel_flag: Arc<AtomicBool>,
    mut on_progress: impl FnMut(ExportProgress),
) -> Result<ExportOutcome> {
    smol::block_on(async {
        export_scene_async(request, cancel_flag.as_ref(), &mut on_progress).await
    })
}

async fn export_scene_async(
    request: ExportRequest,
    cancel_flag: &AtomicBool,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<ExportOutcome> {
    ensure!(
        request.settings.render_size.width > 0 && request.settings.render_size.height > 0,
        "render size must be non-zero"
    );
    ensure!(request.settings.fps > 0, "video fps must be non-zero");
    check_cancelled(cancel_flag)?;

    if let Some(parent) = request.output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create export directory {}", parent.display()))?;
    }
    check_cancelled(cancel_flag)?;

    match request.kind {
        ExportKind::Image { timestamp } => {
            let prepared = prepare_scene(
                &request.root_text,
                &request.root_path,
                &request.open_documents,
                cancel_flag,
                on_progress,
                0,
                4,
            )?;
            export_image(
                prepared,
                request.output_path,
                request.settings,
                timestamp,
                cancel_flag,
                on_progress,
            )
            .await
        }
        ExportKind::Video => {
            let prepared = prepare_scene(
                &request.root_text,
                &request.root_path,
                &request.open_documents,
                cancel_flag,
                on_progress,
                0,
                0,
            )?;
            export_video(
                prepared,
                request.output_path,
                request.settings,
                cancel_flag,
                on_progress,
            )
            .await
        }
    }
}

fn prepare_scene(
    root_text: &str,
    root_path: &PathBuf,
    open_documents: &HashMap<PathBuf, String>,
    cancel_flag: &AtomicBool,
    on_progress: &mut dyn FnMut(ExportProgress),
    completed: usize,
    total: usize,
) -> Result<PreparedScene> {
    emit_progress_checked(cancel_flag, on_progress, "Parsing scene", completed, total)?;

    let root_text_rope = Rope::from_str(root_text);
    let root_lex_rope = lex_rope_from_str(root_text);

    let mut import_context = ParseImportContext {
        root_file_path: root_path.clone(),
        open_tab_ropes: open_documents
            .iter()
            .map(|(path, text)| {
                (
                    path.clone(),
                    (lex_rope_from_str(text), Rope::from_str(text.as_str())),
                )
            })
            .collect(),
        cached_parses: HashMap::new(),
    };

    let (bundles, parse_artifacts) = Parser::parse(
        &mut import_context,
        root_lex_rope,
        root_text_rope.clone(),
        None,
    );
    if !parse_artifacts.error_diagnostics.is_empty() {
        bail!(
            "{}",
            format_parse_diagnostics(&parse_artifacts.error_diagnostics, &root_text_rope)
        );
    }
    check_cancelled(cancel_flag)?;

    emit_progress_checked(
        cancel_flag,
        on_progress,
        "Compiling scene",
        completed + 1,
        total,
    )?;

    let mut compiler_cache = CompilerCache::default();
    let compile_result = compile(&mut compiler_cache, None, &bundles);
    if !compile_result.errors.is_empty() {
        bail!(
            "{}",
            format_compile_errors(&compile_result.errors, &root_text_rope)
        );
    }
    check_cancelled(cancel_flag)?;

    let mut executor = Executor::new(compile_result.bytecode, registry().func_table());
    executor.set_text_render_quality(TextRenderQuality::High);

    Ok(PreparedScene {
        executor,
        root_text_rope,
    })
}

async fn export_image(
    mut prepared: PreparedScene,
    output_path: PathBuf,
    settings: ExportSettings,
    timestamp: ImageExportTimestamp,
    cancel_flag: &AtomicBool,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<ExportOutcome> {
    emit_progress_checked(cancel_flag, on_progress, "Rendering frame", 2, 4)?;

    let timestamp = match timestamp {
        ImageExportTimestamp::Exact(timestamp) => timestamp,
        ImageExportTimestamp::SceneEnd => {
            resolve_scene_end_timestamp(&mut prepared.executor, &prepared.root_text_rope).await?
        }
    };

    let mut renderer = Renderer::try_new(RenderOptions::default())
        .context("failed to initialize blade renderer")?;

    let frame = render_frame(
        &mut prepared.executor,
        &prepared.root_text_rope,
        &mut renderer,
        timestamp,
        settings.render_size,
    )
    .await?;
    check_cancelled(cancel_flag)?;

    emit_progress_checked(cancel_flag, on_progress, "Writing image", 3, 4)?;

    bgra_to_rgba(frame)
        .save_with_format(&output_path, ImageFormat::Png)
        .with_context(|| format!("failed to write image export {}", output_path.display()))?;

    emit_progress_checked(cancel_flag, on_progress, "Finished image export", 4, 4)?;

    let transcript = collect_transcript(&prepared.executor);
    Ok(ExportOutcome {
        output_path,
        frames_written: 1,
        transcript,
    })
}

fn collect_transcript(executor: &Executor) -> Vec<executor::transcript::TranscriptEntry> {
    executor.state.transcript.iter_entries().cloned().collect()
}

async fn resolve_scene_end_timestamp(
    executor: &mut Executor,
    root_text_rope: &Rope<TextAggregate>,
) -> Result<Timestamp> {
    let internal = seek_internal_timestamp(
        executor,
        Timestamp::new(executor.total_sections(), f64::INFINITY),
        root_text_rope,
    )
    .await?;
    Ok(executor.internal_to_user_timestamp(internal))
}

async fn export_video(
    mut prepared: PreparedScene,
    output_path: PathBuf,
    settings: ExportSettings,
    cancel_flag: &AtomicBool,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<ExportOutcome> {
    emit_progress_checked(cancel_flag, on_progress, "Evaluating timeline", 0, 0)?;

    ensure!(
        settings.render_size.width % 2 == 0 && settings.render_size.height % 2 == 0,
        "video render size must use even dimensions for H.264 export"
    );

    let video_size = settings.render_size;
    let slide_durations =
        precompute_slide_durations(&mut prepared.executor, &prepared.root_text_rope).await?;
    let transcript = collect_transcript(&prepared.executor);
    check_cancelled(cancel_flag)?;
    let frame_targets = build_frame_targets(&slide_durations, settings.fps);
    ensure!(!frame_targets.is_empty(), "scene has no slides to export");

    let total_steps = frame_targets.len() + 1;
    let mp4_timescale = settings
        .fps
        .checked_mul(MP4_FRAME_DURATION)
        .ok_or_else(|| anyhow!("mp4 timescale overflow"))?;

    let mut renderer = Renderer::try_new(RenderOptions::default())
        .context("failed to initialize blade renderer")?;
    let mut encoder = Encoder::with_api_config(
        openh264::OpenH264API::from_source(),
        EncoderConfig::new()
            .skip_frames(false)
            .rate_control_mode(RateControlMode::Quality)
            .complexity(Complexity::High)
            .usage_type(UsageType::ScreenContentRealTime)
            .adaptive_quantization(false)
            .background_detection(false)
            .max_frame_rate(FrameRate::from_hz(settings.fps as f32))
            .bitrate(BitRate::from_bps(video_bitrate(video_size, settings.fps)))
            .qp(video_qp_range(video_size))
            .intra_frame_period(IntraFramePeriod::from_num_frames(settings.fps)),
    )
    .context("failed to initialize H.264 encoder")?;

    let mut parameter_sets = AvcParameterSets::default();
    let mut cleanup = PartialOutputCleanup::new(output_path.clone());
    let mut writer = None;
    let mut sample_start_time = 0_u64;
    let mut previous_slide = None;

    for (index, timestamp) in frame_targets.iter().copied().enumerate() {
        emit_progress_checked(
            cancel_flag,
            on_progress,
            format!("Rendering frame {} of {}", index + 1, frame_targets.len()),
            index,
            total_steps,
        )?;

        if index > 0 && previous_slide != Some(timestamp.slide) {
            encoder.force_intra_frame();
        }

        let frame = render_frame(
            &mut prepared.executor,
            &prepared.root_text_rope,
            &mut renderer,
            timestamp,
            video_size,
        )
        .await?;
        check_cancelled(cancel_flag)?;

        let encoded = encode_video_sample(&frame, video_size, &mut encoder, &mut parameter_sets)
            .with_context(|| {
                format!(
                    "failed to encode H.264 sample at slide {} time {:.3} ({}x{})",
                    timestamp.slide, timestamp.time, video_size.width, video_size.height
                )
            })?;
        check_cancelled(cancel_flag)?;

        if writer.is_none() {
            ensure!(
                parameter_sets.ready(),
                "encoder did not emit SPS/PPS parameter sets for the initial video sample"
            );
            check_cancelled(cancel_flag)?;
            let file = File::create(&output_path).with_context(|| {
                format!("failed to create video export {}", output_path.display())
            })?;
            cleanup.arm();
            writer = Some(start_mp4_writer(
                BufWriter::new(file),
                mp4_timescale,
                video_size,
                std::mem::take(&mut parameter_sets),
            )?);
        }

        writer
            .as_mut()
            .unwrap()
            .write_sample(
                MP4_TRACK_ID,
                &Mp4Sample {
                    start_time: sample_start_time,
                    duration: MP4_FRAME_DURATION,
                    rendering_offset: 0,
                    is_sync: encoded.is_sync,
                    bytes: Bytes::from(encoded.bytes),
                },
            )
            .with_context(|| format!("failed to write mp4 sample {}", index + 1))?;
        check_cancelled(cancel_flag)?;

        sample_start_time += u64::from(MP4_FRAME_DURATION);
        previous_slide = Some(timestamp.slide);
    }

    emit_progress_checked(
        cancel_flag,
        on_progress,
        "Finalizing video",
        frame_targets.len(),
        total_steps,
    )?;

    writer
        .as_mut()
        .ok_or_else(|| anyhow!("video export produced no encoded samples"))?
        .write_end()
        .with_context(|| format!("failed to finalize video export {}", output_path.display()))?;
    cleanup.keep();

    emit_progress_checked(
        cancel_flag,
        on_progress,
        "Finished video export",
        total_steps,
        total_steps,
    )?;

    Ok(ExportOutcome {
        output_path,
        frames_written: frame_targets.len(),
        transcript,
    })
}

async fn precompute_slide_durations(
    executor: &mut Executor,
    root_text_rope: &Rope<TextAggregate>,
) -> Result<Vec<f64>> {
    seek_internal_timestamp(
        executor,
        Timestamp::new(executor.total_sections(), f64::INFINITY),
        root_text_rope,
    )
    .await?;

    let durations = executor.real_slide_durations();
    let minimum_durations = executor.real_minimum_slide_durations();
    let slide_count = executor.real_slide_count();

    let mut resolved = Vec::with_capacity(slide_count);
    for slide in 0..slide_count {
        let duration = durations
            .get(slide)
            .copied()
            .flatten()
            .or_else(|| minimum_durations.get(slide).copied().flatten())
            .unwrap_or_default()
            .max(0.0);
        resolved.push(duration);
    }

    Ok(resolved)
}

fn build_frame_targets(slide_durations: &[f64], fps: u32) -> Vec<Timestamp> {
    let fps = fps as f64;
    let mut frames = Vec::new();

    for (slide, duration) in slide_durations.iter().copied().enumerate() {
        let frame_count = ((duration * fps).ceil() as usize).max(1);
        for frame in 0..frame_count {
            frames.push(Timestamp::new(slide, frame as f64 / fps));
        }
    }

    frames
}

async fn render_frame(
    executor: &mut Executor,
    root_text_rope: &Rope<TextAggregate>,
    renderer: &mut Renderer,
    user_timestamp: Timestamp,
    render_size: RenderSize,
) -> Result<RgbaImage> {
    seek_internal_timestamp(
        executor,
        executor.user_to_internal_timestamp(user_timestamp),
        root_text_rope,
    )
    .await?;

    let scene = executor
        .capture_stable_scene_snapshot()
        .await
        .map_err(|error| {
            anyhow!(format_runtime_error_message(
                executor,
                root_text_rope,
                &error
            ))
        })?;

    renderer
        .render(&SceneRenderData::from(scene), render_size)
        .with_context(|| {
            format!(
                "failed to render frame at slide {} time {:.3}",
                user_timestamp.slide, user_timestamp.time
            )
        })
}

async fn seek_internal_timestamp(
    executor: &mut Executor,
    target: Timestamp,
    root_text_rope: &Rope<TextAggregate>,
) -> Result<Timestamp> {
    match executor.seek_to(target).await {
        SeekToResult::SeekedTo(timestamp) => Ok(timestamp),
        SeekToResult::Error(error) => {
            let message = executor
                .state
                .errors
                .last()
                .map(|runtime_error| {
                    format_runtime_error_message(executor, root_text_rope, runtime_error)
                })
                .unwrap_or_else(|| error.to_string());
            bail!("{message}");
        }
    }
}

fn start_mp4_writer(
    writer: BufWriter<File>,
    timescale: u32,
    size: RenderSize,
    parameter_sets: AvcParameterSets,
) -> Result<Mp4Writer<BufWriter<File>>> {
    let config = Mp4Config {
        major_brand: parse_fourcc("isom"),
        minor_version: MP4_MINOR_VERSION,
        compatible_brands: vec![
            parse_fourcc("isom"),
            parse_fourcc("iso2"),
            parse_fourcc("avc1"),
            parse_fourcc("mp41"),
        ],
        timescale,
    };

    let mut mp4 = Mp4Writer::write_start(writer, &config).context("failed to start mp4 writer")?;
    let track = TrackConfig {
        track_type: TrackType::Video,
        timescale,
        language: "und".into(),
        media_conf: MediaConfig::AvcConfig(parameter_sets.into_config(size)?),
    };
    mp4.add_track(&track)
        .context("failed to add mp4 video track")?;
    Ok(mp4)
}

fn encode_video_sample(
    frame: &RgbaImage,
    size: RenderSize,
    encoder: &mut Encoder,
    parameter_sets: &mut AvcParameterSets,
) -> Result<EncodedSample> {
    let bgra = BgraSliceU8::new(frame.as_raw(), (size.width as usize, size.height as usize));
    let yuv = YUVBuffer::from_rgb_source(bgra);
    let stream = encoder.encode(&yuv)?;

    let mut bytes = Vec::new();
    for layer_index in 0..stream.num_layers() {
        let layer = stream
            .layer(layer_index)
            .ok_or_else(|| anyhow!("missing expected encoded layer {layer_index}"))?;
        for nal_index in 0..layer.nal_count() {
            let nal = layer
                .nal_unit(nal_index)
                .ok_or_else(|| anyhow!("missing expected encoded nal {nal_index}"))?;
            let payload = strip_annex_b_start_code(nal)
                .ok_or_else(|| anyhow!("unexpected nal start code format"))?;
            if payload.is_empty() {
                continue;
            }

            match payload[0] & 0x1f {
                7 => {
                    parameter_sets.sps.get_or_insert_with(|| payload.to_vec());
                }
                8 => {
                    parameter_sets.pps.get_or_insert_with(|| payload.to_vec());
                }
                _ if layer.is_video() => {
                    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                    bytes.extend_from_slice(payload);
                }
                _ => {}
            }
        }
    }

    ensure!(
        !bytes.is_empty(),
        "encoder did not emit any video NAL units for the current frame"
    );

    Ok(EncodedSample {
        bytes,
        is_sync: matches!(stream.frame_type(), FrameType::IDR | FrameType::I),
    })
}

fn parse_fourcc(value: &str) -> FourCC {
    value
        .parse()
        .expect("hard-coded fourcc should always parse")
}

fn video_bitrate(size: RenderSize, fps: u32) -> u32 {
    let pixels_per_second = u64::from(size.width) * u64::from(size.height) * u64::from(fps);
    ((pixels_per_second * 3) / 2)
        .clamp(36_000_000, 240_000_000)
        .try_into()
        .expect("clamped bitrate should fit in u32")
}

fn video_qp_range(size: RenderSize) -> QpRange {
    let pixels = u64::from(size.width) * u64::from(size.height);
    if pixels <= 1280_u64 * 720 {
        QpRange::new(10, 20)
    } else if pixels <= 1920_u64 * 1080 {
        QpRange::new(10, 22)
    } else {
        QpRange::new(10, 24)
    }
}

fn lex_rope_from_str(text: &str) -> Rope<Attribute<Token>> {
    Rope::default().replace_range(
        0..0,
        Lexer::new(text.chars()).map(|(attribute, codeunits)| RLEData {
            codeunits,
            attribute,
        }),
    )
}

fn emit_progress(
    on_progress: &mut dyn FnMut(ExportProgress),
    message: impl Into<String>,
    completed: usize,
    total: usize,
) {
    on_progress(ExportProgress {
        message: message.into(),
        completed,
        total,
    });
}

fn emit_progress_checked(
    cancel_flag: &AtomicBool,
    on_progress: &mut dyn FnMut(ExportProgress),
    message: impl Into<String>,
    completed: usize,
    total: usize,
) -> Result<()> {
    check_cancelled(cancel_flag)?;
    emit_progress(on_progress, message, completed, total);
    Ok(())
}

fn check_cancelled(cancel_flag: &AtomicBool) -> Result<()> {
    if cancel_flag.load(Ordering::Relaxed) {
        bail!(EXPORT_CANCELLED_MESSAGE);
    }
    Ok(())
}

fn bgra_to_rgba(mut frame: RgbaImage) -> RgbaImage {
    for pixel in frame.as_mut().chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    frame
}

fn strip_annex_b_start_code(nal: &[u8]) -> Option<&[u8]> {
    if nal.starts_with(&[0, 0, 0, 1]) {
        Some(&nal[4..])
    } else if nal.starts_with(&[0, 0, 1]) {
        Some(&nal[3..])
    } else {
        None
    }
}

fn format_parse_diagnostics(
    diagnostics: &[Diagnostic],
    root_text_rope: &Rope<TextAggregate>,
) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{} on line {}: {}",
                diagnostic.title,
                line_number(root_text_rope, diagnostic.span.start),
                diagnostic.message,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_compile_errors(errors: &[CompileError], root_text_rope: &Rope<TextAggregate>) -> String {
    errors
        .iter()
        .map(|error| {
            format!(
                "Compile error on line {}: {}",
                line_number(root_text_rope, error.span.start),
                error.message,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_number(text_rope: &Rope<TextAggregate>, offset: usize) -> usize {
    text_rope.utf8_prefix_summary(offset).newlines + 1
}

fn format_runtime_error_message(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    runtime_error: &RuntimeError,
) -> String {
    let mut message = runtime_error.error.to_string();
    if runtime_error.callstack.is_empty() {
        return message;
    }

    let formatted_callstack = runtime_error
        .callstack
        .iter()
        .map(|frame| format_runtime_call_frame(executor, root_text_rope, frame))
        .collect::<Vec<_>>()
        .join("\n");

    message.push_str("\n\ntop of callstack:\n");
    message.push_str(&formatted_callstack);
    message
}

fn format_runtime_call_frame(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    frame: &RuntimeCallFrame,
) -> String {
    let section_idx = frame.section as usize;
    let section = executor.section_bytecode(section_idx);

    if section.flags.is_root_module {
        let line = line_number(root_text_rope, frame.span.start);
        format!("{}:{}", root_section_label(executor, section_idx), line)
    } else if let Some(name) = &section.source_file_name {
        format!("<{}>", name)
    } else if let Some(index) = section.import_display_index {
        format!("<imported library {}>", index)
    } else {
        "<imported library>".into()
    }
}

fn root_section_label(executor: &Executor, section_idx: usize) -> String {
    let section = executor.section_bytecode(section_idx);
    if section.flags.is_init {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| section.flags.is_root_module && section.flags.is_init)
            .count();
        if ordinal <= 1 {
            "<init>".into()
        } else {
            format!("<init {}>", ordinal)
        }
    } else if section.flags.is_library {
        "<prelude>".into()
    } else {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| {
                section.flags.is_root_module && !section.flags.is_library && !section.flags.is_init
            })
            .count();
        format!("<slide {}>", ordinal)
    }
}
