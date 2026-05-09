use geo::{mesh::Lin, simd::Float3};

use super::helpers::{push_closed_polyline, push_open_polyline};

const REVERSE_BIT: u8 = 1 << 6;

// original monocurl marching table: reverse, vertical, horizontal, tl, tr, bl, br
const fn case(
    reverse: bool,
    vertical: bool,
    horizontal: bool,
    tl: bool,
    tr: bool,
    bl: bool,
    br: bool,
) -> u8 {
    ((reverse as u8) << 6)
        | ((vertical as u8) << 5)
        | ((horizontal as u8) << 4)
        | ((tl as u8) << 3)
        | ((tr as u8) << 2)
        | ((bl as u8) << 1)
        | (br as u8)
}

const CONTOUR_CASES: [u8; 16] = [
    case(false, false, false, false, false, false, false),
    case(true, false, false, false, false, false, true),
    case(true, false, false, false, false, true, false),
    case(false, false, true, false, false, false, false),
    case(true, false, false, false, true, false, false),
    case(true, false, false, false, true, false, true),
    case(true, true, false, false, false, false, false),
    case(false, false, false, true, false, false, false),
    case(true, false, false, true, false, false, false),
    case(false, true, false, false, false, false, false),
    case(true, false, false, true, false, true, false),
    case(false, false, false, false, true, false, false),
    case(true, false, true, false, false, false, false),
    case(false, false, false, false, false, true, false),
    case(false, false, false, false, false, false, true),
    case(true, false, false, false, false, false, false),
];

pub(super) fn contour_lins(
    sign: &[bool],
    rows: usize,
    cols: usize,
    x0: f32,
    y0: f32,
    x_step: f32,
    y_step: f32,
) -> Vec<Lin> {
    let sign_stride = cols + 2;
    let mut next = vec![None; 2 * (rows + 2) * sign_stride];

    for r in 0..=rows {
        for c in 0..=cols {
            let w = sign[r * sign_stride + c];
            let x = sign[r * sign_stride + c + 1];
            let y = sign[(r + 1) * sign_stride + c + 1];
            let z = sign[(r + 1) * sign_stride + c];
            let mask =
                ((w as usize) << 3) | ((x as usize) << 2) | ((y as usize) << 1) | (z as usize);
            let result = CONTOUR_CASES[mask];
            let args = [
                2 * (r * sign_stride + c),
                2 * (r * sign_stride + c) + 1,
                2 * (r * sign_stride + c + 1),
                2 * ((r + 1) * sign_stride + c) + 1,
            ];
            let reverse = result & REVERSE_BIT != 0;

            for i in 0..4 {
                if result & (1 << (3 - i)) != 0 {
                    let mut line = [args[i], args[(i + 1) % 4]];
                    if reverse {
                        line.swap(0, 1);
                    }
                    next[line[0]] = Some(line[1]);
                }
            }

            for i in 0..2 {
                if result & (1 << (i + 4)) != 0 {
                    let mut line = [args[i], args[(i + 2) % 4]];
                    if reverse {
                        line.swap(0, 1);
                    }
                    next[line[0]] = Some(line[1]);
                }
            }
        }
    }

    let mut lins = Vec::new();
    for q in 0..next.len() {
        if next[q].is_none() {
            continue;
        }

        let mut p = q;
        let mut points = vec![contour_point(q, rows, cols, x0, y0, x_step, y_step)];
        let mut closed = false;
        while let Some(n) = next[p] {
            next[p] = None;
            if n == q {
                closed = true;
                break;
            }

            points.push(contour_point(n, rows, cols, x0, y0, x_step, y_step));
            p = n;
        }

        if points.len() >= 2 {
            if closed {
                push_closed_polyline(&mut lins, &points, Float3::Z);
            } else {
                push_open_polyline(&mut lins, &points, Float3::Z);
            }
        }
    }

    lins
}

fn contour_point(
    index: usize,
    rows: usize,
    cols: usize,
    x0: f32,
    y0: f32,
    x_step: f32,
    y_step: f32,
) -> Float3 {
    let is_up = index % 2 == 0;
    let row = (index / 2) / (cols + 2);
    let col = (index / 2) % (cols + 2);

    let y = if row == 0 {
        y0
    } else if row == rows {
        y0 + (rows - 1) as f32 * y_step
    } else if is_up {
        y0 + (row as f32 - 0.5) * y_step
    } else {
        y0 + (row as f32 - 1.0) * y_step
    };

    let x = if col == 0 {
        x0
    } else if col == cols {
        x0 + (cols - 1) as f32 * x_step
    } else if is_up {
        x0 + (col as f32 - 1.0) * x_step
    } else {
        x0 + (col as f32 - 0.5) * x_step
    };

    Float3::new(x, y, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contour_lins_emits_closed_linked_boundary() {
        let rows = 2;
        let cols = 2;
        let stride = cols + 2;
        let mut sign = vec![false; (rows + 2) * stride];
        for r in 0..rows {
            for c in 0..cols {
                sign[(r + 1) * stride + c + 1] = true;
            }
        }

        let lins = contour_lins(&sign, rows, cols, -1.0, -1.0, 2.0, 2.0);

        assert!(!lins.is_empty());
        for (idx, lin) in lins.iter().enumerate() {
            assert!(lin.prev >= 0, "line {idx} should have a previous link");
            assert!(lin.next >= 0, "line {idx} should have a next link");
            assert_eq!(lins[lin.prev as usize].next, idx as i32);
            assert_eq!(lins[lin.next as usize].prev, idx as i32);
        }
    }

    #[test]
    fn contour_lins_skips_empty_sign_grid() {
        let rows = 2;
        let cols = 2;
        let sign = vec![false; (rows + 2) * (cols + 2)];

        let lins = contour_lins(&sign, rows, cols, -1.0, -1.0, 2.0, 2.0);

        assert!(lins.is_empty());
    }
}
