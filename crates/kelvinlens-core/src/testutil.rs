//! Deterministic input mutation for "never panics" tests on the file parsers.

pub(crate) struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    pub fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// `seed` with one to four random edits: bit flip, 0x00, 0xFF, four 0xFF bytes (a `u32::MAX`
/// field) or truncation.
pub(crate) fn mutate(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut b = seed.to_vec();
    for _ in 0..=rng.below(4) {
        if b.is_empty() {
            break;
        }
        let at = rng.below(b.len());
        match rng.below(5) {
            0 => b[at] ^= 1 << rng.below(8),
            1 => b[at] = 0,
            2 => b[at] = 0xFF,
            3 => (at..(at + 4).min(b.len())).for_each(|i| b[i] = 0xFF),
            _ => b.truncate(at),
        }
    }
    b
}

/// Runs `f` on `iterations` mutations of the `seeds`; on a panic, fails with the input bytes.
pub(crate) fn never_panics(name: &str, seeds: &[Vec<u8>], iterations: usize, mut f: impl FnMut(&[u8])) {
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
    for i in 0..iterations {
        let input = mutate(&seeds[i % seeds.len()], &mut rng);
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&input)));
        assert!(
            run.is_ok(),
            "{name}: panic at iteration {i}, {} bytes: {:02x?}",
            input.len(),
            &input[..input.len().min(200)]
        );
    }
}
