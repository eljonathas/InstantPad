# Rust audit, November 2026

A read-only review of every Rust crate. Four reviewers covered these areas:

- [audio](audio.md)
- [voice-ai and training](voice-ai-training.md)
- [application and domain](application-domain.md)
- [desktop shell, platform, storage and catalog](desktop-platform-storage-catalog.md)

Each report gives every finding a file:line, the reason it matters and a fix. The high and
medium findings were then checked by hand against the code.

## Baseline when the audit started

- `cargo clippy --workspace --all-targets` with the workspace lints (`clippy::all = deny`, `unsafe_code = deny`, `unused = deny`) gave no warnings.
- `cargo test --workspace`: 658 tests, all passing.
- `cargo check --target x86_64-pc-windows-gnu` passes for `platform` and `training`, so Windows-only code can be type-checked on Linux.
- Pedantic clippy (`pedantic`, `nursery`, `unwrap_used`, `expect_used`, `indexing_slicing`) on library code gave about 1,380 warnings:
  - 440 are long literals without separators;
  - about 225 are `unwrap`/`expect`, mostly `lock().expect("… poisoned")`;
  - about 100 are casts that can truncate or lose precision;
  - 42 are guards that could be dropped earlier.

## Verdict

This is not careless generated code. The reviewers kept running into decisions that hold up under
scrutiny:

- **Audio callbacks.** They take no locks, allocate nothing and log nothing. A counting global allocator in tests proves it.
- **Mixer.** The mixer and its control side talk only through `rtrb` rings.
- **Device threads.** Every device runs on its own thread with a timeout.
- **DSP.** Denormals are flushed in every IIR filter.
- **Locks and async.** No std `MutexGuard` lives across an `.await`. Lock ordering is consistent.
- **Downloads.** Every download is pinned by size and SHA-256, written to a `.part` file, and renamed only once its hash matches.
- **Tauri surface.** It is small: no fs, shell or http plugins, no asset protocol, and a strict CSP.
- **Windows `unsafe`.** It is sound and carries accurate SAFETY comments.
- **Comments.** They explain why rather than restate the code.

The weak spots are the usual ones for code that grew fast:

- trust in what the webview sends;
- a few destructive error paths;
- unbounded memory for decoded sounds;
- disk work on the UI thread or under shared locks;
- every lock turning one panic into a broken session;
- duplicated download code.

## What to fix first

| ID | Sev | One line |
|----|-----|----------|
| TAU-1 / APP-1 | high | The webview sends `SoundSource.path` and `sound_url`, and the backend downloads, renames and copies with them unchecked |
| PLT-1 | high | Only the VB-CABLE setup exe is checked; the rest of the zip sits beside it in `%TEMP%` while it runs elevated, so a planted DLL can be side-loaded |
| AUD-1 | high | Every pad is decoded in full into RAM (48 kHz stereo f32), with no cap or budget |
| VAI-1 | medium | A model ONNX Runtime refuses is deleted, including trained voices that cannot be downloaded again |
| STO-1 | medium | Any read error counts as a first run, and the next save replaces the user's file with defaults |
| TRN-1 | medium | The training runner waits for pipe EOF before the child exits; a lingering descendant hangs it |
| APP-2 | medium | `bars` comes unbounded from IPC, so `vec![0.0; bars]` can try to allocate 16 GiB |
| APP-8 / VAI-5 | medium | Every lock `expect`s, so one panic poisons it for the rest of the session |
| TAU-2 / APP-7 | medium | Sync commands run on the main thread, and saves happen under shared locks |

The tables at the top of each report list the rest.
