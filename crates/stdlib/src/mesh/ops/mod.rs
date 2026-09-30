//! mesh operators: the natives behind `shift{...}`, `stroke{...}`, `point_map{...}`
//! and friends. shared readers and tree helpers live here; the operators are
//! grouped by what they touch

mod dashed;
mod placement;
mod remap;
mod styling;
mod topology;

use std::{future::Future, pin::Pin};

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{mesh::Mesh, simd::Float3};

use super::helpers::*;

fn read_scale_factor(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float3, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::Integer(value) => Ok(Float3::splat(value as f32)),
        Value::Float(value) => Ok(Float3::splat(value as f32)),
        Value::List(_) => read_float3(executor, stack_idx, index, name),
        other => Err(ExecutorError::type_error_for(
            "float or list of length 3",
            other.type_name(),
            name,
        )),
    }
}

fn read_level(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<f32, ExecutorError> {
    Ok(crate::read_float(executor, stack_idx, index, name)?.clamp(0.0, 1.0) as f32)
}

fn recolor_mesh(mesh: &mut Mesh, color: geo::simd::Float4, level: f32) {
    for dot in &mut mesh.dots {
        dot.col = dot.col.lerp(color, level);
    }
    for lin in &mut mesh.lins {
        lin.a.col = lin.a.col.lerp(color, level);
        lin.b.col = lin.b.col.lerp(color, level);
    }
    for tri in &mut mesh.tris {
        tri.a.col = tri.a.col.lerp(color, level);
        tri.b.col = tri.b.col.lerp(color, level);
        tri.c.col = tri.c.col.lerp(color, level);
    }
}

fn transform_hint_normal(normal: Float3, x_unit: Float3, y_unit: Float3, z_unit: Float3) -> Float3 {
    let mapped = x_unit * normal.x + y_unit * normal.y + z_unit * normal.z;
    if mapped.len_sq() > 1e-12 {
        mapped.normalize()
    } else {
        normal
    }
}

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn filtered_tree_view<'a>(
    executor: &'a mut Executor,
    tree: &'a MeshTree,
    filter: Option<&'a TagFilter>,
) -> Pin<Box<dyn Future<Output = Result<Option<MeshTree>, ExecutorError>> + 'a>> {
    Box::pin(async move {
        match filter {
            Some(filter) => filter_tree_by_tag_filter(executor, tree.clone(), filter).await,
            None => Ok(Some(tree.clone())),
        }
    })
}

async fn affected_tree_center(
    executor: &mut Executor,
    tree: &MeshTree,
    filter: Option<&TagFilter>,
) -> Result<Option<Float3>, ExecutorError> {
    let Some(view) = filtered_tree_view(executor, tree, filter).await? else {
        return Ok(None);
    };
    Ok(Some(tree_center(&view).unwrap_or(Float3::ZERO)))
}

fn blend_mesh_positions(mesh: &mut Mesh, level: f32, map: impl Fn(Float3) -> Float3) {
    for dot in &mut mesh.dots {
        let original = dot.pos;
        dot.pos = original.lerp(map(original), level);
    }
    for lin in &mut mesh.lins {
        let original = lin.a.pos;
        lin.a.pos = original.lerp(map(original), level);
        let original = lin.b.pos;
        lin.b.pos = original.lerp(map(original), level);
    }
    for tri in &mut mesh.tris {
        let original = tri.a.pos;
        tri.a.pos = original.lerp(map(original), level);
        let original = tri.b.pos;
        tri.b.pos = original.lerp(map(original), level);
        let original = tri.c.pos;
        tri.c.pos = original.lerp(map(original), level);
    }
}

#[cfg(test)]
mod tests {
    use geo::{
        mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
        mesh_build::{BoundaryEdges, mesh_ref},
        simd::{Float2, Float3, Float4},
    };

    use geo::mesh_build::{BoundaryEdge, IndexedSurface, SurfaceVertex};

    use super::{
        MeshTree, build_indexed_surface,
        dashed::{dashed_lines, dashed_mesh, push_dashed_segment},
        default_lin, recolor_mesh,
        topology::{subdivide_indexed_surface, subdivide_line_mesh},
    };

    fn square_surface() -> IndexedSurface {
        let vertices = vec![
            SurfaceVertex {
                pos: Float3::new(-1.0, -1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, -1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(-1.0, 1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3]];
        let boundary_edges = BoundaryEdges::from_iter([
            (
                (0, 1),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
            (
                (1, 2),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
            (
                (2, 3),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
            (
                (3, 0),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
        ]);

        IndexedSurface {
            vertices,
            faces,
            boundary_edges,
        }
    }

    #[test]
    fn subdivide_surface_authors_consistent_boundary_topology() {
        let surface = subdivide_indexed_surface(&square_surface());
        let (lins, tris) =
            build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
        let mesh = Mesh {
            dots: Default::default(),
            lins: lins.into(),
            tris: tris.into(),
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert_eq!(mesh.tris.len(), 8);
        assert_eq!(mesh.lins.len(), 8);
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn subdivide_lines_remaps_endpoint_dot_backrefs() {
        let mut mesh = Mesh {
            dots: Default::default(),
            lins: vec![default_lin(
                Float3::new(-2.0, -2.0, 0.0),
                Float3::new(-1.0, -2.0, 0.0),
                Float3::Z,
            )]
            .into(),
            tris: Default::default(),
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        mesh.normalize_line_dot_topology();

        subdivide_line_mesh(&mut mesh, 3);

        assert_eq!(mesh.lins.len(), 6);
        assert_eq!(mesh.dots.len(), 4);
        assert_eq!(mesh.dots[0].inv, mesh_ref(0));
        assert_eq!(mesh.dots[1].inv, mesh_ref(2));
        assert_eq!(mesh.dots[2].inv, mesh_ref(3));
        assert_eq!(mesh.dots[3].inv, mesh_ref(5));
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn revolved_strip_authors_consistent_boundary_topology() {
        let vertices = vec![
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 1.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 1.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
        ];
        let faces = vec![[0, 1, 3], [0, 3, 2]];
        let (lins, tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        let mesh = Mesh {
            dots: Default::default(),
            lins: lins.into(),
            tris: tris.into(),
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert!(!mesh.lins.is_empty());
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn recolor_mesh_updates_dots_lines_and_tris() {
        let start = Float4::new(0.2, 0.3, 0.4, 1.0);
        let target = Float4::new(0.8, 0.1, 0.6, 1.0);
        let mut mesh = Mesh {
            dots: vec![Dot {
                pos: Float3::ZERO,
                norm: Float3::Z,
                col: start,
                inv: -1,
                is_dom_sib: false,
            }]
            .into(),
            lins: vec![Lin {
                a: LinVertex {
                    pos: Float3::ZERO,
                    col: start,
                },
                b: LinVertex {
                    pos: Float3::X,
                    col: start,
                },
                norm: Float3::Z,
                prev: -1,
                next: -1,
                inv: -1,
                is_dom_sib: false,
            }]
            .into(),
            tris: vec![Tri {
                a: TriVertex {
                    pos: Float3::ZERO,
                    col: start,
                    uv: Float2::ZERO,
                },
                b: TriVertex {
                    pos: Float3::X,
                    col: start,
                    uv: Float2::ZERO,
                },
                c: TriVertex {
                    pos: Float3::Y,
                    col: start,
                    uv: Float2::ZERO,
                },
                ab: -1,
                bc: -1,
                ca: -1,
                is_dom_sib: false,
            }]
            .into(),
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        recolor_mesh(&mut mesh, target, 1.0);

        let approx_eq =
            |lhs: Float4, rhs: Float4| (lhs - rhs).to_array().into_iter().all(|x| x.abs() < 1e-6);

        assert!(approx_eq(mesh.dots[0].col, target));
        assert!(approx_eq(mesh.lins[0].a.col, target));
        assert!(approx_eq(mesh.lins[0].b.col, target));
        assert!(approx_eq(mesh.tris[0].a.col, target));
        assert!(approx_eq(mesh.tris[0].b.col, target));
        assert!(approx_eq(mesh.tris[0].c.col, target));
    }

    #[test]
    fn dashed_lines_keep_continuous_visible_piece_links() {
        let mut first = default_lin(Float3::ZERO, Float3::X, Float3::Z);
        let mut second = default_lin(Float3::X, Float3::new(2.0, 0.0, 0.0), Float3::Z);
        first.next = 1;
        second.prev = 0;

        let dashed = dashed_lines(&[first, second], 1.5, 0.5, 0.0);
        let mesh = Mesh {
            dots: Default::default(),
            lins: dashed.clone().into(),
            tris: Default::default(),
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert_eq!(dashed.len(), 2);
        assert_eq!(dashed[0].next, 1);
        assert_eq!(dashed[1].prev, 0);
        assert_eq!(dashed[0].a.pos, Float3::ZERO);
        assert_eq!(dashed[0].b.pos, Float3::X);
        assert_eq!(dashed[1].a.pos, Float3::X);
        assert_eq!(dashed[1].b.pos, Float3::new(1.5, 0.0, 0.0));
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn dashed_segments_only_link_when_endpoints_match() {
        let mut out = Vec::new();
        let mut current_piece_last = None;
        let first = default_lin(Float3::ZERO, Float3::X, Float3::Z);
        let second = default_lin(
            Float3::new(2.0, 0.0, 0.0),
            Float3::new(3.0, 0.0, 0.0),
            Float3::Z,
        );

        push_dashed_segment(&mut out, &mut current_piece_last, &first, 0.0, 1.0);
        push_dashed_segment(&mut out, &mut current_piece_last, &second, 0.0, 1.0);

        assert_eq!(out[0].next, -1);
        assert_eq!(out[1].prev, -1);
    }

    #[test]
    fn dashed_segments_snap_near_endpoints_before_linking() {
        let mut out = Vec::new();
        let mut current_piece_last = None;
        let first = default_lin(Float3::ZERO, Float3::X, Float3::Z);
        let second = default_lin(Float3::X, Float3::new(2.0, 0.0, 0.0), Float3::Z);

        push_dashed_segment(&mut out, &mut current_piece_last, &first, 0.0, 0.99999994);
        push_dashed_segment(&mut out, &mut current_piece_last, &second, 0.0, 0.5);

        assert_eq!(out[0].b.pos, Float3::X);
        assert_eq!(out[0].next, 1);
        assert_eq!(out[1].prev, 0);
        assert_eq!(out[1].a.pos, Float3::X);
    }

    #[test]
    fn dashed_surface_splits_fill_and_stroke_meshes() {
        let surface = square_surface();
        let (lins, tris) =
            build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
        let mesh = Mesh {
            dots: Default::default(),
            lins: lins.into(),
            tris: tris.into(),
            uniform: Uniforms::default(),
            tag: vec![7],
            version: Mesh::fresh_version(),
        };
        let MeshTree::List(children) = dashed_mesh(&mesh, 0.6, 0.4, 0.0) else {
            panic!("expected dashed surface to split into child meshes");
        };
        assert_eq!(children.len(), 2);

        let MeshTree::Mesh(base) = &children[0] else {
            panic!("expected base mesh");
        };
        let MeshTree::Mesh(stroke) = &children[1] else {
            panic!("expected dashed stroke mesh");
        };

        assert_eq!(base.tris.len(), mesh.tris.len());
        assert!(base.lins.is_empty());
        assert!(stroke.tris.is_empty());
        assert!(stroke.lins.len() > mesh.lins.len());
        assert_eq!(base.tag, mesh.tag);
        assert_eq!(stroke.tag, mesh.tag);
        assert!(base.has_consistent_topology());
        assert!(stroke.has_consistent_topology());
    }
}
