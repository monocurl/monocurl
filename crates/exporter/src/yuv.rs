//! bgra frames to the i420 planes the h.264 encoder takes

use openh264::formats::YUVSlices;

/// i420 planes of one frame size, reused from frame to frame
pub(crate) struct I420Frame {
    width: usize,
    height: usize,
    planes: Vec<u8>,
}

impl I420Frame {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        assert!(
            width.is_multiple_of(2) && height.is_multiple_of(2),
            "i420 needs even dimensions"
        );
        Self {
            width,
            height,
            planes: vec![0; width * height * 3 / 2],
        }
    }

    /// writes the same bytes as `YUVBuffer::from_rgb_source(BgraSliceU8)`. its float math
    /// is exact (coefficients are multiples of 1/256, chroma inputs are quarters of integer
    /// sums, everything stays below 2^24) and its results are positive, so the truncating
    /// casts are these integer shifts. it walks rows instead of columns, which is what made
    /// it slow
    pub(crate) fn read_bgra(&mut self, bgra: &[u8]) {
        let width = self.width;
        assert_eq!(bgra.len(), width * self.height * 4);

        let (luma, chroma) = self.planes.split_at_mut(width * self.height);
        let (u_plane, v_plane) = chroma.split_at_mut(luma.len() / 4);
        let row_pairs = bgra
            .chunks_exact(8 * width)
            .zip(luma.chunks_exact_mut(2 * width))
            .zip(u_plane.chunks_exact_mut(width / 2))
            .zip(v_plane.chunks_exact_mut(width / 2));

        for (((pixels, luma), u_row), v_row) in row_pairs {
            let (top, bottom) = pixels.split_at(4 * width);
            for (pixel, y) in pixels.chunks_exact(4).zip(luma.iter_mut()) {
                *y = luma_of(pixel);
            }
            let quads = top.chunks_exact(8).zip(bottom.chunks_exact(8));
            for ((top, bottom), (u, v)) in quads.zip(u_row.iter_mut().zip(v_row.iter_mut())) {
                let sum = |channel: usize| {
                    i32::from(top[channel])
                        + i32::from(top[channel + 4])
                        + i32::from(bottom[channel])
                        + i32::from(bottom[channel + 4])
                };
                let (b, g, r) = (sum(0), sum(1), sum(2));
                *u = ((112 * b - 38 * r - 74 * g + (128 << 10)) >> 10) as u8;
                *v = ((112 * r - 94 * g - 18 * b + (128 << 10)) >> 10) as u8;
            }
        }
    }

    pub(crate) fn slices(&self) -> YUVSlices<'_> {
        let luma = self.width * self.height;
        let (y, chroma) = self.planes.split_at(luma);
        let (u, v) = chroma.split_at(luma / 4);
        YUVSlices::new(
            (y, u, v),
            (self.width, self.height),
            (self.width, self.width / 2, self.width / 2),
        )
    }
}

fn luma_of(bgra: &[u8]) -> u8 {
    let [b, g, r] = [bgra[0], bgra[1], bgra[2]].map(u32::from);
    ((66 * r + 129 * g + 25 * b) >> 8) as u8 + 16
}

#[cfg(test)]
mod tests {
    use openh264::formats::{BgraSliceU8, YUVBuffer, YUVSource};

    use super::I420Frame;

    fn assert_matches_openh264(width: usize, height: usize, bgra: &[u8]) {
        let expected = YUVBuffer::from_rgb_source(BgraSliceU8::new(bgra, (width, height)));
        let mut frame = I420Frame::new(width, height);
        frame.read_bgra(bgra);
        let actual = frame.slices();
        assert_eq!(actual.y(), expected.y(), "luma of {width}x{height}");
        assert_eq!(actual.u(), expected.u(), "u of {width}x{height}");
        assert_eq!(actual.v(), expected.v(), "v of {width}x{height}");
    }

    fn noise(len: usize, mut state: u64) -> Vec<u8> {
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 32) as u8
            })
            .collect()
    }

    #[test]
    fn random_frames_match_openh264_byte_for_byte() {
        for (seed, (width, height)) in [(64, 48), (322, 6), (2, 2), (256, 256)]
            .into_iter()
            .enumerate()
        {
            let bgra = noise(width * height * 4, 0x9e37_79b9_7f4a_7c15 ^ seed as u64);
            assert_matches_openh264(width, height, &bgra);
        }
    }

    #[test]
    fn extreme_colours_match_openh264() {
        let corners = [0u8, 1, 127, 128, 254, 255];
        let pixels: Vec<u8> = corners
            .iter()
            .flat_map(|&b| corners.iter().flat_map(move |&g| corners.map(|r| [b, g, r, 255])))
            .flatten()
            .collect();
        // 216 colours as a 12x18 frame, then the same colours with each 2x2 block uniform
        assert_matches_openh264(12, 18, &pixels);
        let blocks: Vec<u8> = pixels
            .chunks_exact(4)
            .flat_map(|pixel| pixel.repeat(2))
            .collect::<Vec<_>>()
            .chunks_exact(4 * 24)
            .flat_map(|row| row.repeat(2))
            .collect();
        assert_matches_openh264(24, 36, &blocks);
    }

    #[test]
    fn every_luma_input_matches_the_float_formula() {
        for rgb in 0..1u32 << 24 {
            let [r, g, b, _] = rgb.to_le_bytes().map(f32::from);
            let float =
                (0.097_656_25f32.mul_add(b, 0.257_812_5f32.mul_add(r, 0.503_906_25 * g)) + 16.0)
                    as u8;
            let [r, g, b, _] = rgb.to_le_bytes();
            assert_eq!(super::luma_of(&[b, g, r]), float, "rgb {rgb:06x}");
        }
    }
}
