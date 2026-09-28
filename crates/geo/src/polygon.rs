//! constrained delaunay triangulation of one small simple polygon, for the
//! shapes scenes rebuild every frame (arrows, capsules, user polygons). ear
//! clipping followed by lawson flips reaches the triangulation libtess2 would
//! produce, without its allocation and sweep setup. anything degenerate or
//! self-intersecting is declined so the caller can fall back to the general
//! tessellator

/// triangulate `points`, a closed loop in either winding. faces come back
/// counter-clockwise and index into `points`. `None` means the polygon is not
/// simple (or is too flat to tell), never that it is empty
pub fn triangulate_simple_polygon(points: &[[f64; 2]]) -> Option<Vec<[usize; 3]>> {
    let n = points.len();
    if n < 3 {
        return None;
    }

    let extent = points
        .iter()
        .fold(0.0f64, |acc, p| acc.max(p[0].abs()).max(p[1].abs()));
    let eps = extent * extent * 1e-10;
    // cocircular vertices (every regular polygon) must not oscillate between
    // equally good diagonals, so the circle test has a looser tolerance
    let circle_eps = extent.powi(4) * 1e-9;
    let area = (0..n)
        .map(|i| cross(points[i], points[(i + 1) % n]))
        .sum::<f64>();
    if area.abs() <= eps {
        return None;
    }

    // work counter-clockwise; `order` maps back to the caller's indices
    let order: Vec<usize> = if area > 0.0 {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };
    let at = |i: usize| points[order[i]];

    for i in 0..n {
        let (a, b, c) = (at(i), at((i + 1) % n), at((i + 2) % n));
        if orient(a, b, c).abs() <= eps {
            return None;
        }
    }
    if !is_simple(&order, points, eps) {
        return None;
    }

    let mut faces = ear_clip(n, &at, eps)?;
    lawson_flip(&mut faces, &at, eps, circle_eps);
    for face in &mut faces {
        *face = face.map(|i| order[i]);
    }
    Some(faces)
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// twice the signed area of `abc`: positive when counter-clockwise
fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    cross([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]])
}

/// whether `d` lies strictly inside the circumcircle of the counter-clockwise
/// triangle `abc`
fn in_circle(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], circle_eps: f64) -> bool {
    let (ax, ay) = (a[0] - d[0], a[1] - d[1]);
    let (bx, by) = (b[0] - d[0], b[1] - d[1]);
    let (cx, cy) = (c[0] - d[0], c[1] - d[1]);
    let det = (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay)
        + (cx * cx + cy * cy) * (ax * by - bx * ay);
    det > circle_eps
}

fn segments_touch(p: [f64; 2], q: [f64; 2], r: [f64; 2], s: [f64; 2], eps: f64) -> bool {
    let d1 = orient(p, q, r);
    let d2 = orient(p, q, s);
    let d3 = orient(r, s, p);
    let d4 = orient(r, s, q);
    let straddles = |x: f64, y: f64| (x > eps && y < -eps) || (x < -eps && y > eps);
    if straddles(d1, d2) && straddles(d3, d4) {
        return true;
    }
    let on_segment = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: f64| {
        d.abs() <= eps
            && c[0] >= a[0].min(b[0]) - eps
            && c[0] <= a[0].max(b[0]) + eps
            && c[1] >= a[1].min(b[1]) - eps
            && c[1] <= a[1].max(b[1]) + eps
    };
    on_segment(p, q, r, d1)
        || on_segment(p, q, s, d2)
        || on_segment(r, s, p, d3)
        || on_segment(r, s, q, d4)
}

fn is_simple(order: &[usize], points: &[[f64; 2]], eps: f64) -> bool {
    let n = order.len();
    let at = |i: usize| points[order[i % n]];
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_touch(at(i), at(i + 1), at(j), at(j + 1), eps) {
                return false;
            }
        }
    }
    true
}

fn point_in_triangle(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2], eps: f64) -> bool {
    orient(a, b, p) >= -eps && orient(b, c, p) >= -eps && orient(c, a, p) >= -eps
}

fn ear_clip(n: usize, at: &dyn Fn(usize) -> [f64; 2], eps: f64) -> Option<Vec<[usize; 3]>> {
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    let mut faces = Vec::with_capacity(n - 2);
    let mut remaining = n;
    let mut cursor = 0;

    while remaining > 3 {
        let mut clipped = false;
        for _ in 0..remaining {
            let (p, c, q) = (prev[cursor], cursor, next[cursor]);
            let (a, b, d) = (at(p), at(c), at(q));
            let convex = orient(a, b, d) > eps;
            let is_ear = convex
                && (0..n).all(|other| {
                    !alive[other]
                        || other == p
                        || other == c
                        || other == q
                        || !point_in_triangle(at(other), a, b, d, eps)
                });
            if is_ear {
                faces.push([p, c, q]);
                alive[c] = false;
                next[p] = q;
                prev[q] = p;
                remaining -= 1;
                cursor = q;
                clipped = true;
                break;
            }
            cursor = q;
        }
        if !clipped {
            return None;
        }
    }
    let first = (0..n).find(|&i| alive[i])?;
    faces.push([prev[first], first, next[first]]);
    Some(faces)
}

/// flip diagonals until every interior edge is locally delaunay
fn lawson_flip(
    faces: &mut [[usize; 3]],
    at: &dyn Fn(usize) -> [f64; 2],
    eps: f64,
    circle_eps: f64,
) {
    let limit = 8 * faces.len() * faces.len() + 8;
    for _ in 0..limit {
        let Some((t, u, ta, tb, p, q)) = find_flippable(faces, at, eps, circle_eps) else {
            return;
        };
        faces[t] = [p, ta, q];
        faces[u] = [q, tb, p];
    }
}

/// an interior edge `ta -> tb` of face `t`, shared with face `u`, whose
/// opposite vertices `p` (in `t`) and `q` (in `u`) violate the delaunay
/// condition across a convex quad
fn find_flippable(
    faces: &[[usize; 3]],
    at: &dyn Fn(usize) -> [f64; 2],
    eps: f64,
    circle_eps: f64,
) -> Option<(usize, usize, usize, usize, usize, usize)> {
    for (t, face) in faces.iter().enumerate() {
        for edge in 0..3 {
            let ta = face[edge];
            let tb = face[(edge + 1) % 3];
            let p = face[(edge + 2) % 3];
            let Some((u, q)) = faces.iter().enumerate().find_map(|(u, other)| {
                (u != t)
                    .then(|| {
                        (0..3).find_map(|k| {
                            (other[k] == tb && other[(k + 1) % 3] == ta).then(|| other[(k + 2) % 3])
                        })
                    })
                    .flatten()
                    .map(|q| (u, q))
            }) else {
                continue;
            };
            let (a, b, pp, qq) = (at(ta), at(tb), at(p), at(q));
            let convex = orient(pp, qq, a) < -eps && orient(pp, qq, b) > eps;
            if convex && in_circle(a, b, pp, qq, circle_eps) {
                return Some((t, u, ta, tb, p, q));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_area(points: &[[f64; 2]], face: [usize; 3]) -> f64 {
        orient(points[face[0]], points[face[1]], points[face[2]]) / 2.0
    }

    fn total_area(points: &[[f64; 2]]) -> f64 {
        let n = points.len();
        (0..n)
            .map(|i| cross(points[i], points[(i + 1) % n]))
            .sum::<f64>()
            .abs()
            / 2.0
    }

    fn check(points: &[[f64; 2]]) -> Vec<[usize; 3]> {
        let faces = triangulate_simple_polygon(points).expect("polygon should triangulate");
        assert_eq!(faces.len(), points.len() - 2);
        let covered: f64 = faces.iter().map(|&face| face_area(points, face)).sum();
        assert!((covered - total_area(points)).abs() < 1e-9 * total_area(points).max(1.0));
        assert!(faces.iter().all(|&face| face_area(points, face) > 0.0));
        faces
    }

    #[test]
    fn convex_polygons_of_either_winding_triangulate_counter_clockwise() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        check(&square);
        let mut reversed = square;
        reversed.reverse();
        check(&reversed);
    }

    #[test]
    fn arrow_shapes_triangulate() {
        let arrow = [
            [0.0, -0.1],
            [2.0, -0.1],
            [2.0, -0.4],
            [3.0, 0.0],
            [2.0, 0.4],
            [2.0, 0.1],
            [0.0, 0.1],
        ];
        check(&arrow);
    }

    #[test]
    fn regular_polygon_edges_are_delaunay() {
        let points: Vec<_> = (0..24)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / 24.0;
                [theta.cos(), theta.sin()]
            })
            .collect();
        let faces = check(&points);
        let at = |i: usize| points[i];
        assert!(find_flippable(&faces, &at, 1e-10, 1e-9).is_none());
    }

    #[test]
    fn self_intersecting_and_degenerate_loops_are_declined() {
        let bowtie = [[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]];
        assert!(triangulate_simple_polygon(&bowtie).is_none());
        let line = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        assert!(triangulate_simple_polygon(&line).is_none());
        let repeated = [[0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        assert!(triangulate_simple_polygon(&repeated).is_none());
        let collinear_corner = [[0.0, 0.0], [0.5, 0.0], [1.0, 0.0], [0.0, 1.0]];
        assert!(triangulate_simple_polygon(&collinear_corner).is_none());
    }

    #[test]
    fn a_comb_shaped_polygon_needs_reflex_aware_ears() {
        let comb = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 3.0],
            [5.0, 3.0],
            [5.0, 1.0],
            [4.0, 1.0],
            [4.0, 3.0],
            [3.0, 3.0],
            [3.0, 1.0],
            [2.0, 1.0],
            [2.0, 3.0],
            [1.0, 3.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];
        check(&comb);
    }
}
