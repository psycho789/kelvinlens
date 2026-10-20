//! A minimal Motion-JPEG AVI writer for exporting rendered video, the format FLIR Thermal Studio
//! exports too. Plays in VLC, Windows Media Player and browsers' video tools; frames are JPEGs.

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 9);
    out.extend_from_slice(id);
    out.extend_from_slice(&le32(data.len() as u32));
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut data = kind.to_vec();
    data.extend_from_slice(body);
    chunk(b"LIST", &data)
}

/// The AVI container around already-encoded JPEG frames.
pub fn container(jpegs: &[Vec<u8>], width: usize, height: usize, fps: f64) -> Vec<u8> {
    let fps = if fps.is_finite() && fps > 0.0 { fps } else { 30.0 };
    let (scale, rate) = (1000u32, (fps * 1000.0).round().max(1.0) as u32);
    let largest = jpegs.iter().map(Vec::len).max().unwrap_or(0) as u32;
    let n = jpegs.len() as u32;
    let (w, h) = (width as u32, height as u32);

    let mut avih = Vec::new();
    for v in [
        (1_000_000.0 / fps).round() as u32, // microseconds per frame
        (largest as f64 * fps) as u32,      // max bytes per second
        0,                                  // padding granularity
        0x10,                               // AVIF_HASINDEX
        n,
        0,
        1, // streams
        largest,
        w,
        h,
        0,
        0,
        0,
        0,
    ] {
        avih.extend_from_slice(&le32(v));
    }

    let mut strh = Vec::new();
    strh.extend_from_slice(b"vidsMJPG");
    for v in [0u32, 0, 0, scale, rate, 0, n, largest, u32::MAX, 0] {
        strh.extend_from_slice(&le32(v));
    }
    for v in [0u16, 0, u16::try_from(w).unwrap_or(u16::MAX), u16::try_from(h).unwrap_or(u16::MAX)] {
        strh.extend_from_slice(&v.to_le_bytes());
    }

    let mut strf = Vec::new();
    strf.extend_from_slice(&le32(40));
    strf.extend_from_slice(&le32(w));
    strf.extend_from_slice(&le32(h));
    strf.extend_from_slice(&1u16.to_le_bytes());
    strf.extend_from_slice(&24u16.to_le_bytes());
    strf.extend_from_slice(b"MJPG");
    for v in [w.saturating_mul(h).saturating_mul(3), 0, 0, 0, 0] {
        strf.extend_from_slice(&le32(v));
    }

    let strl = list(b"strl", &[chunk(b"strh", &strh), chunk(b"strf", &strf)].concat());
    let hdrl = list(b"hdrl", &[chunk(b"avih", &avih), strl].concat());

    let mut movi_body = Vec::new();
    let mut index = Vec::new();
    for j in jpegs {
        // Offsets in idx1 count from the "movi" four-character code.
        let offset = 4 + movi_body.len() as u32;
        index.extend_from_slice(b"00dc");
        index.extend_from_slice(&le32(0x10)); // keyframe
        index.extend_from_slice(&le32(offset));
        index.extend_from_slice(&le32(j.len() as u32));
        movi_body.extend(chunk(b"00dc", j));
    }
    let movi = list(b"movi", &movi_body);
    let idx1 = chunk(b"idx1", &index);

    let mut body = b"AVI ".to_vec();
    body.extend(hdrl);
    body.extend(movi);
    body.extend(idx1);
    chunk(b"RIFF", &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export;

    #[test]
    fn structure_and_index_point_at_frames() {
        let frames = [vec![10u8; 8 * 6 * 3], vec![200u8; 8 * 6 * 3], vec![90u8; 8 * 6 * 3]];
        let jpegs: Vec<Vec<u8>> = frames.iter().map(|f| export::jpeg(f, 8, 6, 85).unwrap()).collect();
        let avi = container(&jpegs, 8, 6, 9.0);
        assert_eq!(&avi[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(avi[4..8].try_into().unwrap()) as usize, avi.len() - 8);
        assert_eq!(&avi[8..12], b"AVI ");
        let movi = avi.windows(4).position(|w| w == b"movi").unwrap();
        let idx = avi.windows(4).rposition(|w| w == b"idx1").unwrap();
        let entries = u32::from_le_bytes(avi[idx + 4..idx + 8].try_into().unwrap()) as usize / 16;
        assert_eq!(entries, 3);
        for e in 0..entries {
            let at = idx + 8 + e * 16;
            let off = u32::from_le_bytes(avi[at + 8..at + 12].try_into().unwrap()) as usize;
            let size = u32::from_le_bytes(avi[at + 12..at + 16].try_into().unwrap()) as usize;
            let frame = movi + off;
            assert_eq!(&avi[frame..frame + 4], b"00dc");
            let data = &avi[frame + 8..frame + 8 + size];
            let img = image::load_from_memory(data).unwrap();
            assert_eq!((img.width(), img.height()), (8, 6));
        }
        // Frame rate as rate/scale in the stream header.
        let strh = avi.windows(4).position(|w| w == b"strh").unwrap() + 8;
        let scale = u32::from_le_bytes(avi[strh + 20..strh + 24].try_into().unwrap());
        let rate = u32::from_le_bytes(avi[strh + 24..strh + 28].try_into().unwrap());
        assert_eq!(rate / scale, 9);
    }

    #[test]
    fn huge_dimensions_do_not_panic() {
        assert!(container(&[], 70_000, 70_000, 0.0).starts_with(b"RIFF"));
        assert!(container(&[vec![0xFF, 0xD8, 0xFF, 0xD9]], 1_500_000, 1_500_000, f64::NAN).starts_with(b"RIFF"));
    }
}
