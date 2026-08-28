//! CPU reference for the weighted-blended OIT math implemented in `blade.wgsl`
//! (`oit_weight` / `oit_fragment` / `fs_oit_composite`). Kept in sync by eye;
//! its job in tests is to pin down the compositing algebra and, above all, the
//! order-independence that WBOIT buys over the previous sorted-blend path.

/// McGuire/Bavoil 2013 eq. 10 weight: emphasise nearer fragments, bounded so
/// the fp16 accumulation target cannot overflow. `view_z` is camera-space depth
/// (sign irrelevant).
pub(super) fn oit_weight(view_z: f32, alpha: f32) -> f32 {
    let z = view_z.abs().max(1e-4);
    let a = z / 5.0;
    let b = z / 200.0;
    let denom = 1e-5 + a * a + b.powi(6);
    alpha * (10.0 / denom).clamp(1e-2, 3e3)
}

/// Accumulates one fragment (straight-alpha `rgb`, coverage `alpha`, camera
/// depth `z`) into `(accum, revealage)` exactly as the GPU accumulation pass:
/// `accum` is additive (premultiplied colour * weight), `revealage` is the
/// running product of `1 - alpha`.
pub(super) fn accumulate(
    accum: &mut [f32; 4],
    revealage: &mut f32,
    rgb: [f32; 3],
    alpha: f32,
    z: f32,
) {
    let w = oit_weight(z, alpha);
    accum[0] += rgb[0] * alpha * w;
    accum[1] += rgb[1] * alpha * w;
    accum[2] += rgb[2] * alpha * w;
    accum[3] += alpha * w;
    *revealage *= 1.0 - alpha;
}

/// Full-screen composite: `out = avg * (1 - revealage) + dst * revealage`,
/// where `avg = accum.rgb / max(accum.a, eps)`.
pub(super) fn composite(accum: [f32; 4], revealage: f32, dst: [f32; 3]) -> [f32; 3] {
    if revealage >= 1.0 {
        return dst;
    }
    let denom = accum[3].max(1e-5);
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let avg = accum[i] / denom;
        out[i] = avg * (1.0 - revealage) + dst[i] * revealage;
    }
    out
}

/// Convenience: accumulate every fragment then composite over `dst`.
pub(super) fn resolve(fragments: &[([f32; 3], f32, f32)], dst: [f32; 3]) -> [f32; 3] {
    let mut accum = [0.0f32; 4];
    let mut revealage = 1.0f32;
    for &(rgb, alpha, z) in fragments {
        accumulate(&mut accum, &mut revealage, rgb, alpha, z);
    }
    composite(accum, revealage, dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DST: [f32; 3] = [0.2, 0.2, 0.2];

    fn close(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= eps)
    }

    #[test]
    fn weight_is_positive_and_bounded() {
        for &z in &[0.0_f32, 0.01, 1.0, 5.0, 50.0, 200.0, 5_000.0] {
            let w = oit_weight(z, 1.0);
            assert!(w >= 1e-2 - 1e-6 && w <= 3e3 + 1.0, "z={z} w={w}");
        }
        // weight scales linearly with alpha
        assert!((oit_weight(3.0, 0.5) - 0.5 * oit_weight(3.0, 1.0)).abs() < 1e-4);
    }

    #[test]
    fn no_fragments_leaves_destination_untouched() {
        assert_eq!(resolve(&[], DST), DST);
    }

    #[test]
    fn fully_opaque_transparent_fragment_replaces_destination() {
        // alpha == 1 -> revealage == 0, avg == the fragment colour.
        let out = resolve(&[([0.8, 0.1, 0.3], 1.0, 4.0)], DST);
        assert!(close(out, [0.8, 0.1, 0.3], 1e-4), "{out:?}");
    }

    #[test]
    fn single_half_alpha_fragment_is_half_colour_half_destination() {
        let color = [0.9, 0.4, 0.1];
        let out = resolve(&[(color, 0.5, 4.0)], DST);
        let want = [
            0.5 * color[0] + 0.5 * DST[0],
            0.5 * color[1] + 0.5 * DST[1],
            0.5 * color[2] + 0.5 * DST[2],
        ];
        assert!(close(out, want, 1e-4), "{out:?} vs {want:?}");
    }

    #[test]
    fn resolve_is_independent_of_fragment_order() {
        // Three overlapping translucent fragments at assorted depths.
        let frags = [
            ([1.0, 0.0, 0.0], 0.5, 2.0),
            ([0.0, 1.0, 0.0], 0.4, 6.0),
            ([0.0, 0.0, 1.0], 0.6, 4.0),
        ];
        let base = resolve(&frags, DST);
        let perms = [
            [frags[0], frags[2], frags[1]],
            [frags[1], frags[0], frags[2]],
            [frags[1], frags[2], frags[0]],
            [frags[2], frags[0], frags[1]],
            [frags[2], frags[1], frags[0]],
        ];
        for p in &perms {
            let got = resolve(p, DST);
            assert!(
                close(base, got, 1e-4),
                "WBOIT resolve order-dependent: {base:?} vs {got:?}"
            );
        }
    }

    #[test]
    fn revealage_tracks_combined_coverage() {
        // Two 50%-coverage fragments leave 25% of the background showing.
        let mut accum = [0.0; 4];
        let mut revealage = 1.0;
        accumulate(&mut accum, &mut revealage, [1.0, 1.0, 1.0], 0.5, 3.0);
        accumulate(&mut accum, &mut revealage, [1.0, 1.0, 1.0], 0.5, 3.0);
        assert!((revealage - 0.25).abs() < 1e-6);
    }
}
