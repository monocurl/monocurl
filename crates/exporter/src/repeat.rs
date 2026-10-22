//! repeated frames without the encoder: a frame identical to the previous one is
//! written as a P slice whose macroblocks are all skipped, which decodes to an exact
//! copy of its reference. the encoder never sees those frames, so the frame numbers of
//! the slices it does write are rewritten to count them. only streams whose parameter
//! sets make that exact (one reference frame, picture order from frame numbers, cavlc,
//! no weighted prediction) take this path

/// what a skipped frame and a frame number rewrite need from the sps and pps
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StreamParams {
    log2_max_frame_num: u32,
    pic_size_in_mbs: u32,
    pps_id: u32,
    deblocking_control: bool,
}

impl StreamParams {
    /// the parameters of an sps and pps (nal payloads with headers), or `None` when the
    /// stream is not one a skipped frame can join exactly
    pub(crate) fn parse(sps: &[u8], pps: &[u8]) -> Option<Self> {
        let sps = unescape(sps.get(1..)?);
        let mut r = BitReader::new(&sps);
        let profile_idc = r.bits(8)?;
        r.bits(16)?; // constraint flags, reserved bits, level_idc
        r.ue()?; // seq_parameter_set_id
        if matches!(
            profile_idc,
            100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
        ) {
            let chroma_format_idc = r.ue()?;
            if chroma_format_idc == 3 {
                r.bits(1)?;
            }
            r.ue()?; // bit_depth_luma_minus8
            r.ue()?; // bit_depth_chroma_minus8
            r.bits(1)?; // qpprime_y_zero_transform_bypass_flag
            if r.bits(1)? != 0 {
                return None; // scaling matrices: never written by openh264
            }
        }
        let log2_max_frame_num = r.ue()? + 4;
        if r.ue()? != 2 {
            return None; // picture order from frame numbers only
        }
        let max_num_ref_frames = r.ue()?;
        r.bits(1)?; // gaps_in_frame_num_value_allowed_flag
        let width_mbs = r.ue()? + 1;
        let height_map_units = r.ue()? + 1;
        let frame_mbs_only = r.bits(1)? == 1;
        if max_num_ref_frames != 1 || !frame_mbs_only {
            return None;
        }

        let pps = unescape(pps.get(1..)?);
        let mut r = BitReader::new(&pps);
        let pps_id = r.ue()?;
        r.ue()?; // seq_parameter_set_id
        let entropy_coding_mode = r.bits(1)?;
        let bottom_field_pic_order = r.bits(1)?;
        let num_slice_groups = r.ue()? + 1;
        let ref_idx_l0_active = r.ue()? + 1;
        r.ue()?; // num_ref_idx_l1_default_active_minus1
        let weighted_pred = r.bits(1)?;
        r.bits(2)?; // weighted_bipred_idc
        r.se()?; // pic_init_qp_minus26
        r.se()?; // pic_init_qs_minus26
        r.se()?; // chroma_qp_index_offset
        let deblocking_control = r.bits(1)? == 1;
        r.bits(1)?; // constrained_intra_pred_flag
        let redundant_pic_cnt = r.bits(1)?;
        if entropy_coding_mode != 0
            || bottom_field_pic_order != 0
            || num_slice_groups != 1
            || ref_idx_l0_active != 1
            || weighted_pred != 0
            || redundant_pic_cnt != 0
        {
            return None;
        }
        Some(Self {
            log2_max_frame_num,
            pic_size_in_mbs: width_mbs * height_map_units,
            pps_id,
            deblocking_control,
        })
    }

    pub(crate) fn max_frame_num(self) -> u32 {
        1 << self.log2_max_frame_num
    }

    /// a reference P slice (nal payload with its header) skipping every macroblock
    pub(crate) fn skipped_frame(self, frame_num: u32) -> Vec<u8> {
        let mut w = BitWriter::default();
        w.ue(0); // first_mb_in_slice
        w.ue(5); // slice_type: P, as every slice of the picture
        w.ue(self.pps_id);
        w.bits(frame_num, self.log2_max_frame_num);
        w.bits(0, 1); // num_ref_idx_active_override_flag
        w.bits(0, 1); // ref_pic_list_modification_flag_l0
        w.bits(0, 1); // adaptive_ref_pic_marking_mode_flag: sliding window
        w.se(0); // slice_qp_delta
        if self.deblocking_control {
            w.ue(1); // disable_deblocking_filter_idc: nothing to filter anyway
        }
        w.ue(self.pic_size_in_mbs); // mb_skip_run over the whole picture
        w.trailing_bits();
        let mut nal = vec![0x61]; // nal_ref_idc 3, non-idr slice
        nal.extend(escape(&w.bytes));
        nal
    }

    /// the frame number of a slice (nal payload with its header)
    pub(crate) fn frame_num(self, nal: &[u8]) -> Option<u32> {
        let rbsp = unescape(nal.get(1..)?);
        let mut r = BitReader::new(&rbsp);
        r.ue()?;
        r.ue()?;
        r.ue()?;
        r.bits(self.log2_max_frame_num)
    }

    /// `nal` (a slice with its header) with its frame number replaced
    pub(crate) fn with_frame_num(self, nal: &[u8], frame_num: u32) -> Option<Vec<u8>> {
        let (&header, payload) = nal.split_first()?;
        let mut rbsp = unescape(payload);
        let mut r = BitReader::new(&rbsp);
        r.ue()?;
        r.ue()?;
        r.ue()?;
        let at = r.position;
        r.bits(self.log2_max_frame_num)?;
        for i in 0..self.log2_max_frame_num as usize {
            let bit = (frame_num >> (self.log2_max_frame_num as usize - 1 - i)) & 1;
            let (byte, shift) = ((at + i) / 8, 7 - (at + i) % 8);
            rbsp[byte] = rbsp[byte] & !(1 << shift) | (bit as u8) << shift;
        }
        let mut out = vec![header];
        out.extend(escape(&rbsp));
        Some(out)
    }
}

/// frame numbers of a stream that mixes encoded and skipped frames
pub(crate) struct Repeats {
    params: Option<StreamParams>,
    /// of the last reference frame written
    frame_num: u32,
    /// whether a frame was skipped since the last idr, after which the encoder's own
    /// numbering is behind and gets rewritten
    skipped_since_idr: bool,
}

impl Repeats {
    pub(crate) fn new(params: Option<StreamParams>) -> Self {
        Self {
            params,
            frame_num: 0,
            skipped_since_idr: false,
        }
    }

    /// a length-prefixed sample repeating the last frame, or `None` when the stream
    /// cannot take one and the frame has to be encoded
    pub(crate) fn skipped(&mut self) -> Option<Vec<u8>> {
        let params = self.params?;
        self.frame_num = (self.frame_num + 1) % params.max_frame_num();
        self.skipped_since_idr = true;
        let nal = params.skipped_frame(self.frame_num);
        let mut sample = (nal.len() as u32).to_be_bytes().to_vec();
        sample.extend(nal);
        Some(sample)
    }

    /// an encoded length-prefixed sample, its slices numbered after the frames skipped
    /// before it. before any frame of a gop is skipped, the encoder's numbering is
    /// checked against the one modelled here, and a difference turns skipping off for
    /// good (nothing written so far depends on the model being wrong)
    pub(crate) fn renumber(&mut self, sample: Vec<u8>) -> Vec<u8> {
        let Some(params) = self.params else {
            return sample;
        };
        let nals = nal_units(&sample);
        let Some(&first) = nals.iter().find(|nal| matches!(nal[0] & 0x1f, 1 | 5)) else {
            return sample;
        };
        let (idr, reference) = (first[0] & 0x1f == 5, first[0] & 0x60 != 0);
        let expected = match idr {
            true => 0,
            false => (self.frame_num + 1) % params.max_frame_num(),
        };
        if !self.skipped_since_idr || idr {
            if nals
                .iter()
                .filter(|nal| matches!(nal[0] & 0x1f, 1 | 5))
                .any(|nal| params.frame_num(nal) != Some(expected))
            {
                self.params = None;
                return sample;
            }
            if idr {
                self.skipped_since_idr = false;
            }
            if reference {
                self.frame_num = expected;
            }
            return sample;
        }
        let mut out = Vec::with_capacity(sample.len());
        for nal in nals {
            let nal = match nal[0] & 0x1f {
                1 => match params.with_frame_num(nal, expected) {
                    Some(renumbered) => renumbered,
                    None => {
                        // cannot happen for a slice the encoder wrote; keep the stream sound
                        self.params = None;
                        nal.to_vec()
                    }
                },
                _ => nal.to_vec(),
            };
            out.extend((nal.len() as u32).to_be_bytes());
            out.extend(nal);
        }
        if reference {
            self.frame_num = expected;
        }
        out
    }
}

/// the nal units of a length-prefixed sample
fn nal_units(sample: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut rest = sample;
    while let Some((len, tail)) = rest.split_first_chunk::<4>() {
        let len = u32::from_be_bytes(*len) as usize;
        let Some((nal, tail)) = (len <= tail.len()).then(|| tail.split_at(len)) else {
            break;
        };
        if !nal.is_empty() {
            nals.push(nal);
        }
        rest = tail;
    }
    nals
}

/// a nal payload without its emulation prevention bytes
fn unescape(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len());
    let mut zeros = 0;
    for &byte in payload {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

/// `rbsp` with emulation prevention bytes, so no start code can appear in it
fn escape(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len() + rbsp.len() / 64);
    let mut zeros = 0;
    for &byte in rbsp {
        if zeros >= 2 && byte <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.position / 8)?;
        let bit = (byte >> (7 - self.position % 8)) & 1;
        self.position += 1;
        Some(u32::from(bit))
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        (0..count).try_fold(0, |value, _| Some(value << 1 | self.bit()?))
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1 << zeros) - 1 + self.bits(zeros)?)
    }

    fn se(&mut self) -> Option<i32> {
        let k = self.ue()? as i64;
        Some(if k % 2 == 1 { (k + 1) / 2 } else { -k / 2 } as i32)
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    fn bit(&mut self, bit: u32) {
        if self.used % 8 == 0 {
            self.bytes.push(0);
        }
        if bit != 0 {
            *self.bytes.last_mut().unwrap() |= 1 << (7 - self.used % 8);
        }
        self.used += 1;
    }

    fn bits(&mut self, value: u32, count: u32) {
        for i in (0..count).rev() {
            self.bit((value >> i) & 1);
        }
    }

    fn ue(&mut self, value: u32) {
        let coded = value + 1;
        let length = 32 - coded.leading_zeros();
        self.bits(0, length - 1);
        self.bits(coded, length);
    }

    fn se(&mut self, value: i32) {
        self.ue(if value > 0 {
            2 * value as u32 - 1
        } else {
            (-2 * value) as u32
        });
    }

    /// rbsp_stop_one_bit and the zero bits that align it
    fn trailing_bits(&mut self) {
        self.bit(1);
        while self.used % 8 != 0 {
            self.bit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use openh264::{decoder::Decoder, formats::YUVSource};
    use renderer::RenderSize;

    use super::{BitReader, BitWriter, Repeats, StreamParams, escape, unescape};
    use crate::{
        AvcParameterSets, ExportSettings, encode_video_sample, new_video_encoder, yuv::I420Frame,
    };

    const WIDTH: usize = 160;
    const HEIGHT: usize = 96;

    /// a bgra frame with a square at `step`, on a gradient
    fn frame(step: usize) -> Vec<u8> {
        let mut pixels = vec![0; WIDTH * HEIGHT * 4];
        for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let (x, y) = (i % WIDTH, i / WIDTH);
            let inside = (x + 1000 - step * 7) % 200 < 30 && (20..60).contains(&y);
            let shade = if inside { 230 } else { (x + y) as u8 / 2 };
            pixel.copy_from_slice(&[shade, (step * 40) as u8 ^ shade, 255 - shade, 255]);
        }
        pixels
    }

    fn decode(samples: &[Vec<u8>], sps: &[u8], pps: &[u8]) -> Vec<Vec<u8>> {
        let annex_b = |nal: &[u8]| [&[0, 0, 0, 1][..], nal].concat();
        let mut decoder = Decoder::new().unwrap();
        let mut planes = |yuv: &dyn YUVSource| {
            let (w, h) = yuv.dimensions();
            let (sy, su, sv) = yuv.strides();
            let mut out = Vec::new();
            for row in 0..h {
                out.extend(&yuv.y()[row * sy..row * sy + w]);
            }
            for row in 0..h / 2 {
                out.extend(&yuv.u()[row * su..row * su + w / 2]);
                out.extend(&yuv.v()[row * sv..row * sv + w / 2]);
            }
            out
        };
        let mut frames = Vec::new();
        decoder.decode(&annex_b(sps)).unwrap();
        decoder.decode(&annex_b(pps)).unwrap();
        for sample in samples {
            let stream: Vec<u8> = super::nal_units(sample)
                .into_iter()
                .flat_map(annex_b)
                .collect();
            if let Some(yuv) = decoder.decode(&stream).unwrap() {
                frames.push(planes(&yuv));
            }
        }
        for yuv in decoder.flush_remaining().unwrap() {
            frames.push(planes(&yuv));
        }
        frames
    }

    /// encodes `steps` the way the exporter does, repeating unchanged frames when
    /// `repeat` is set: the samples, the parameter sets and the repeated frames
    fn encode(steps: &[usize], repeat: bool) -> (Vec<Vec<u8>>, Vec<u8>, Vec<u8>, Vec<usize>) {
        // an idr every 10 encoded frames, so renumbering crosses gops
        let settings = ExportSettings {
            render_size: RenderSize::new(WIDTH as u32, HEIGHT as u32),
            fps: 10,
        };
        let mut encoder = new_video_encoder(settings).unwrap();
        let mut planes = I420Frame::new(WIDTH, HEIGHT);
        let mut sets = AvcParameterSets::default();
        let mut repeats: Option<Repeats> = None;
        let (mut samples, mut repeated) = (Vec::new(), Vec::new());
        for (i, &step) in steps.iter().enumerate() {
            let unchanged = repeat && i > 0 && steps[i - 1] == step;
            if let Some(bytes) = repeats
                .as_mut()
                .filter(|_| unchanged)
                .and_then(Repeats::skipped)
            {
                samples.push(bytes);
                repeated.push(i);
                continue;
            }
            planes.read_bgra(&frame(step));
            let mut encoded = encode_video_sample(&planes, &mut encoder, &mut sets).unwrap();
            if let Some(repeats) = repeats.as_mut() {
                encoded.bytes = repeats.renumber(encoded.bytes);
            }
            if repeats.is_none() && repeat {
                let params =
                    StreamParams::parse(sets.sps.as_deref().unwrap(), sets.pps.as_deref().unwrap());
                assert!(
                    params.is_some(),
                    "the exporter's encoder settings cannot repeat frames"
                );
                repeats = Some(Repeats::new(params));
            }
            samples.push(encoded.bytes);
        }
        (samples, sets.sps.unwrap(), sets.pps.unwrap(), repeated)
    }

    #[test]
    fn repeated_frames_decode_to_exact_copies_and_the_rest_stays_sound() {
        // static runs, including across idrs (every 10 encoded frames) and long ones
        let steps: Vec<usize> = [
            0, 1, 1, 1, 2, 3, 3, 4, 5, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
        ]
        .into_iter()
        .chain((8..30).flat_map(|step| [step, step, step]))
        .collect();
        let (samples, sps, pps, repeated) = encode(&steps, true);
        assert!(
            repeated.len() > 40,
            "only {} frames repeated",
            repeated.len()
        );
        let decoded = decode(&samples, &sps, &pps);
        assert_eq!(decoded.len(), steps.len(), "a frame did not decode");
        for &i in &repeated {
            assert!(
                decoded[i] == decoded[i - 1],
                "repeated frame {i} is not a copy"
            );
        }

        // against every frame encoded, the decoded pictures differ only by encoder noise
        let (plain, sps, pps, none) = encode(&steps, false);
        assert!(none.is_empty());
        let reference = decode(&plain, &sps, &pps);
        for (i, (a, b)) in decoded.iter().zip(&reference).enumerate() {
            let worst = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
            assert!(worst <= 12, "frame {i} drifted by {worst}");
        }
    }

    #[test]
    fn exp_golomb_and_escaping_round_trip() {
        let mut w = BitWriter::default();
        for value in [0, 1, 2, 7, 8, 255, 4096, 1 << 20] {
            w.ue(value);
        }
        for value in [0, 1, -1, 5, -6, 1000, -1000] {
            w.se(value);
        }
        w.trailing_bits();
        let mut r = BitReader::new(&w.bytes);
        for value in [0, 1, 2, 7, 8, 255, 4096, 1 << 20] {
            assert_eq!(r.ue(), Some(value));
        }
        for value in [0, 1, -1, 5, -6, 1000, -1000] {
            assert_eq!(r.se(), Some(value));
        }
        let awkward = [0, 0, 0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0];
        let escaped = escape(&awkward);
        assert!(
            escaped
                .windows(3)
                .all(|w| !(w[0] == 0 && w[1] == 0 && w[2] <= 2))
        );
        assert_eq!(unescape(&escaped), awkward);
    }
}
