//! FLIR radiometric video: SEQ files (FFF frames with raw counts) and CSQ files (FFF frames whose
//! raw data is JPEG-LS compressed). Frames are indexed when the file is opened and decoded on
//! demand, so long recordings don't have to fit in memory.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fff;
use crate::measure::{self, Measurement, Stats};
use crate::{Error, Params, Result, ThermalImage};

/// Where one frame's FFF block is in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameRef {
    offset: u64,
    length: usize,
}

#[derive(Clone, Debug)]
pub struct Video {
    path: PathBuf,
    frames: Vec<FrameRef>,
    /// Capture time of each frame in seconds since 1970, where recorded.
    times: Vec<Option<f64>>,
    width: usize,
    height: usize,
    compressed: bool,
    frame_rate: Option<u16>,
}

/// What the UI needs to show a video.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub frames: usize,
    pub width: usize,
    pub height: usize,
    pub compressed: bool,
    /// Seconds from the first frame, per frame (estimated from the frame rate where a frame
    /// has no timestamp).
    pub times: Vec<f64>,
    pub frame_rate: Option<f64>,
}

/// Extensions of FLIR radiometric video files.
pub const EXTENSIONS: [&str; 2] = ["seq", "csq"];

pub fn is_video(path: &Path) -> bool {
    crate::folder::has_extension(path, &EXTENSIONS)
}

impl Video {
    /// Indexes every frame. Reads the file once, block by block.
    pub fn open(path: &Path) -> Result<Video> {
        let mut file = File::open(path).map_err(|e| Error::Open(e.to_string()))?;
        let size = file.metadata().map_err(|e| Error::Open(e.to_string()))?.len();
        let mut frames = Vec::new();
        let mut times = Vec::new();
        let (mut width, mut height, mut compressed, mut frame_rate) = (0, 0, false, None);
        let mut at = 0u64;
        // Headers and directories sit well inside the first 64 KiB of a frame.
        let mut head = vec![0u8; 65_536];
        while at + 64 <= size {
            let n = read_at(&mut file, at, &mut head)?;
            let Some(dir) = fff::directory(&head[..n]) else {
                // Not a frame start: look for the next one nearby (some writers pad frames).
                match find_next(&head[..n], 1) {
                    Some(skip) => {
                        at += skip as u64;
                        continue;
                    }
                    None => break,
                }
            };
            let length = dir.extent();
            if length > crate::limits::MAX_FRAME_BYTES {
                return Err(Error::Open("video frame is implausibly large".into()));
            }
            if length < 64 {
                break;
            }
            let raw = dir.get(fff::RAW_DATA).ok_or_else(|| Error::Open("video frame without raw data".into()))?;
            if frames.is_empty() {
                // Raw record: 2 bytes, then width and height (little-endian), then 26 more bytes.
                let mut rh = [0u8; 34];
                read_at(&mut file, at + raw.offset as u64, &mut rh)?;
                width = u16::from_le_bytes([rh[2], rh[3]]) as usize;
                height = u16::from_le_bytes([rh[4], rh[5]]) as usize;
                if crate::limits::pixel_count(width, height).is_none() {
                    return Err(Error::Open("video frame size is implausible".into()));
                }
                compressed = rh[32..34] == [0xFF, 0xD8];
                frame_rate = fff::camera_extras(&head[..n]).and_then(|x| x.frame_rate);
            }
            let extras = fff::camera_extras(&head[..n]);
            frames.push(FrameRef { offset: at, length: length.min((size - at) as usize) });
            times.push(extras.and_then(|x| x.time).map(|(s, _)| s));
            at += length as u64;
        }
        if frames.is_empty() || width == 0 || height == 0 {
            return Err(Error::Open("no FLIR video frames found".into()));
        }
        Ok(Video { path: path.to_path_buf(), frames, times, width, height, compressed, frame_rate })
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn info(&self) -> VideoInfo {
        let fps = self.estimated_fps();
        let first = self.times.first().copied().flatten();
        let times = self
            .times
            .iter()
            .enumerate()
            .map(|(i, t)| match (t, first) {
                (Some(t), Some(f)) => t - f,
                _ => i as f64 / fps.unwrap_or(1.0),
            })
            .collect();
        VideoInfo {
            frames: self.frames.len(),
            width: self.width,
            height: self.height,
            compressed: self.compressed,
            times,
            frame_rate: fps,
        }
    }

    /// Frames per second: from the timestamps where they are spread out, else the recorded rate.
    fn estimated_fps(&self) -> Option<f64> {
        let stamped: Vec<f64> = self.times.iter().flatten().copied().collect();
        if stamped.len() >= 2 {
            let span = stamped[stamped.len() - 1] - stamped[0];
            if span > 0.0 {
                return Some((stamped.len() - 1) as f64 / span);
            }
        }
        self.frame_rate.map(f64::from)
    }

    /// The raw bytes of frame `i`, exactly as stored.
    fn frame_bytes(&self, i: usize) -> Result<Vec<u8>> {
        let f = self.frames.get(i).ok_or_else(|| Error::Open(format!("there is no frame {}", i + 1)))?;
        let mut file = File::open(&self.path)?;
        let mut buf = vec![0u8; f.length];
        let n = read_at(&mut file, f.offset, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Frame `i` as an image, with `params` applied when they differ from the frame's own.
    pub fn frame(&self, i: usize, params: Option<&Params>) -> Result<ThermalImage> {
        let block = decompressed(self.frame_bytes(i)?)?;
        let mut image = ThermalImage::from_fff_frame(&self.path, &block)?;
        if let Some(p) = params {
            image.set_params(p)?;
        }
        Ok(image)
    }

    /// Statistics of each measurement in every frame from `first` to `last` (inclusive), with
    /// `params` applied. One row per frame.
    pub fn series(
        &self,
        first: usize,
        last: usize,
        params: Option<&Params>,
        measurements: &[Measurement],
    ) -> Result<Vec<Vec<Option<Stats>>>> {
        let last = last.min(self.len().saturating_sub(1));
        (first..=last)
            .map(|i| {
                let img = self.frame(i, params)?;
                Ok(measurements
                    .iter()
                    .map(|m| measure::stats(img.kelvin(), img.width(), img.height(), &m.shape))
                    .collect())
            })
            .collect()
    }

    /// Frames `first..=last` as a new SEQ file: the original frame blocks, unchanged (CSQ frames
    /// are written decompressed so any SEQ reader can open them).
    pub fn write_segment(&self, first: usize, last: usize, out: &Path) -> Result<usize> {
        let last = last.min(self.len().saturating_sub(1));
        if first > last {
            return Err(Error::Encode("the segment is empty".into()));
        }
        let mut bytes = Vec::new();
        for i in first..=last {
            bytes.extend(decompressed(self.frame_bytes(i)?)?);
        }
        std::fs::write(out, bytes)?;
        Ok(last - first + 1)
    }
}

/// `current` minus the mean of `previous` (kelvin, same size), optionally 2×2 binned.
pub fn difference_from_mean(
    current: &[f32],
    previous: &[&[f32]],
    width: usize,
    height: usize,
    binning: bool,
) -> Vec<f32> {
    let mut diff: Vec<f32> = if previous.is_empty() {
        vec![0.0; width * height]
    } else {
        let mut mean = vec![0f64; width * height];
        for frame in previous {
            for (m, k) in mean.iter_mut().zip(*frame) {
                *m += *k as f64;
            }
        }
        let n = previous.len() as f64;
        current.iter().zip(&mean).map(|(k, m)| (*k as f64 - m / n) as f32).collect()
    };
    if binning {
        diff = bin2x2(&diff, width, height);
    }
    diff
}

/// 2×2 pixel binning that keeps the image size: each block becomes its mean (NaN skipped), which
/// halves the noise of a difference image at the cost of resolution.
pub fn bin2x2(values: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut out = values.to_vec();
    for by in (0..height).step_by(2) {
        for bx in (0..width).step_by(2) {
            let cells: Vec<usize> = [(bx, by), (bx + 1, by), (bx, by + 1), (bx + 1, by + 1)]
                .into_iter()
                .filter(|&(x, y)| x < width && y < height)
                .map(|(x, y)| y * width + x)
                .collect();
            let finite: Vec<f32> = cells.iter().map(|&i| values[i]).filter(|v| v.is_finite()).collect();
            let mean = if finite.is_empty() { f32::NAN } else { finite.iter().sum::<f32>() / finite.len() as f32 };
            for i in cells {
                out[i] = mean;
            }
        }
    }
    out
}

fn read_at(file: &mut File, at: u64, buf: &mut [u8]) -> Result<usize> {
    file.seek(SeekFrom::Start(at))?;
    let mut n = 0;
    while n < buf.len() {
        let r = file.read(&mut buf[n..])?;
        if r == 0 {
            break;
        }
        n += r;
    }
    Ok(n)
}

/// Offset of the next `FFF\0` at or after `from` whose directory parses.
fn find_next(b: &[u8], from: usize) -> Option<usize> {
    (from..b.len().saturating_sub(4)).find(|&i| b[i..].starts_with(b"FFF\0") && fff::directory(&b[i..]).is_some())
}

/// Decodes a JPEG-LS frame. A stream cut short at the end of a file (the last frame of some
/// recordings) is retried with its end marker restored and the missing bytes zero-filled; the
/// last pixels may then be approximate, but the frame is not lost.
fn decode_jpegls(data: &[u8], w: usize, h: usize) -> Result<Vec<u16>> {
    let decode = |stream: &[u8]| -> Option<Vec<u16>> {
        let (px, dw, dh) = jpegls::decode(stream, w as u32, h as u32).ok()?;
        ((dw as usize, dh as usize) == (w, h) && px.len() == w * h).then_some(px)
    };
    if let Some(px) = decode(data) {
        return Ok(px);
    }
    if !data.ends_with(&[0xFF, 0xD9]) {
        let body = data.strip_suffix(&[0xFF]).unwrap_or(data);
        for missing in 0..=8 {
            let mut stream = body.to_vec();
            stream.extend(std::iter::repeat_n(0u8, missing));
            stream.extend_from_slice(&[0xFF, 0xD9]);
            if let Some(px) = decode(&stream) {
                return Ok(px);
            }
        }
    }
    Err(Error::Open("JPEG-LS frame could not be decoded".into()))
}

/// The frame with JPEG-LS raw data replaced by uncompressed little-endian counts, which flyr reads.
/// A frame cut short at the end of a file is padded so the decoder can try it.
/// Frame sizes and block lengths are capped (`limits`), and a frame cut inside a non-raw record is refused.
fn decompressed(mut block: Vec<u8>) -> Result<Vec<u8>> {
    let dir = fff::directory(&block).ok_or_else(|| Error::Open("unreadable video frame".into()))?;
    let raw = *dir.get(fff::RAW_DATA).ok_or_else(|| Error::Open("video frame without raw data".into()))?;
    // The decoder slices the other records unchecked, so a frame cut inside one of them is refused.
    if dir.entries.iter().any(|e| e.kind != fff::RAW_DATA && e.offset.saturating_add(e.length) > block.len()) {
        return Err(Error::Open("video frame is truncated".into()));
    }
    let end = (raw.offset + raw.length).min(block.len());
    if raw.offset + 34 > end {
        return Err(Error::Open("video frame is truncated".into()));
    }
    let header: [u8; 32] = block[raw.offset..raw.offset + 32].try_into().unwrap();
    let (w, h) =
        (u16::from_le_bytes([header[2], header[3]]) as usize, u16::from_le_bytes([header[4], header[5]]) as usize);
    let data = &block[raw.offset + 32..end];
    let n = crate::limits::pixel_count(w, h).ok_or_else(|| Error::Open("video frame size is implausible".into()))?;
    let pixels: Vec<u16> = if data.starts_with(&[0xFF, 0xD8]) {
        decode_jpegls(data, w, h)?
    } else if data.len() >= n * 2 {
        // Uncompressed, possibly with trailing bytes: keep exactly one frame of counts.
        data[..n * 2].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()
    } else {
        return Err(Error::Open("video frame is truncated".into()));
    };
    // Append a new raw record and point the directory entry at it; other records are unchanged.
    let at = block.len();
    block.extend_from_slice(&header);
    block.extend(pixels.iter().flat_map(|v| v.to_le_bytes()));
    let length = (32 + pixels.len() * 2) as u32;
    let (off_bytes, len_bytes) = if dir.little_endian {
        ((at as u32).to_le_bytes(), length.to_le_bytes())
    } else {
        ((at as u32).to_be_bytes(), length.to_be_bytes())
    };
    block[raw.entry_at + 12..raw.entry_at + 16].copy_from_slice(&off_bytes);
    block[raw.entry_at + 16..raw.entry_at + 20].copy_from_slice(&len_bytes);
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little-endian FFF frame with raw data (2×2 counts) and a camera-info record.
    pub fn frame(counts: [u16; 4], secs: u32, jpegls: bool) -> Vec<u8> {
        let mut info = vec![0u8; 0x480];
        let put = |info: &mut Vec<u8>, at: usize, v: f32| info[at..at + 4].copy_from_slice(&v.to_le_bytes());
        // A plausible camera calibration (from a FLIR T640) and capture parameters.
        put(&mut info, 0x20, 0.95);
        put(&mut info, 0x24, 1.0);
        put(&mut info, 0x28, 293.15);
        put(&mut info, 0x2c, 293.15);
        put(&mut info, 0x30, 293.15);
        put(&mut info, 0x34, 1.0);
        put(&mut info, 0x3c, 0.5);
        put(&mut info, 0x58, 17096.453);
        put(&mut info, 0x5c, 1428.0);
        put(&mut info, 0x60, 1.0);
        put(&mut info, 0x70, 0.006569);
        put(&mut info, 0x74, 0.01262);
        put(&mut info, 0x78, -0.002276);
        put(&mut info, 0x7c, -0.00667);
        put(&mut info, 0x80, 1.9);
        info[0x308..0x30c].copy_from_slice(&(-5786i32).to_le_bytes());
        put(&mut info, 0x30c, 0.0128);
        info[0x384..0x388].copy_from_slice(&secs.to_le_bytes());
        let mut raw = vec![0u8; 32];
        raw[2..4].copy_from_slice(&2u16.to_le_bytes());
        raw[4..6].copy_from_slice(&2u16.to_le_bytes());
        if jpegls {
            let mut enc = Vec::new();
            jpegls::encode(&counts, 2, 2, &mut enc).unwrap();
            raw.extend(enc);
        } else {
            raw.extend(counts.iter().flat_map(|v| v.to_le_bytes()));
        }
        let mut b = vec![0u8; 64 + 64];
        b[..4].copy_from_slice(b"FFF\0");
        b[0x24] = 1; // little-endian swap pattern
        b[24..28].copy_from_slice(&64u32.to_le_bytes());
        b[28..32].copy_from_slice(&2u32.to_le_bytes());
        let entry = |b: &mut Vec<u8>, i: usize, kind: u16, off: usize, len: usize| {
            let e = 64 + 32 * i;
            b[e..e + 2].copy_from_slice(&kind.to_le_bytes());
            b[e + 12..e + 16].copy_from_slice(&(off as u32).to_le_bytes());
            b[e + 16..e + 20].copy_from_slice(&(len as u32).to_le_bytes());
        };
        entry(&mut b, 0, fff::CAMERA_INFO, 128, info.len());
        entry(&mut b, 1, fff::RAW_DATA, 128 + info.len(), raw.len());
        b.extend(info);
        b.extend(raw);
        b
    }

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kelvinlens-video-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn indexes_and_decodes_seq_and_csq_alike() {
        let counts = [[14000, 15000, 16000, 17000], [14100, 15100, 16100, 17100], [14200, 15200, 16200, 17200]];
        let seq: Vec<u8> =
            counts.iter().enumerate().flat_map(|(i, c)| frame(*c, 1_600_000_000 + i as u32, false)).collect();
        let csq: Vec<u8> =
            counts.iter().enumerate().flat_map(|(i, c)| frame(*c, 1_600_000_000 + i as u32, true)).collect();
        let (a, b) = (temp_file("a.seq", &seq), temp_file("a.csq", &csq));
        let (va, vb) = (Video::open(&a).unwrap(), Video::open(&b).unwrap());
        assert_eq!((va.len(), vb.len()), (3, 3));
        assert!(!va.info().compressed && vb.info().compressed);
        assert_eq!(va.info().times, vec![0.0, 1.0, 2.0]);
        assert_eq!(va.info().frame_rate, Some(1.0));
        for i in 0..3 {
            let (fa, fb) = (va.frame(i, None).unwrap(), vb.frame(i, None).unwrap());
            assert_eq!((fa.width(), fa.height()), (2, 2));
            assert_eq!(fa.kelvin(), fb.kelvin(), "frame {i}");
        }
        // Hotter counts, hotter frames.
        assert!(va.frame(2, None).unwrap().kelvin()[0] > va.frame(0, None).unwrap().kelvin()[0]);
        std::fs::remove_dir_all(a.parent().unwrap()).unwrap();
        std::fs::remove_dir_all(b.parent().unwrap()).unwrap();
    }

    #[test]
    fn params_apply_to_frames_and_series() {
        let seq: Vec<u8> = (0..2).flat_map(|i| frame([15000, 15000, 15000, 15000], 1_600_000_000 + i, false)).collect();
        let p = temp_file("b.seq", &seq);
        let v = Video::open(&p).unwrap();
        let base = v.frame(0, None).unwrap().kelvin()[0];
        let lower_e = Params { emissivity: Some(0.8), ..Default::default() };
        let changed = v.frame(0, Some(&lower_e)).unwrap().kelvin()[0];
        // The same as recomputing the decoded frame; this object is colder than its surroundings,
        // so a lower emissivity makes it colder still.
        let mut direct = v.frame(0, None).unwrap();
        direct.set_params(&lower_e).unwrap();
        assert_eq!(changed, direct.kelvin()[0]);
        assert!(changed < base, "{changed} vs {base}");
        let m = [Measurement::new("Sp1", crate::Shape::Spot { x: 0.5, y: 0.5 })];
        let rows = v.series(0, 5, Some(&lower_e), &m).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].unwrap().max, changed);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn segment_round_trips_and_truncated_frame_is_kept_when_decodable() {
        let csq: Vec<u8> = (0..3).flat_map(|i| frame([14000 + i as u16 * 100; 4], 1_600_000_000 + i, true)).collect();
        // Cut the final two bytes (the JPEG-LS end marker).
        let p = temp_file("c.csq", &csq[..csq.len() - 2]);
        let v = Video::open(&p).unwrap();
        assert_eq!(v.len(), 3);
        let last = v.frame(2, None).unwrap();
        let out = p.with_file_name("seg.seq");
        assert_eq!(v.write_segment(1, 2, &out).unwrap(), 2);
        let seg = Video::open(&out).unwrap();
        assert_eq!(seg.len(), 2);
        assert!(!seg.info().compressed);
        assert_eq!(seg.frame(1, None).unwrap().kelvin(), last.kelvin());
        assert!(v.write_segment(2, 1, &out).is_err());
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn binning_averages_blocks_and_keeps_size() {
        let v = [1.0, 3.0, 10.0, f32::NAN, 5.0, 7.0, 20.0, 30.0, 100.0];
        let b = bin2x2(&v, 3, 3);
        assert_eq!(b.len(), 9);
        // Top-left block: 1, 3, NaN, 5 → 3. Top-right column: 10 and 7 → 8.5.
        assert_eq!((b[0], b[1], b[3], b[4]), (3.0, 3.0, 3.0, 3.0));
        assert_eq!((b[2], b[5]), (8.5, 8.5));
        assert_eq!((b[6], b[7], b[8]), (25.0, 25.0, 100.0));
    }

    #[test]
    fn rejects_non_video() {
        let p = temp_file("d.seq", b"not a video at all, just some text that is long enough to read a header from");
        assert!(Video::open(&p).is_err());
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
        assert!(is_video(Path::new("x/Rec-0001.SEQ")) && is_video(Path::new("a.csq")) && !is_video(Path::new("a.jpg")));
    }

    #[test]
    fn difference_with_no_reference_is_zero() {
        assert_eq!(difference_from_mean(&[5.0; 4], &[], 2, 2, false), vec![0.0; 4]);
    }

    #[test]
    fn difference_from_two_references_is_current_minus_mean() {
        let (a, b) = ([8.0f32; 4], [6.0f32; 4]);
        let out = difference_from_mean(&[10.0; 4], &[&a, &b], 2, 2, false);
        assert_eq!(out, vec![3.0; 4]);
    }

    #[test]
    fn difference_binning_averages_blocks_and_keeps_size() {
        // bin2x2 keeps the image size (each 2×2 block becomes its mean).
        let current = [1.0, 3.0, 10.0, 20.0, 5.0, 7.0, 30.0, 40.0];
        let out = difference_from_mean(&current, &[], 4, 2, false);
        assert_eq!(out.len(), 8);
        let binned = difference_from_mean(&current, &[&[0.0; 8]], 4, 2, true);
        assert_eq!(binned.len(), 8);
        assert_eq!(binned[0], 4.0);
        assert_eq!(binned[2], 25.0);
    }

    #[test]
    fn mutated_video_never_panics() {
        let seeds =
            vec![[frame([1, 2, 3, 4], 0, false), frame([5, 6, 7, 8], 1, false)].concat(), frame([1, 2, 3, 4], 0, true)];
        let path = std::env::temp_dir().join(format!("kelvinlens-mutated-{}.seq", std::process::id()));
        crate::testutil::never_panics("video", &seeds, 600, |b| {
            std::fs::write(&path, b).unwrap();
            if let Ok(v) = Video::open(&path) {
                for i in 0..v.len().min(3) {
                    let _ = v.frame(i, None);
                }
            }
        });
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn regression_frame_cut_inside_camera_info() {
        let mut f = frame([1, 2, 3, 4], 0, false);
        let cut = f.len() - 10;
        f.truncate(cut);
        assert!(decompressed(f).is_err());
    }
}
