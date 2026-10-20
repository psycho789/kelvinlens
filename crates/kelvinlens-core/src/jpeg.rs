//! Walking the marker segments of a JPEG file, before the image data.

/// One marker segment.
pub(crate) struct Segment<'a> {
    /// The marker byte after `0xFF`, such as `0xE1` for APP1.
    pub marker: u8,
    /// Offset of `data` from the start of the file.
    pub offset: usize,
    /// The payload, without the marker and the two length bytes.
    pub data: &'a [u8],
}

/// The segments found before the start-of-scan marker.
pub(crate) struct Scan<'a> {
    pub segments: Vec<Segment<'a>>,
    /// `false` when the walk stopped early at a byte that is not a marker or at a segment that
    /// runs past the end of the file. The segments before that point are still returned.
    pub well_formed: bool,
}

/// Segments of `bytes` from the start-of-image marker up to the first scan or end-of-image
/// marker. `None` if the file does not start with `FF D8`. Markers without a payload (`TEM`,
/// `RSTn`, `SOI`) are skipped.
pub(crate) fn segments(bytes: &[u8]) -> Option<Scan<'_>> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }
    let mut segments = Vec::new();
    let mut well_formed = true;
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xff {
            well_formed = false;
            break;
        }
        let marker = bytes[i + 1];
        if marker == 0xd8 || (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let end = i + 2 + len;
        if len < 2 || end > bytes.len() {
            well_formed = false;
            break;
        }
        segments.push(Segment { marker, offset: i + 4, data: &bytes[i + 4..end] });
        i = end;
    }
    Some(Scan { segments, well_formed })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SOI, then one segment per `(marker, payload)`, then EOI.
    fn jpeg(parts: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = vec![0xff, 0xd8];
        for (marker, payload) in parts {
            out.extend([0xff, *marker]);
            out.extend((payload.len() as u16 + 2).to_be_bytes());
            out.extend(*payload);
        }
        out.extend([0xff, 0xd9, 0, 0]);
        out
    }

    #[test]
    fn walks_segments_until_the_end_marker() {
        let bytes = jpeg(&[(0xe1, b"abc"), (0xe2, b"de")]);
        let scan = segments(&bytes).unwrap();
        assert!(scan.well_formed);
        let found: Vec<_> = scan.segments.iter().map(|s| (s.marker, s.offset, s.data)).collect();
        assert_eq!(found, vec![(0xe1, 6, &b"abc"[..]), (0xe2, 13, &b"de"[..])]);
        assert_eq!(&bytes[6..9], b"abc");
    }

    #[test]
    fn skips_markers_without_a_payload() {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0x01, 0xff, 0xd3];
        bytes.extend([0xff, 0xe1, 0x00, 0x04, b'h', b'i', 0xff, 0xd9]);
        let scan = segments(&bytes).unwrap();
        assert!(scan.well_formed);
        assert_eq!(scan.segments.len(), 1);
        assert_eq!(scan.segments[0].data, b"hi");
    }

    #[test]
    fn reports_malformed_files_but_keeps_the_good_prefix() {
        // Second segment claims 100 bytes but the file ends.
        let mut bytes = jpeg(&[(0xe1, b"abc")]);
        bytes.truncate(bytes.len() - 4);
        bytes.extend([0xff, 0xe2, 0x00, 0x64, 1, 2]);
        let scan = segments(&bytes).unwrap();
        assert!(!scan.well_formed);
        assert_eq!(scan.segments.len(), 1);
        // A byte that is not a marker where one is expected.
        let scan = segments(&[0xff, 0xd8, 0x12, 0x34, 0x56, 0x78]).unwrap();
        assert!(!scan.well_formed && scan.segments.is_empty());
        assert!(segments(b"not a jpeg").is_none());
    }
}
