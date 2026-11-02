// Runs InstantPad's voice effect (same library and settings as crates/audio/src/voice.rs)
// over mono 48 kHz f32le read from stdin, writing the same format to stdout.
use signalsmith_stretch::Stretch;
use std::io::{Read, Write};
const SR: usize = 48_000;
const BLOCK: usize = SR * 10 / 1000;
fn main() {
    let a: Vec<f32> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let (semis, formant) = (a[0], a[1]);
    let mut raw = Vec::new();
    std::io::stdin().read_to_end(&mut raw).unwrap();
    let input: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let mut st = Stretch::new(1, 40 * SR / 1000, 10 * SR / 1000);
    let mut out = Vec::with_capacity(input.len());
    let mut buf = [0f32; BLOCK];
    let mut changed = [0f32; BLOCK];
    for chunk in input.chunks(BLOCK) {
        buf.fill(0.0);
        buf[..chunk.len()].copy_from_slice(chunk);
        st.set_transpose_factor_semitones(semis, None);
        st.set_formant_factor_semitones(formant, true);
        st.process(buf, &mut changed);
        out.extend_from_slice(&changed[..chunk.len()]);
    }
    let mut o = std::io::stdout().lock();
    for s in out { o.write_all(&s.to_le_bytes()).unwrap(); }
}
