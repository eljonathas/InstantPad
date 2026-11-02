//! SHA-256 of what the app keeps track of without keeping it.

use sha2::{Digest, Sha256};

pub(crate) fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The little-endian bytes of the speech, in hex.
pub(crate) fn of_speech(speech: &[f32]) -> String {
    let mut hasher = Sha256::new();
    let mut bytes = [0; 4096];
    for block in speech.chunks(bytes.len() / 4) {
        for (slot, sample) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(block) {
            slot.copy_from_slice(&sample.to_le_bytes());
        }
        hasher.update(&bytes[..block.len() * 4]);
    }
    hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_of_no_speech_is_the_hash_of_nothing() {
        assert_eq!(
            of_speech(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn the_fingerprint_hashes_the_samples_as_little_endian_floats() {
        let mut bytes = Vec::new();
        for sample in [1.0_f32, -2.0, 0.5] {
            bytes.extend(sample.to_le_bytes());
        }

        assert_eq!(of_speech(&[1.0, -2.0, 0.5]), hex(&Sha256::digest(&bytes)));
        assert_eq!(of_speech(&[1.0, -2.0, 0.5]).len(), 64);
    }

    #[test]
    fn a_long_speech_is_hashed_across_blocks() {
        let speech: Vec<f32> = (0..3000).map(|n| n as f32).collect();
        let bytes: Vec<u8> = speech
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();

        assert_eq!(of_speech(&speech), hex(&Sha256::digest(&bytes)));
    }
}
