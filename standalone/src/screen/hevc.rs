//! RTP/HEVC (RFC 7798) to whole frames for the browser's video decoder.
//!
//! The phone sends its screen as HEVC over plain RTP. This module reassembles the NAL units,
//! groups them into access units (one frame each), keeps the parameter sets (VPS/SPS/PPS) and
//! turns them into what WebCodecs needs: a codec string, an `hvcC` record (ISO/IEC 14496-15
//! §8.3.3.1) and frames as 4-byte length-prefixed NAL units.

pub const NAL_VPS: u8 = 32;
pub const NAL_SPS: u8 = 33;
pub const NAL_PPS: u8 = 34;
const NAL_AUD: u8 = 35;
const NAL_AP: u8 = 48;
const NAL_FU: u8 = 49;

/// Apple's display service ends every coded slice with this 14-byte footer. It is not part of
/// the HEVC bitstream, so it is removed before decoding (a no-op when a stream has none).
pub const APPLE_SLICE_FOOTER: [u8; 14] = [0x04, 0xf0, 0x0a, 0xc0, 0x00, 0x00, 0x03, 0x00, 0x00, 0x04, 0xec, 0x0a, 0xb0, 0x03];

pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map(|b| (b >> 1) & 0x3f).unwrap_or(0xff)
}

/// IRAP pictures (BLA, IDR, CRA) can be decoded on their own.
fn is_key(t: u8) -> bool {
    (16..=21).contains(&t)
}

fn strip_footer(mut nal: Vec<u8>) -> Vec<u8> {
    if nal_type(&nal) <= 31 && nal.ends_with(&APPLE_SLICE_FOOTER) {
        nal.truncate(nal.len() - APPLE_SLICE_FOOTER.len());
    }
    nal
}

/// One decoded-ready frame.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessUnit {
    pub rtp_ts: u32,
    pub key: bool,
    /// Coded slices and SEI, without parameter sets or access-unit delimiters.
    pub nals: Vec<Vec<u8>>,
}

impl AccessUnit {
    /// The frame as WebCodecs expects it in `hvcC` mode: each NAL with a 4-byte big-endian length.
    pub fn length_prefixed(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.nals.iter().map(|n| n.len() + 4).sum());
        for n in &self.nals {
            out.extend_from_slice(&(n.len() as u32).to_be_bytes());
            out.extend_from_slice(n);
        }
        out
    }
}

#[derive(Debug, Default)]
pub struct Depacketizer {
    expect_seq: Option<u16>,
    fu: Option<Vec<u8>>,
    cur: Vec<Vec<u8>>,
    cur_ts: Option<u32>,
    cur_key: bool,
    /// A packet was lost: frames are held back until the next keyframe.
    pub waiting_for_key: bool,
    pub vps: Option<Vec<u8>>,
    pub sps: Option<Vec<u8>>,
    pub pps: Option<Vec<u8>>,
    /// Set when a parameter set changed; the caller sends a new decoder config and clears it.
    pub config_changed: bool,
    pub lost_packets: u64,
}

impl Depacketizer {
    pub fn new() -> Self {
        Self { waiting_for_key: true, ..Default::default() }
    }

    /// Feed one RTP packet. Returns the frames it completed (usually zero or one).
    pub fn push(&mut self, seq: u16, ts: u32, marker: bool, payload: &[u8]) -> Vec<AccessUnit> {
        let mut done = Vec::new();
        if let Some(expect) = self.expect_seq {
            let gap = seq.wrapping_sub(expect);
            if gap >= 0x8000 {
                return done; // a late duplicate of a packet already handled
            }
            if gap != 0 {
                self.lost_packets += gap as u64;
                self.drop_partial();
                self.waiting_for_key = true;
            }
        }
        self.expect_seq = Some(seq.wrapping_add(1));

        if self.cur_ts.is_some_and(|t| t != ts) {
            self.finish(&mut done);
        }
        self.cur_ts = Some(ts);

        if payload.len() < 2 {
            return done;
        }
        match nal_type(payload) {
            NAL_AP => {
                let mut i = 2;
                while i + 2 <= payload.len() {
                    let n = u16::from_be_bytes([payload[i], payload[i + 1]]) as usize;
                    i += 2;
                    if n == 0 || i + n > payload.len() {
                        break;
                    }
                    self.add_nal(payload[i..i + n].to_vec());
                    i += n;
                }
            }
            NAL_FU => {
                if payload.len() < 3 {
                    return done;
                }
                let h = payload[2];
                let (start, end, t) = (h & 0x80 != 0, h & 0x40 != 0, h & 0x3f);
                if start {
                    let mut nal = vec![(payload[0] & 0x81) | (t << 1), payload[1]];
                    nal.extend_from_slice(&payload[3..]);
                    self.fu = Some(nal);
                } else if let Some(buf) = self.fu.as_mut() {
                    buf.extend_from_slice(&payload[3..]);
                }
                if end {
                    if let Some(nal) = self.fu.take() {
                        self.add_nal(strip_footer(nal));
                    }
                }
            }
            _ => self.add_nal(strip_footer(payload.to_vec())),
        }
        if marker {
            self.finish(&mut done);
        }
        done
    }

    fn add_nal(&mut self, nal: Vec<u8>) {
        let t = nal_type(&nal);
        let slot = match t {
            NAL_VPS => Some(&mut self.vps),
            NAL_SPS => Some(&mut self.sps),
            NAL_PPS => Some(&mut self.pps),
            _ => None,
        };
        if let Some(slot) = slot {
            if slot.as_deref() != Some(&nal[..]) {
                *slot = Some(nal);
                self.config_changed = true;
            }
            return;
        }
        if t == NAL_AUD {
            return;
        }
        if is_key(t) {
            self.cur_key = true;
        }
        self.cur.push(nal);
    }

    fn drop_partial(&mut self) {
        self.fu = None;
        self.cur.clear();
        self.cur_key = false;
        self.cur_ts = None;
    }

    fn finish(&mut self, done: &mut Vec<AccessUnit>) {
        let nals = std::mem::take(&mut self.cur);
        let key = std::mem::replace(&mut self.cur_key, false);
        let ts = self.cur_ts.take().unwrap_or(0);
        self.fu = None;
        if nals.is_empty() {
            return;
        }
        if self.waiting_for_key {
            if !key || self.sps.is_none() || self.pps.is_none() || self.vps.is_none() {
                return;
            }
            self.waiting_for_key = false;
        }
        done.push(AccessUnit { rtp_ts: ts, key, nals });
    }

    /// Codec string, `hvcC` record and picture size for the current parameter sets.
    pub fn decoder_config(&self) -> Option<DecoderConfig> {
        let (vps, sps, pps) = (self.vps.as_ref()?, self.sps.as_ref()?, self.pps.as_ref()?);
        let info = parse_sps(sps)?;
        Some(DecoderConfig { codec: info.codec_string(), hvcc: hvcc(&info, vps, sps, pps), width: info.width, height: info.height })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecoderConfig {
    pub codec: String,
    pub hvcc: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Remove emulation-prevention bytes (00 00 03 -> 00 00) after the 2-byte NAL header.
pub fn rbsp(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &b in nal.iter().skip(2) {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.d.get(self.pos / 8)?;
        let v = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(v as u32)
    }
    fn bits(&mut self, n: usize) -> Option<u64> {
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | self.bit()? as u64;
        }
        Some(v)
    }
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some(((1u64 << zeros) - 1 + self.bits(zeros)?) as u32)
    }
}

/// What the SPS says about the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct SpsInfo {
    /// The 12-byte general profile_tier_level (profile space/tier/idc, compatibility flags,
    /// constraint flags, level), exactly as `hvcC` stores it.
    pub ptl: [u8; 12],
    pub max_sub_layers: u8,
    pub temporal_id_nested: bool,
    pub chroma_format_idc: u32,
    pub bit_depth_luma: u32,
    pub bit_depth_chroma: u32,
    pub width: u32,
    pub height: u32,
}

pub fn parse_sps(sps: &[u8]) -> Option<SpsInfo> {
    if nal_type(sps) != NAL_SPS {
        return None;
    }
    let d = rbsp(sps);
    let mut b = Bits { d: &d, pos: 0 };
    b.bits(4)?; // sps_video_parameter_set_id
    let max_sub_layers_minus1 = b.bits(3)? as usize;
    let nested = b.bit()? == 1;
    let start = b.pos / 8;
    let ptl: [u8; 12] = d.get(start..start + 12)?.try_into().ok()?;
    b.pos += 96;
    let mut profile_present = [false; 8];
    let mut level_present = [false; 8];
    for i in 0..max_sub_layers_minus1 {
        profile_present[i] = b.bit()? == 1;
        level_present[i] = b.bit()? == 1;
    }
    if max_sub_layers_minus1 > 0 {
        for _ in max_sub_layers_minus1..8 {
            b.bits(2)?;
        }
    }
    for i in 0..max_sub_layers_minus1 {
        if profile_present[i] {
            b.pos += 88;
        }
        if level_present[i] {
            b.pos += 8;
        }
    }
    b.ue()?; // sps_seq_parameter_set_id
    let chroma = b.ue()?;
    if chroma == 3 {
        b.bit()?; // separate_colour_plane_flag
    }
    let mut width = b.ue()?;
    let mut height = b.ue()?;
    if b.bit()? == 1 {
        // Conformance window, in chroma units.
        let (l, r, t, bo) = (b.ue()?, b.ue()?, b.ue()?, b.ue()?);
        let (sx, sy) = match chroma {
            1 => (2, 2),
            2 => (2, 1),
            _ => (1, 1),
        };
        width = width.saturating_sub(sx * (l + r));
        height = height.saturating_sub(sy * (t + bo));
    }
    let bit_depth_luma = b.ue()? + 8;
    let bit_depth_chroma = b.ue()? + 8;
    Some(SpsInfo {
        ptl,
        max_sub_layers: max_sub_layers_minus1 as u8 + 1,
        temporal_id_nested: nested,
        chroma_format_idc: chroma,
        bit_depth_luma,
        bit_depth_chroma,
        width,
        height,
    })
}

impl SpsInfo {
    /// The WebCodecs / RFC 6381 codec string (ISO/IEC 14496-15 Annex E), e.g. `hev1.1.6.L120.90`.
    pub fn codec_string(&self) -> String {
        let p = &self.ptl;
        let space = p[0] >> 6;
        let tier = if p[0] & 0x20 != 0 { 'H' } else { 'L' };
        let idc = p[0] & 0x1f;
        let compat = u32::from_be_bytes([p[1], p[2], p[3], p[4]]).reverse_bits();
        let mut s = format!("hev1.{}{}.{:X}.{}{}", ["", "A", "B", "C"][space as usize], idc, compat, tier, p[11]);
        let constraints = &p[5..11];
        let used = constraints.iter().rposition(|&c| c != 0).map(|i| i + 1).unwrap_or(0);
        for c in &constraints[..used] {
            s.push_str(&format!(".{c:X}"));
        }
        s
    }
}

/// HEVCDecoderConfigurationRecord (`hvcC`), the WebCodecs `description` for HEVC.
pub fn hvcc(info: &SpsInfo, vps: &[u8], sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut r = Vec::with_capacity(64 + vps.len() + sps.len() + pps.len());
    r.push(1); // configurationVersion
    r.extend_from_slice(&info.ptl);
    r.extend_from_slice(&[0xf0, 0x00]); // reserved + min_spatial_segmentation_idc = 0
    r.push(0xfc); // reserved + parallelismType = 0 (unknown)
    r.push(0xfc | (info.chroma_format_idc as u8 & 3));
    r.push(0xf8 | ((info.bit_depth_luma.saturating_sub(8)) as u8 & 7));
    r.push(0xf8 | ((info.bit_depth_chroma.saturating_sub(8)) as u8 & 7));
    r.extend_from_slice(&[0, 0]); // avgFrameRate unknown
    // constantFrameRate 0 | numTemporalLayers | temporalIdNested | lengthSizeMinusOne = 3
    r.push(((info.max_sub_layers & 7) << 3) | ((info.temporal_id_nested as u8) << 2) | 3);
    r.push(3);
    for (t, nal) in [(NAL_VPS, vps), (NAL_SPS, sps), (NAL_PPS, pps)] {
        r.push(0x80 | t); // array_completeness = 1: parameter sets only come out of band
        r.extend_from_slice(&1u16.to_be_bytes());
        r.extend_from_slice(&(nal.len() as u16).to_be_bytes());
        r.extend_from_slice(nal);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real 1170x2532 Main-profile SPS (level 6.0), built to the spec: see `sample_sps`.
    fn sample_sps() -> Vec<u8> {
        // Write an SPS with known values using a tiny bit writer.
        let mut w = Writer::default();
        w.put(0, 4); // vps id
        w.put(0, 3); // max_sub_layers_minus1
        w.put(1, 1); // temporal_id_nesting
        w.put(0, 2); // profile space
        w.put(0, 1); // tier
        w.put(1, 5); // profile idc = Main
        w.put(0x6000_0000, 32); // compatibility flags: Main and Main10
        w.put(0x90, 8); // progressive + frame_only constraint bits
        w.put(0, 40);
        w.put(180, 8); // level 6.0
        w.ue(0); // sps id
        w.ue(1); // chroma 4:2:0
        w.ue(1184); // coded width
        w.ue(2544); // coded height
        w.put(1, 1); // conformance window
        w.ue(0);
        w.ue(7); // right: 14 luma px
        w.ue(0);
        w.ue(6); // bottom: 12 luma px
        w.ue(0); // bit depth luma 8
        w.ue(0); // bit depth chroma 8
        w.put(1, 1); // stop bit
        let mut nal = vec![NAL_SPS << 1, 1];
        // Emulation prevention, as an encoder would add it.
        let mut zeros = 0;
        for b in w.finish() {
            if zeros >= 2 && b <= 3 {
                nal.push(3);
                zeros = 0;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            nal.push(b);
        }
        nal
    }

    #[derive(Default)]
    struct Writer {
        bits: Vec<u8>,
    }
    impl Writer {
        fn put(&mut self, v: u64, n: usize) {
            for i in (0..n).rev() {
                self.bits.push(((v >> i) & 1) as u8);
            }
        }
        fn ue(&mut self, v: u32) {
            let x = v as u64 + 1;
            let len = 64 - x.leading_zeros() as usize;
            self.put(0, len - 1);
            self.put(x, len);
        }
        fn finish(mut self) -> Vec<u8> {
            while self.bits.len() % 8 != 0 {
                self.bits.push(0);
            }
            self.bits.chunks(8).map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b)).collect()
        }
    }

    #[test]
    fn parses_sps_size_and_codec() {
        let info = parse_sps(&sample_sps()).unwrap();
        assert_eq!((info.width, info.height), (1170, 2532));
        assert_eq!(info.chroma_format_idc, 1);
        assert_eq!((info.bit_depth_luma, info.bit_depth_chroma), (8, 8));
        assert_eq!(info.codec_string(), "hev1.1.6.L180.90");
    }

    #[test]
    fn hvcc_record_layout() {
        let sps = sample_sps();
        let info = parse_sps(&sps).unwrap();
        let vps = vec![NAL_VPS << 1, 1, 0xaa];
        let pps = vec![NAL_PPS << 1, 1, 0xbb];
        let r = hvcc(&info, &vps, &sps, &pps);
        assert_eq!(r[0], 1);
        assert_eq!(&r[1..13], &info.ptl);
        assert_eq!(r[16], 0xfd); // 4:2:0
        assert_eq!(r[21] & 3, 3); // 4-byte lengths
        assert_eq!(r[22], 3);
        assert_eq!(r[23], 0x80 | NAL_VPS);
        assert_eq!(&r[26..28], &3u16.to_be_bytes());
        assert!(r.ends_with(&pps));
    }

    fn slice(t: u8, body: &[u8]) -> Vec<u8> {
        let mut v = vec![t << 1, 1];
        v.extend_from_slice(body);
        v
    }

    fn params(d: &mut Depacketizer, seq: &mut u16) {
        for nal in [slice(NAL_VPS, &[1]), sample_sps(), slice(NAL_PPS, &[2])] {
            d.push(*seq, 1000, false, &nal);
            *seq += 1;
        }
    }

    #[test]
    fn single_nal_frames_and_footer() {
        let mut d = Depacketizer::new();
        let mut seq = 10;
        params(&mut d, &mut seq);
        let mut idr = slice(19, &[9, 9, 9]);
        idr.extend_from_slice(&APPLE_SLICE_FOOTER);
        let out = d.push(seq, 1000, true, &idr);
        assert_eq!(out.len(), 1);
        assert!(out[0].key);
        assert_eq!(out[0].nals, vec![slice(19, &[9, 9, 9])]);
        assert!(d.config_changed);
        let cfg = d.decoder_config().unwrap();
        assert_eq!((cfg.width, cfg.height), (1170, 2532));
        assert_eq!(out[0].length_prefixed(), [&[0, 0, 0, 5][..], &slice(19, &[9, 9, 9])].concat());
    }

    #[test]
    fn fragmented_and_aggregated() {
        let mut d = Depacketizer::new();
        let mut seq = 0;
        params(&mut d, &mut seq);
        // An aggregation packet carrying an SEI and a short IDR slice.
        let sei = slice(39, &[5]);
        let idr = slice(19, &[1, 2]);
        let mut ap = vec![NAL_AP << 1, 1];
        for n in [&sei, &idr] {
            ap.extend_from_slice(&(n.len() as u16).to_be_bytes());
            ap.extend_from_slice(n);
        }
        let out = d.push(seq, 2000, true, &ap);
        seq += 1;
        assert_eq!(out[0].nals, vec![sei.clone(), idr.clone()]);
        // A TRAIL_R slice split into three fragments.
        let body: Vec<u8> = (0..30).collect();
        let fu = |start: bool, end: bool, part: &[u8]| {
            let mut v = vec![NAL_FU << 1, 1, (if start { 0x80 } else { 0 }) | (if end { 0x40 } else { 0 }) | 1];
            v.extend_from_slice(part);
            v
        };
        assert!(d.push(seq, 3000, false, &fu(true, false, &body[..10])).is_empty());
        assert!(d.push(seq + 1, 3000, false, &fu(false, false, &body[10..20])).is_empty());
        let out = d.push(seq + 2, 3000, true, &fu(false, true, &body[20..]));
        assert_eq!(out.len(), 1);
        assert!(!out[0].key);
        assert_eq!(out[0].nals, vec![slice(1, &body)]);
    }

    #[test]
    fn loss_holds_frames_until_keyframe() {
        let mut d = Depacketizer::new();
        let mut seq = 0;
        params(&mut d, &mut seq);
        assert_eq!(d.push(seq, 1, true, &slice(19, &[1])).len(), 1);
        // seq + 1 is lost.
        assert!(d.push(seq + 2, 2, true, &slice(1, &[1])).is_empty());
        assert!(d.waiting_for_key);
        assert_eq!(d.lost_packets, 1);
        assert!(d.push(seq + 3, 3, true, &slice(1, &[1])).is_empty());
        let out = d.push(seq + 4, 4, true, &slice(20, &[1]));
        assert!(out[0].key && !d.waiting_for_key);
        // A late duplicate is ignored.
        assert!(d.push(seq + 1, 2, true, &slice(1, &[1])).is_empty());
    }

    #[test]
    fn timestamp_change_closes_a_frame_without_marker() {
        let mut d = Depacketizer::new();
        let mut seq = 0;
        params(&mut d, &mut seq);
        assert!(d.push(seq, 10, false, &slice(19, &[1])).is_empty());
        let out = d.push(seq + 1, 11, false, &slice(1, &[2]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rtp_ts, 10);
    }
}
