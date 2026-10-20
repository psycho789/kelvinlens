//! Bounds-checked readers for the fixed-offset records of FLIR FFF, InfiRay IJPEG and EXIF.
//! Every reader returns `None` instead of panicking when the record is too short.

/// The `u16` at byte `at`.
pub(crate) fn u16_at(b: &[u8], at: usize, little_endian: bool) -> Option<u16> {
    let s: [u8; 2] = b.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if little_endian { u16::from_le_bytes(s) } else { u16::from_be_bytes(s) })
}

/// The `u32` at byte `at`.
pub(crate) fn u32_at(b: &[u8], at: usize, little_endian: bool) -> Option<u32> {
    let s: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(if little_endian { u32::from_le_bytes(s) } else { u32::from_be_bytes(s) })
}

/// The little-endian `f32` at byte `at`; `None` when it is NaN or infinite.
pub(crate) fn f32_le(b: &[u8], at: usize) -> Option<f32> {
    let s: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(f32::from_le_bytes(s)).filter(|v| v.is_finite())
}

/// Text of at most `len` bytes at `at`, cut at the first NUL and trimmed; `None` when empty.
pub(crate) fn text(b: &[u8], at: usize, len: usize) -> Option<String> {
    let raw = b.get(at..at.checked_add(len)?)?;
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    let s = String::from_utf8_lossy(&raw[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readers_check_bounds_and_byte_order() {
        let b = [0x01, 0x02, 0x03, 0x04, 0x00, 0x00, 0x80, 0x3f];
        assert_eq!(u16_at(&b, 0, true), Some(0x0201));
        assert_eq!(u16_at(&b, 0, false), Some(0x0102));
        assert_eq!(u32_at(&b, 0, true), Some(0x0403_0201));
        assert_eq!(u32_at(&b, 0, false), Some(0x0102_0304));
        assert_eq!(f32_le(&b, 4), Some(1.0));
        assert_eq!(f32_le(&[0, 0, 0xc0, 0x7f], 0), None, "NaN is not a reading");
        // Past the end, or an offset that would overflow `usize`.
        assert_eq!(u16_at(&b, 7, true), None);
        assert_eq!(u32_at(&b, 5, true), None);
        assert_eq!(u32_at(&b, usize::MAX, true), None);
        assert_eq!(text(&b, usize::MAX, 2), None);
    }

    #[test]
    fn text_stops_at_nul_and_trims() {
        assert_eq!(text(b"  T640\0junk", 0, 11).as_deref(), Some("T640"));
        assert_eq!(text(b"\0abc", 0, 4), None);
        assert_eq!(text(b"abc", 1, 5), None);
    }
}
