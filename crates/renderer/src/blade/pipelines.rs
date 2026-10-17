use blade_graphics as gpu;

use super::{
    DEPTH_FORMAT, TARGET_FORMAT,
    types::{
        BackgroundData, BackgroundParams, CameraParams, DotInstancePod, DotShaderParams, DotsData,
        LineShaderParams, LineVertexPod, LinesData, TriShaderParams, TriVertexPod, TrianglesData,
    },
};

pub(super) struct Pipelines {
    pub(super) background: gpu::RenderPipeline,
    pub(super) triangles: gpu::RenderPipeline,
    /// depth only pre-pass for translucent meshes, so that only their nearest surface blends
    pub(super) triangles_depth: gpu::RenderPipeline,
    pub(super) lines: gpu::RenderPipeline,
    pub(super) dots: gpu::RenderPipeline,
    /// translucent strokes and dots, painting each sample at most once per stencil reference
    pub(super) lines_once: gpu::RenderPipeline,
    pub(super) dots_once: gpu::RenderPipeline,
    /// full screen stencil reset, for when the per frame references run out
    pub(super) stencil_reset: gpu::RenderPipeline,
}

impl Pipelines {
    pub(super) fn new(gpu: &gpu::Context, sample_count: u32) -> Self {
        use gpu::ShaderData as _;

        let shader = gpu.create_shader(gpu::ShaderDesc {
            source: include_str!("../blade.wgsl"),
        });
        shader.check_struct_size::<BackgroundParams>();
        shader.check_struct_size::<CameraParams>();
        shader.check_struct_size::<TriShaderParams>();
        shader.check_struct_size::<LineShaderParams>();
        shader.check_struct_size::<DotShaderParams>();
        shader.check_struct_size::<TriVertexPod>();
        shader.check_struct_size::<LineVertexPod>();
        shader.check_struct_size::<DotInstancePod>();

        let alpha_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: Some(gpu::BlendState::ALPHA_BLENDING),
            write_mask: gpu::ColorWrites::default(),
        }];
        let depth_only_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: None,
            write_mask: gpu::ColorWrites::empty(),
        }];
        let replace_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: None,
            write_mask: gpu::ColorWrites::default(),
        }];
        // Opaque fills write depth. Lines and dots test against it but do NOT
        // write it, and carry a small camera-ward eye-space "decal" bias (see
        // `blade.wgsl`, DECAL_SCALE) so a stroke stays crisply in front of the
        // fill it decorates under a tilted camera without z-fighting.
        //
        // Not writing depth is what keeps that bias safe. A biased stroke can
        // only ever win the depth test against geometry drawn *before* it; it
        // can never punch a hole in an opaque mesh painted afterwards, so 2D
        // paint order is byte-for-byte preserved. And because the bias is tiny
        // (a few 1e-5 world units - just enough to clear depth-buffer noise
        // between a stroke and its own coplanar fill), a stroke that is
        // genuinely hidden behind a separate opaque surface stays hidden: the
        // surface's depth is far more than a decal in front of it. A larger
        // bias here is what caused the x-ray-wireframe regression - the far side
        // of a bumped surface's own wireframe bleeding through the front.
        //
        // Making lines/dots write depth is not an option: the decal biases both
        // the depth test and (necessarily, one value per fragment) the depth
        // write, so a decal large enough to fix coplanar flicker also makes an
        // earlier stroke wrongly occlude later coplanar fills/strokes, which
        // visibly changes flat 2D scenes with layered strokes.
        //
        // Hardware `bias` is left at Default: on the Metal backend
        // `bias.constant` (i32) is cast straight to f32 in
        // `setDepthBias_slopeScale_clamp` (vendor/blade-graphics-0.7.1/src/metal
        // /command.rs), too coarse for the sub-unit offset a coplanar stroke
        // needs, and `slope_scale` multiplies the primitive's own depth slope
        // (~0 for an image-plane-parallel stroke quad).
        let depth_write = gpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: gpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let depth_read_only = gpu::DepthStencilState {
            depth_write_enabled: false,
            ..depth_write.clone()
        };
        // a translucent stroke is one layer: its pieces overlap at joins and
        // self crossings, and blending each would darken those spots. every
        // translucent stroke (or dot set) gets its own stencil reference, and
        // a sample takes the first fragment that reaches it and rejects the
        // rest. depth failures leave the stencil alone, so a hidden piece does
        // not block a visible one
        let paint_once = gpu::StencilFaceState {
            compare: gpu::CompareFunction::NotEqual,
            fail_op: gpu::StencilOperation::Keep,
            depth_fail_op: gpu::StencilOperation::Keep,
            pass_op: gpu::StencilOperation::Replace,
        };
        let depth_read_paint_once = gpu::DepthStencilState {
            stencil: gpu::StencilState {
                front: paint_once,
                back: paint_once,
                read_mask: 0xff,
                write_mask: 0xff,
            },
            ..depth_read_only.clone()
        };
        let overwrite = gpu::StencilFaceState {
            compare: gpu::CompareFunction::Always,
            fail_op: gpu::StencilOperation::Keep,
            depth_fail_op: gpu::StencilOperation::Keep,
            pass_op: gpu::StencilOperation::Replace,
        };
        let stencil_overwrite = gpu::DepthStencilState {
            depth_write_enabled: false,
            depth_compare: gpu::CompareFunction::Always,
            stencil: gpu::StencilState {
                front: overwrite,
                back: overwrite,
                read_mask: 0xff,
                write_mask: 0xff,
            },
            ..depth_write.clone()
        };
        let no_depth = gpu::DepthStencilState {
            depth_write_enabled: false,
            depth_compare: gpu::CompareFunction::Always,
            ..depth_write.clone()
        };
        let lines = |name, depth_stencil: &gpu::DepthStencilState| {
            gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name,
                data_layouts: &[&LinesData::layout()],
                vertex: shader.at("vs_line"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil.clone()),
                fragment: Some(shader.at("fs_line")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            })
        };
        let dots = |name, depth_stencil: &gpu::DepthStencilState| {
            gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name,
                data_layouts: &[&DotsData::layout()],
                vertex: shader.at("vs_dot"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil.clone()),
                fragment: Some(shader.at("fs_dot")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            })
        };
        let full_screen = |name, depth_stencil: &gpu::DepthStencilState, color_targets| {
            gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name,
                data_layouts: &[&BackgroundData::layout()],
                vertex: shader.at("vs_background"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil.clone()),
                fragment: Some(shader.at("fs_background")),
                color_targets,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            })
        };

        Self {
            background: full_screen("renderer-background", &no_depth, &replace_target),
            stencil_reset: full_screen(
                "renderer-stencil-reset",
                &stencil_overwrite,
                &depth_only_target,
            ),
            triangles: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-triangles",
                data_layouts: &[&TrianglesData::layout()],
                vertex: shader.at("vs_triangle"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    front_face: gpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                fragment: Some(shader.at("fs_triangle")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            triangles_depth: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-triangles-depth",
                data_layouts: &[&TrianglesData::layout()],
                vertex: shader.at("vs_triangle"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    front_face: gpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                fragment: Some(shader.at("fs_triangle")),
                color_targets: &depth_only_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            lines: lines("renderer-lines", &depth_read_only),
            dots: dots("renderer-dots", &depth_read_only),
            lines_once: lines("renderer-lines-once", &depth_read_paint_once),
            dots_once: dots("renderer-dots-once", &depth_read_paint_once),
        }
    }

    pub(super) fn destroy(&mut self, gpu: &gpu::Context) {
        gpu.destroy_render_pipeline(&mut self.background);
        gpu.destroy_render_pipeline(&mut self.triangles);
        gpu.destroy_render_pipeline(&mut self.triangles_depth);
        gpu.destroy_render_pipeline(&mut self.lines);
        gpu.destroy_render_pipeline(&mut self.dots);
        gpu.destroy_render_pipeline(&mut self.lines_once);
        gpu.destroy_render_pipeline(&mut self.dots_once);
        gpu.destroy_render_pipeline(&mut self.stencil_reset);
    }
}

#[cfg(test)]
mod tests {
    use naga::{
        ShaderStage,
        front::wgsl,
        valid::{Capabilities, ValidationFlags, Validator},
    };

    #[test]
    fn blade_shader_parses_and_validates() {
        let source = include_str!("../blade.wgsl");
        assert!(source.contains("struct TriVertexPod"));
        assert!(source.contains("struct LineVertexPod"));
        assert!(source.contains("struct DotInstancePod"));
        // The coplanar-stroke decal offset for lines/dots must stay wired in.
        assert!(source.contains("eye_bias: f32"));
        assert!(source.contains("width_eye * DECAL_SCALE"));

        let module = wgsl::parse_str(source).expect("blade.wgsl should parse successfully");
        Validator::new(
            ValidationFlags::all() & !ValidationFlags::BINDINGS,
            Capabilities::all(),
        )
        .validate(&module)
        .expect("blade.wgsl should validate successfully");

        for entry_point in &module.entry_points {
            match entry_point.stage {
                ShaderStage::Fragment => {
                    assert!(
                        entry_point
                            .function
                            .result
                            .as_ref()
                            .and_then(|result| result.binding.as_ref())
                            .is_some(),
                        "fragment entry point '{}' must have an explicitly bound output",
                        entry_point.name
                    );
                }
                ShaderStage::Vertex => {
                    let Some(result) = entry_point.function.result.as_ref() else {
                        panic!(
                            "vertex entry point '{}' must return a varying struct",
                            entry_point.name
                        );
                    };
                    let naga::TypeInner::Struct { ref members, .. } = module.types[result.ty].inner
                    else {
                        panic!(
                            "vertex entry point '{}' must return a struct so varying bindings are explicit",
                            entry_point.name
                        );
                    };
                    assert!(
                        members.iter().all(|member| member.binding.is_some()),
                        "vertex entry point '{}' has an unbound varying member",
                        entry_point.name
                    );
                }
                _ => {}
            }
        }
    }
}
