use blade_graphics as gpu;

use super::{
    DEPTH_FORMAT, OIT_ACCUM_FORMAT, OIT_REVEAL_FORMAT, TARGET_FORMAT,
    types::{
        BackgroundData, BackgroundParams, CameraParams, DotInstancePod, DotShaderParams, DotsData,
        LineShaderParams, LineVertexPod, LinesData, OitCompositeData, TriShaderParams, TriVertexPod,
        TrianglesData,
    },
};

pub(super) struct Pipelines {
    pub(super) background: gpu::RenderPipeline,
    pub(super) triangles: gpu::RenderPipeline,
    pub(super) lines: gpu::RenderPipeline,
    pub(super) dots: gpu::RenderPipeline,
    /// Weighted-blended OIT accumulation pipelines (write to accum + revealage,
    /// depth test only) and the full-screen composite.
    pub(super) triangles_oit: gpu::RenderPipeline,
    pub(super) lines_oit: gpu::RenderPipeline,
    pub(super) dots_oit: gpu::RenderPipeline,
    pub(super) oit_composite: gpu::RenderPipeline,
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
        // Opaque geometry writes depth; transparent fills and all screen-space
        // primitives (lines/dots) only test against it so they can never occlude
        // geometry painted after them. Hardware slope-scaled bias is deliberately
        // left at zero: line/dot quads are image-plane-parallel (slope ~ 0), so it
        // would do nothing for them; depth ordering is handled in the shader via
        // a perspective-correct eye-space decal bias plus the paint-rank offset.
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

        // WBOIT accumulation: RT0 additive, RT1 multiplicative (dst *= 1 - src).
        let oit_accum_targets = [
            gpu::ColorTargetState {
                format: OIT_ACCUM_FORMAT,
                blend: Some(gpu::BlendState::ADDITIVE),
                write_mask: gpu::ColorWrites::default(),
            },
            gpu::ColorTargetState {
                format: OIT_REVEAL_FORMAT,
                blend: Some(gpu::BlendState {
                    color: gpu::BlendComponent {
                        src_factor: gpu::BlendFactor::Zero,
                        dst_factor: gpu::BlendFactor::OneMinusSrc,
                        operation: gpu::BlendOperation::Add,
                    },
                    alpha: gpu::BlendComponent {
                        src_factor: gpu::BlendFactor::Zero,
                        dst_factor: gpu::BlendFactor::OneMinusSrc,
                        operation: gpu::BlendOperation::Add,
                    },
                }),
                write_mask: gpu::ColorWrites::default(),
            },
        ];
        // Composite: out = avg * (1 - revealage) + dst * revealage.
        let oit_composite_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: Some(gpu::BlendState {
                color: gpu::BlendComponent {
                    src_factor: gpu::BlendFactor::OneMinusSrcAlpha,
                    dst_factor: gpu::BlendFactor::SrcAlpha,
                    operation: gpu::BlendOperation::Add,
                },
                alpha: gpu::BlendComponent {
                    src_factor: gpu::BlendFactor::OneMinusSrcAlpha,
                    dst_factor: gpu::BlendFactor::SrcAlpha,
                    operation: gpu::BlendOperation::Add,
                },
            }),
            write_mask: gpu::ColorWrites::default(),
        }];

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
                depth_stencil: Some(depth_read_only.clone()),
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
                depth_stencil: Some(depth_read_only.clone()),
                fragment: Some(shader.at("fs_dot")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            triangles_oit: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-triangles-oit",
                data_layouts: &[&TrianglesData::layout()],
                vertex: shader.at("vs_triangle"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    front_face: gpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(depth_read_only.clone()),
                fragment: Some(shader.at("fs_triangle_oit")),
                color_targets: &oit_accum_targets,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            lines_oit: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-lines-oit",
                data_layouts: &[&LinesData::layout()],
                vertex: shader.at("vs_line"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_read_only.clone()),
                fragment: Some(shader.at("fs_line_oit")),
                color_targets: &oit_accum_targets,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            dots_oit: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-dots-oit",
                data_layouts: &[&DotsData::layout()],
                vertex: shader.at("vs_dot"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_read_only),
                fragment: Some(shader.at("fs_dot_oit")),
                color_targets: &oit_accum_targets,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            oit_composite: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-oit-composite",
                data_layouts: &[&OitCompositeData::layout()],
                vertex: shader.at("vs_oit_composite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_oit_composite")),
                color_targets: &oit_composite_target,
                // Composites into the resolved single-sample colour target.
                multisample_state: gpu::MultisampleState {
                    sample_count: 1,
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
        gpu.destroy_render_pipeline(&mut self.triangles_oit);
        gpu.destroy_render_pipeline(&mut self.lines_oit);
        gpu.destroy_render_pipeline(&mut self.dots_oit);
        gpu.destroy_render_pipeline(&mut self.oit_composite);
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
        // The perspective-correct decal offset for lines/dots must stay wired in.
        assert!(source.contains("eye_bias: f32"));
        assert!(source.contains("width_eye * DECAL_SCALE"));
        // The weighted-blended OIT path must stay wired in.
        for needle in [
            "fn fs_triangle_oit",
            "fn fs_line_oit",
            "fn fs_dot_oit",
            "fn fs_oit_composite",
            "fn oit_weight",
        ] {
            assert!(source.contains(needle), "blade.wgsl missing {needle}");
        }

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
                    let result = entry_point
                        .function
                        .result
                        .as_ref()
                        .expect("fragment entry point must return a value");
                    let bound = if result.binding.is_some() {
                        true
                    } else if let naga::TypeInner::Struct { ref members, .. } =
                        module.types[result.ty].inner
                    {
                        // Multiple render targets: every member must be bound.
                        members.iter().all(|member| member.binding.is_some())
                    } else {
                        false
                    };
                    assert!(
                        bound,
                        "fragment entry point '{}' must have explicitly bound output(s)",
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
