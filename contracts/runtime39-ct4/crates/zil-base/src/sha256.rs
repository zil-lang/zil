//! SHA-256 through RustCrypto's safe streaming interface. Runtime CPU dispatch
//! stays inside the fixed dependency; digest bytes and callers are unchanged.
use sha2::{Digest, Sha256 as Backend};

#[derive(Clone)]
pub(crate) struct Sha256(Backend);

impl Sha256 {
    pub(crate) fn new() -> Self {
        Self(Backend::new())
    }

    pub(crate) fn update(&mut self, input: &[u8]) {
        self.0.update(input);
    }

    pub(crate) fn finish(self) -> [u8; 32] {
        let digest = self.0.finalize();
        let mut bytes = [0; 32];
        bytes.copy_from_slice(&digest);
        bytes
    }
}

pub(crate) fn digest(input: &[u8]) -> [u8; 32] {
    let mut state = Sha256::new();
    state.update(input);
    state.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FingerprintBuilder, sha256_scalar_reference as reference};
    use std::hash::Hasher;

    #[test]
    fn rustcrypto_matches_scalar_at_streaming_boundaries() {
        let bytes = (0..8193).map(|i| (i * 73 + 19) as u8).collect::<Vec<_>>();
        let mut current = Sha256::new();
        let mut previous = reference::Sha256::new();
        let mut start = 0;
        for end in [
            0, 1, 55, 56, 63, 64, 65, 127, 128, 129, 4095, 4096, 4097, 8193,
        ] {
            current.update(&bytes[start..end]);
            previous.update(&bytes[start..end]);
            current.update(&[]);
            previous.update(&[]);
            let expected = previous.clone().finish();
            assert_eq!(current.clone().finish(), expected, "stream prefix {end}");
            assert_eq!(digest(&bytes[..end]), expected, "one-shot prefix {end}");
            start = end;
        }
        assert_eq!(current.finish(), previous.finish());
    }

    #[test]
    fn cloned_streams_and_observational_finish_preserve_scalar_identity() {
        let prefix = [23; 63];
        let mut left = Sha256::new();
        let mut previous_left = reference::Sha256::new();
        left.update(&prefix);
        previous_left.update(&prefix);
        let mut right = left.clone();
        let mut previous_right = previous_left.clone();

        let mut public = FingerprintBuilder::new();
        public.update(&prefix);
        let expected = reference::digest(&prefix);
        for _ in 0..2 {
            assert_eq!(
                public.finish(),
                u64::from_le_bytes(expected[..8].try_into().unwrap())
            );
        }
        left.update(&[41; 65]);
        previous_left.update(&[41; 65]);
        public.update(&[41; 65]);
        let expected_left = previous_left.finish();
        assert_eq!(left.finish(), expected_left);
        assert_eq!(public.digest().0, expected_left);

        right.update(&[7; 55]);
        previous_right.update(&[7; 55]);
        let expected_right = previous_right.finish();
        assert_eq!(right.finish(), expected_right);
        assert_ne!(expected_left, expected_right);
    }
}
