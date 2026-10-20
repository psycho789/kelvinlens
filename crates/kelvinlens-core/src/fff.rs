//! Low-level reading and patching of FLIR's FFF container, which holds the raw sensor data and
//! camera settings. It is stored on its own (SEQ/CSQ video frames, .fff files) or split across
//! `APP1 FLIR` segments of a radiometric JPEG.
//!
//! Layout (as documented by ExifTool's FLIR module): a 64-byte header starting `FFF\0`, then a
//! directory of 32-byte entries (type, subtype, version, index, offset, length, parent, object,
//! checksum). The header's byte order is given by its swap-pattern field; the camera-info record
//! (type 32) is little-endian in every file seen, which is also what flyr assumes.

use crate::bytes::{f32_le, text, u16_at, u32_at};
use crate::jpeg;
use crate::{Error, Params, Result};

pub const RAW_DATA: u16 = 1;
pub const CAMERA_INFO: u16 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: u16,
    /// Offset of the record from the start of the FFF block.
    pub offset: usize,
    pub length: usize,
    /// Offset of this directory entry from the start of the FFF block.
    pub entry_at: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    pub little_endian: bool,
    pub entries: Vec<Entry>,
}

impl Directory {
    pub fn get(&self, kind: u16) -> Option<&Entry> {
        self.entries.iter().find(|e| e.kind == kind)
    }

    /// Bytes from the start of the block to the end of its last record.
    pub fn extent(&self) -> usize {
        self.entries.iter().map(|e| e.offset + e.length).max().unwrap_or(64)
    }
}

/// Parses the header and directory of the FFF block at the start of `b`.
pub fn directory(b: &[u8]) -> Option<Directory> {
    if !(b.starts_with(b"FFF\0") || b.starts_with(b"AFF\0")) || b.len() < 64 {
        return None;
    }
    let le = b.get(0x24..0x26)? != [0, 0];
    let dir_at = u32_at(b, 24, le)? as usize;
    let count = u32_at(b, 28, le)? as usize;
    if count == 0 || count > 4096 {
        return None;
    }
    let mut entries = Vec::new();
    for i in 0..count {
        let e = dir_at + 32 * i;
        if e + 32 > b.len() {
            return None;
        }
        let kind = u16_at(b, e, le)?;
        if kind == 0 {
            continue;
        }
        let offset = u32_at(b, e + 12, le)? as usize;
        let length = u32_at(b, e + 16, le)? as usize;
        entries.push(Entry { kind, offset, length, entry_at: e });
    }
    Some(Directory { little_endian: le, entries })
}

/// For each `APP1 FLIR` segment: the file offset of its data and the data's length.
pub type Segments = Vec<(usize, usize)>;

/// The FFF data of a FLIR JPEG (its `APP1 FLIR` segments joined in order), and for each segment
/// the file offset of its data and the data's length, so FFF offsets can be mapped back to the file.
pub fn from_jpeg(bytes: &[u8]) -> Option<(Vec<u8>, Segments)> {
    // Segments up to the image data; a damaged tail is ignored, as the FLIR segments come first.
    let scan = jpeg::segments(bytes)?;
    // (sequence number, file offset of the data, data length) per `APP1 FLIR` segment.
    let mut chunks: Vec<(u8, usize, usize)> = scan
        .segments
        .iter()
        .filter(|s| s.marker == 0xE1 && s.data.starts_with(b"FLIR\0") && s.data.len() >= 8)
        .map(|s| (s.data[6], s.offset + 8, s.data.len() - 8))
        .collect();
    if chunks.is_empty() {
        return None;
    }
    chunks.sort_by_key(|c| c.0);
    let fff: Vec<u8> = chunks.iter().flat_map(|&(_, at, len)| bytes[at..at + len].iter().copied()).collect();
    Some((fff, chunks.into_iter().map(|(_, at, len)| (at, len)).collect()))
}

/// Camera details stored in the camera-info record (offsets from ExifTool's FLIR::CameraInfo).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraExtras {
    pub model: Option<String>,
    pub serial: Option<String>,
    pub lens: Option<String>,
    pub field_of_view: Option<f32>,
    /// Seconds since 1970 (UTC) with milliseconds, and the time zone offset in minutes.
    pub time: Option<(f64, i16)>,
    pub frame_rate: Option<u16>,
}

/// Text from the camera-info record; strings with control characters are garbage, not text.
fn record_text(rec: &[u8], at: usize, len: usize) -> Option<String> {
    text(rec, at, len).filter(|t| t.chars().all(|c| !c.is_control()))
}

pub fn camera_extras(fff: &[u8]) -> Option<CameraExtras> {
    let dir = directory(fff)?;
    let e = dir.get(CAMERA_INFO)?;
    let rec = fff.get(e.offset..e.offset + e.length)?;
    let time = (|| {
        let secs = u32_at(rec, 0x384, true)?;
        let ms = u32_at(rec, 0x388, true)? & 0xffff;
        let tz = u16_at(rec, 0x38c, true)? as i16;
        // 1990–2100: anything else is not a timestamp.
        (631_152_000..4_102_444_800).contains(&secs).then_some((secs as f64 + ms as f64 / 1000.0, tz))
    })();
    Some(CameraExtras {
        model: record_text(rec, 0xd4, 32),
        serial: record_text(rec, 0x104, 16),
        lens: record_text(rec, 0x170, 32),
        field_of_view: f32_le(rec, 0x1b4).filter(|v| *v > 0.0 && *v < 360.0),
        time,
        frame_rate: u16_at(rec, 0x464, true).filter(|&r| r > 0 && r < 10_000),
    })
}

/// Offsets in the camera-info record of the parameters Kelvinlens edits (all little-endian f32).
const PARAM_OFFSETS: [(crate::Param, usize); 7] = [
    (crate::Param::Emissivity, 0x20),
    (crate::Param::DistanceM, 0x24),
    (crate::Param::ReflectedK, 0x28),
    (crate::Param::AtmosphereK, 0x2c),
    (crate::Param::WindowK, 0x30),
    (crate::Param::WindowTransmission, 0x34),
    (crate::Param::Humidity, 0x3c),
];

/// A copy of a FLIR radiometric JPEG with the capture parameters rewritten in its camera-info
/// record. Everything else, including the raw data, is byte-for-byte unchanged, so FLIR software
/// and Kelvinlens compute the new temperatures from it. Fields left `None` keep the file's value.
pub fn rewrite_jpeg_params(jpeg: &[u8], params: &Params) -> Result<Vec<u8>> {
    let bad = |m: &str| Error::Encode(format!("cannot write parameters: {m}"));
    let (fff, segments) = from_jpeg(jpeg).ok_or_else(|| bad("not a FLIR radiometric JPEG"))?;
    let dir = directory(&fff).ok_or_else(|| bad("no FLIR record directory"))?;
    let info = dir.get(CAMERA_INFO).ok_or_else(|| bad("no camera-info record"))?;
    if info.offset + 0x40 > fff.len() || info.length < 0x40 {
        return Err(bad("camera-info record too short"));
    }
    // FFF offset → file offset through the segments' data ranges.
    let to_file = |mut at: usize| {
        for &(start, len) in &segments {
            if at < len {
                return Some(start + at);
            }
            at -= len;
        }
        None
    };
    let mut out = jpeg.to_vec();
    for (param, off) in PARAM_OFFSETS {
        let Some(v) = params.get(param) else { continue };
        for (k, byte) in v.to_le_bytes().into_iter().enumerate() {
            let at = to_file(info.offset + off + k).ok_or_else(|| bad("record outside the file"))?;
            out[at] = byte;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal big-endian FFF block with a camera-info record of 0x480 zero bytes.
    fn fff_block() -> Vec<u8> {
        let mut b = vec![0u8; 64 + 32 + 0x480];
        b[..4].copy_from_slice(b"FFF\0");
        b[24..28].copy_from_slice(&64u32.to_be_bytes());
        b[28..32].copy_from_slice(&1u32.to_be_bytes());
        b[64..66].copy_from_slice(&CAMERA_INFO.to_be_bytes());
        b[64 + 12..64 + 16].copy_from_slice(&96u32.to_be_bytes());
        b[64 + 16..64 + 20].copy_from_slice(&0x480u32.to_be_bytes());
        b[96 + 0xd4..96 + 0xd4 + 5].copy_from_slice(b"T640\0");
        b[96 + 0x384..96 + 0x388].copy_from_slice(&1_500_000_000u32.to_le_bytes());
        b[96 + 0x388..96 + 0x38c].copy_from_slice(&250u32.to_le_bytes());
        b[96 + 0x38c..96 + 0x38e].copy_from_slice(&(-120i16).to_le_bytes());
        b[96 + 0x464..96 + 0x466].copy_from_slice(&30u16.to_le_bytes());
        b
    }

    /// Wraps an FFF block into a JPEG, split over APP1 segments of `chunk` bytes.
    fn jpeg_with(fff: &[u8], chunk: usize) -> Vec<u8> {
        let mut j = vec![0xFF, 0xD8];
        let parts: Vec<&[u8]> = fff.chunks(chunk).collect();
        for (i, part) in parts.iter().enumerate() {
            j.extend_from_slice(&[0xFF, 0xE1]);
            j.extend_from_slice(&((part.len() + 10) as u16).to_be_bytes());
            j.extend_from_slice(b"FLIR\0\x01");
            j.push(i as u8);
            j.push((parts.len() - 1) as u8);
            j.extend_from_slice(part);
        }
        j.extend_from_slice(&[0xFF, 0xD9]);
        j
    }

    #[test]
    fn parses_directory_and_extras() {
        let b = fff_block();
        let d = directory(&b).unwrap();
        assert!(!d.little_endian);
        assert_eq!(d.entries, vec![Entry { kind: CAMERA_INFO, offset: 96, length: 0x480, entry_at: 64 }]);
        assert_eq!(d.extent(), 96 + 0x480);
        let x = camera_extras(&b).unwrap();
        assert_eq!(x.model.as_deref(), Some("T640"));
        assert_eq!(x.time, Some((1_500_000_000.25, -120)));
        assert_eq!(x.frame_rate, Some(30));
        assert_eq!(x.serial, None);
        assert!(directory(b"not fff").is_none());
    }

    #[test]
    fn joins_jpeg_segments_and_patches_across_them() {
        let b = fff_block();
        // 100-byte segments put the parameters (FFF offset 128..160) across a boundary.
        let jpeg = jpeg_with(&b, 100);
        let (fff, segments) = from_jpeg(&jpeg).unwrap();
        assert_eq!(fff, b);
        assert_eq!(segments.len(), b.len().div_ceil(100));

        let p = Params { emissivity: Some(0.8), humidity: Some(0.35), ..Default::default() };
        let patched = rewrite_jpeg_params(&jpeg, &p).unwrap();
        assert_eq!(patched.len(), jpeg.len());
        let (fff2, _) = from_jpeg(&patched).unwrap();
        let f = |at: usize| f32::from_le_bytes(fff2[96 + at..96 + at + 4].try_into().unwrap());
        assert_eq!((f(0x20), f(0x3c), f(0x24)), (0.8, 0.35, 0.0));
        // Nothing else changed.
        let diff = fff2.iter().zip(&b).filter(|(a, b)| a != b).count();
        assert!(diff <= 8, "{diff}");
    }

    #[test]
    fn rejects_non_flir() {
        assert!(rewrite_jpeg_params(&[0xFF, 0xD8, 0xFF, 0xD9], &Params::default()).is_err());
    }
}
