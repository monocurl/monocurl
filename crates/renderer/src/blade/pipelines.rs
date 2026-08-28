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
    pub(super) lines: gpu::RenderPipeline,
    pub(super) dots: gpu::RenderPipeline,
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
        let replace_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: None,
            write_mask: gpu::ColorWrites::default(),
        }];
        // Every mesh - fills, lines and dots alike - both tests and writes depth,
        // exactly as on base 6080017. Lines/dots writing depth is what gives
        // hidden-line removal: a stroke genuinely behind a separate opaque
        // surface is occluded by that surface, and a stroke in *front* of a
        // surface occludes it instead of a surface painted afterwards bleeding
        // through. The small camera-ward eye-space "decal" bias in `blade.wgsl`
        // (see DECAL_SCALE) keeps a stroke crisply on top of the fill it
        // decorates under a tilted camera without z-fighting; it is deliberately
        // far smaller than the gap to any surface meaningfully in front, so it
        // cannot pop a hidden stroke through one.
        //
        // Hardware `bias` is left at Default: on the Metal backend
        // `bias.constant` (i32) is cast straight to f32 in
        // `setDepthBias_slopeScale_clamp` (vendor/blade-graphics-0.7.1/src/metal
        // /command.rs), too coarse for the sub-unit offset a coplanar stroke
        // needs, and `slope_scale` multiplies the primitive's own depth slope
        // (~0 for an image-plane-parallel stroke quad). The shader decal carries
        // the coplanar case instead.
        let depth_write = gpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: gpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: Default::default(),
        };

        Self {
            background: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-background",
                data_layouts: &[&BackgroundData::layout()],
                vertex: shader.at("vs_background"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_background")),
                color_targets: &replace_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
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
            lines: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-lines",
                data_layouts: &[&LinesData::layout()],
                vertex: shader.at("vs_line"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                fragment: Some(shader.at("fs_line")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            dots: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-dots",
                data_layouts: &[&DotsData::layout()],
                vertex: shader.at("vs_dot"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_write.clone()),
                fragment: Some(shader.at("fs_dot")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
        }
    }

    pub(super) fn destroy(&mut self, gpu: &gpu::Context) {
        gpu.destroy_render_pipeline(&mut self.background);
        gpu.destroy_render_pipeline(&mut self.triangles);
        gpu.destroy_render_pipeline(&mut self.lines);
        gpu.destroy_render_pipeline(&mut self.dots);
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
