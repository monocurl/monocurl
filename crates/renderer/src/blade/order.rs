//! Pure, GPU-free draw-ordering logic for the scene pass.
//!
//! The renderer draws in a single pass but in two logical phases per `z_index`
//! group: opaque meshes first (in scene-declaration order, writing depth), then
//! transparent meshes back-to-front (testing but not writing depth). `z_index`
//! stays the dominant, explicit, user-facing key; camera-space depth is only a
//! tiebreaker inside the transparent phase.

use std::cmp::Ordering;

use geo::simd::Float3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SortKey {
    /// Explicit user ordering. Always dominant.
    pub(super) z_index: i32,
    /// `false` (opaque) sorts before `true` (transparent) within a `z_index`.
    pub(super) transparent: bool,
    /// Camera-space depth of the mesh centroid; larger = farther from camera.
    pub(super) depth: f32,
    /// Scene declaration index; final stable tiebreaker.
    pub(super) order: usize,
}

/// A mesh is transparent (depth-write off, back-to-front) if its uniform alpha is
/// below 1, any rendered vertex carries a partial alpha, or it samples a texture
/// that has any sub-opaque texel.
pub(super) fn is_transparent(
    alpha: f64,
    translucent_vertices: bool,
    texture_has_alpha: bool,
) -> bool {
    alpha < 1.0 || translucent_vertices || texture_has_alpha
}

/// Signed distance of `centroid` in front of the camera along its view axis.
/// Positive is in front; larger is farther away.
pub(super) fn camera_depth(centroid: Float3, position: Float3, forward: Float3) -> f32 {
    (centroid - position).dot(forward)
}

/// Perspective-safe NDC depth offsets for one draw item's primitive groups,
/// keyed to the item's rank in canonical `(z_index, order)` order. Because the
/// key is the *declaration* rank and not the (reordered) draw sequence, the
/// depth-test outcome for coplanar meshes is independent of the transparency
/// split. `triangles < lines < dots` within an item, and every group of a
/// lower-ranked item sorts strictly before every group of a higher-ranked one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RankBias {
    pub(super) tri: f32,
    pub(super) line: f32,
    pub(super) dot: f32,
}

pub(super) fn rank_bias(rank: usize, item_count: usize, depth_step: f32) -> RankBias {
    // Keep the total pull bounded for very large scenes; degrades gracefully to
    // ties (which `LessEqual` + draw order still resolve as "later wins").
    let step = depth_step.min(2.0e-3 / (3 * item_count.max(1)) as f32);
    let base = 3.0 * rank as f32 * step;
    RankBias {
        tri: base,
        line: base + step,
        dot: base + 2.0 * step,
    }
}

/// Total order over draw items:
/// `z_index` asc, then opaque before transparent, then (transparent only)
/// farthest-first, then declaration order.
pub(super) fn draw_order_cmp(a: &SortKey, b: &SortKey) -> Ordering {
    a.z_index
        .cmp(&b.z_index)
        .then_with(|| a.transparent.cmp(&b.transparent))
        .then_with(|| {
            if a.transparent {
                // Farther meshes (larger depth) are drawn first.
                b.depth.total_cmp(&a.depth)
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| a.order.cmp(&b.order))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(z_index: i32, transparent: bool, depth: f32, order: usize) -> SortKey {
        SortKey {
            z_index,
            transparent,
            depth,
            order,
        }
    }

    fn sorted(mut keys: Vec<SortKey>) -> Vec<usize> {
        keys.sort_by(draw_order_cmp);
        keys.into_iter().map(|k| k.order).collect()
    }

    #[test]
    fn flat_scene_is_pure_declaration_order_within_z_index() {
        // All opaque, all equal depth: result must equal a plain (z_index, order) sort.
        let keys = vec![
            key(0, false, 0.0, 2),
            key(1, false, 0.0, 0),
            key(0, false, 0.0, 1),
            key(1, false, 0.0, 3),
        ];
        // Plain (z_index, order) sort: (0,1), (0,2), (1,0), (1,3).
        assert_eq!(sorted(keys), vec![1, 2, 0, 3]);
    }

    #[test]
    fn phase_split_is_scoped_to_each_z_index_group() {
        // A transparent item at z_index 0 still precedes everything at z_index 1.
        let keys = vec![
            key(1, false, 5.0, 0),
            key(0, true, 1.0, 1),
            key(0, false, 9.0, 2),
        ];
        assert_eq!(sorted(keys), vec![2, 1, 0]);
    }

    #[test]
    fn opaque_before_transparent_and_opaque_keeps_declaration_order() {
        let keys = vec![
            key(0, true, 100.0, 0),
            key(0, false, 1.0, 1),
            key(0, false, 50.0, 2),
        ];
        // opaque 1 then 2 (declaration order, depth ignored), then transparent 0.
        assert_eq!(sorted(keys), vec![1, 2, 0]);
    }

    #[test]
    fn transparent_is_farthest_first_with_stable_tiebreak() {
        let keys = vec![
            key(0, true, 1.0, 0),
            key(0, true, 9.0, 1),
            key(0, true, 5.0, 2),
            key(0, true, 9.0, 3), // equal depth to order 1 -> declaration order wins
        ];
        assert_eq!(sorted(keys), vec![1, 3, 2, 0]);
    }

    #[test]
    fn nan_depth_does_not_panic_and_stays_total() {
        let keys = vec![
            key(0, true, f32::NAN, 0),
            key(0, true, 1.0, 1),
            key(0, true, f32::NAN, 2),
        ];
        // Just assert it produces a stable permutation of all inputs.
        let out = sorted(keys);
        assert_eq!(out.len(), 3);
        assert!(out.contains(&0) && out.contains(&1) && out.contains(&2));
    }

    #[test]
    fn rank_bias_is_globally_monotonic_across_items_and_groups() {
        let count = 6;
        let mut prev = f32::NEG_INFINITY;
        for rank in 0..count {
            let b = rank_bias(rank, count, 1e-6);
            for v in [b.tri, b.line, b.dot] {
                assert!(v > prev, "bias not strictly increasing at rank {rank}: {v} <= {prev}");
                prev = v;
            }
        }
    }

    #[test]
    fn rank_bias_stays_below_step_cap_for_huge_scenes() {
        let count = 10_000;
        let last = rank_bias(count - 1, count, 1e-6);
        assert!(last.dot < 2.0e-3, "total depth pull {} exceeded budget", last.dot);
        let first = rank_bias(0, count, 1e-6);
        assert!(first.tri < first.line && first.line < first.dot);
    }

    #[test]
    fn classifier_truth_table() {
        assert!(!is_transparent(1.0, false, false));
        assert!(is_transparent(0.5, false, false));
        assert!(is_transparent(1.0, true, false));
        assert!(is_transparent(1.0, false, true));
    }

    #[test]
    fn camera_depth_positive_in_front_rotated_basis() {
        // Camera at (0,0,5) looking toward -z: forward = (0,0,-1).
        let position = Float3::new(0.0, 0.0, 5.0);
        let forward = Float3::new(0.0, 0.0, -1.0);
        // Point at origin is 5 units in front.
        assert_eq!(camera_depth(Float3::ZERO, position, forward), 5.0);
        // Point behind the camera is negative.
        assert_eq!(
            camera_depth(Float3::new(0.0, 0.0, 10.0), position, forward),
            -5.0
        );

        // Non-axis-aligned: forward along normalized (1,1,0).
        let inv = 1.0 / 2.0_f32.sqrt();
        let forward = Float3::new(inv, inv, 0.0);
        let position = Float3::ZERO;
        let centroid = Float3::new(3.0, 1.0, 7.0);
        assert!((camera_depth(centroid, position, forward) - 4.0 * inv).abs() < 1e-5);
    }
}
