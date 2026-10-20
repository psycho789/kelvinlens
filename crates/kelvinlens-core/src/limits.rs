//! Upper bounds applied before allocating from sizes a file declares.

/// Largest image, in pixels, the decoders accept (the panorama canvas limit, shared).
pub(crate) const MAX_PIXELS: usize = 40_000_000;
/// Largest single video frame block, in bytes.
pub(crate) const MAX_FRAME_BYTES: usize = 256 << 20;

/// `width * height` when it does not overflow and is within `MAX_PIXELS`.
pub(crate) fn pixel_count(width: usize, height: usize) -> Option<usize> {
    width.checked_mul(height).filter(|n| *n <= MAX_PIXELS)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caps_and_overflow() {
        assert_eq!(pixel_count(640, 480), Some(307_200));
        assert_eq!(pixel_count(65_535, 65_535), None);
        assert_eq!(pixel_count(usize::MAX, 2), None);
        assert_eq!(pixel_count(0, 5), Some(0));
    }
}
