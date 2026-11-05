//! Takes are written as 16-bit WAV files, which RVC reads on every system.

use std::io::{self, Write};

/// The size of the data of `samples` and of the whole file, if both fit in the header. The
/// header adds 36 bytes to the data, and that sum must fit as well.
fn sizes(samples: usize) -> Option<(u32, u32)> {
    let data = u32::try_from(samples.checked_mul(2)?).ok()?;
    Some((data, data.checked_add(36)?))
}

/// Mono 16-bit PCM at `rate`, with the samples clipped to -1..1.
pub(crate) fn write(target: &mut impl Write, speech: &[f32], rate: u32) -> io::Result<()> {
    let (data, riff) = sizes(speech.len())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "take too long for a WAV"))?;
    target.write_all(b"RIFF")?;
    target.write_all(&riff.to_le_bytes())?;
    target.write_all(b"WAVEfmt ")?;
    target.write_all(&16_u32.to_le_bytes())?;
    target.write_all(&1_u16.to_le_bytes())?; // PCM
    target.write_all(&1_u16.to_le_bytes())?; // mono
    target.write_all(&rate.to_le_bytes())?;
    target.write_all(&(rate * 2).to_le_bytes())?; // bytes a second
    target.write_all(&2_u16.to_le_bytes())?; // bytes a frame
    target.write_all(&16_u16.to_le_bytes())?;
    target.write_all(b"data")?;
    target.write_all(&data.to_le_bytes())?;
    let mut block = Vec::with_capacity(speech.len().min(1 << 16) * 2);
    for chunk in speech.chunks(1 << 16) {
        block.clear();
        for sample in chunk {
            let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            block.extend_from_slice(&value.to_le_bytes());
        }
        target.write_all(&block)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_take_whose_header_would_overflow_is_refused() {
        assert_eq!(sizes(4), Some((8, 44)));
        let biggest = (u32::MAX as usize - 36) / 2;
        assert!(sizes(biggest).is_some());
        assert_eq!(sizes(biggest + 1), None);
        assert_eq!(sizes(usize::MAX), None);
    }

    #[test]
    fn a_take_becomes_a_mono_16_bit_wav() {
        let mut file = Vec::new();

        write(&mut file, &[0.0, 1.0, -1.0, 2.0], 48_000).unwrap();

        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), 48_000);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 8);
        let samples: Vec<i16> = file[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(samples, [0, i16::MAX, -i16::MAX, i16::MAX]);
        assert_eq!(file.len(), 44 + 8);
    }
}
