# Audit: `crates/audio`

Scope: every file in `crates/audio/src` (about 5.6k lines), read in full. I also read the call sites in `crates/application` and `apps/desktop/src-tauri/src/lib.rs` where they matter, and checked the vendored `rubato-5.0.1`, `signalsmith-stretch-0.1.3` and `symphonia-core-0.6.1` sources for behaviour the findings depend on. I made no edits and ran no cargo commands.

Severity counts: 0 critical, 1 high, 4 medium, 12 low/nit.

Overall the real-time design is careful. Most of the findings sit at the edges: background threads, memory, rare failure paths. The core mixing path itself looks sound.

---

## What is done well (verified)

- **Mixer split** (`mixer.rs`). `Mixer` (the callback side) and `MixerHandle` (the control side) talk only through `rtrb` rings. Finished voices go back through a return ring, so the callback never drops the last `Arc<SoundClip>`. I checked the capacity argument (`RETURN_CAPACITY = COMMAND_CAPACITY + MAX_VOICES`, `mixer.rs:21-23`). At most 256 voices can be queued plus 64 playing, and `add()` drains the return ring before each push. So the ring cannot fill, and the `let _ = self.returned.push(voice)` at `mixer.rs:223` cannot drop on the RT thread. `voices` is pre-sized to `MAX_VOICES` and guarded at `mixer.rs:168`, so it never reallocates.
- **Allocation tests** (`allocation_tests.rs`). A counting `#[global_allocator]` checks that every callback path (mixing, capture, voice, colour, AI, recording, error callback, resampled playback) does zero allocations. On macOS the `malloc_logger` hook also catches C++ allocations from signalsmith. This is unusually rigorous.
- **One thread per device** (`role.rs`). Each device gets its own thread plus an 8 s timeout, so a stuck driver cannot block the other roles. A `!Send` `cpal::Stream` is created and dropped on its own thread.
- **Error callback** (`devices.rs:97-114`). It only does an `AtomicU8::fetch_max`. The tests pin the WASAPI error mapping.
- **Mic drift handling** (`mic.rs`). Backlog is dropped down to a whole-frame prebuffer, and the stream re-primes after running dry. Dropping uses `read_chunk().commit_all()`, which does not allocate.
- **DSP hygiene** (`color.rs`). Denormals are flushed in every IIR state (`flush()`), and the comb buffers are only fed flushed values. FFT plans, scratch space and every buffer are made in `new`. Nothing logs in any callback, and rubato's `log` feature is off (`Cargo.lock`: no `log` dependency).
- **No lock held across `.await`.** Every guard is a temporary within one statement (e.g. `engine.rs:71`, `engine.rs:161-168`). `async_trait`'s `Send` bound would also reject a held `std` guard.
- **Off-runtime decoding.** Decode and resample run in `spawn_blocking` (`engine.rs:174`, `engine.rs:400`), so a panic there becomes a `JoinError` and then a `Failure`, not a crash. Speech decode is capped by `MAX_SPEECH_SECONDS`, and recordings by `MAX_RECORDED`.
- **Thread shutdown.** Background threads end through `Weak::upgrade`, and the `Worker` drop wakes them (`worker.rs:38-42`, `recording.rs:137-151`, `ai.rs:239`).
- **Cut ranges.** They are clamped, and a backwards cut becomes an empty range (`clip.rs:50-61`), so the mixer's slice `samples()[position..end]` (`mixer.rs:45`) cannot go out of bounds for any range from `range()`.
- **Comments.** They explain the reasons behind the code rather than repeating it, and I saw few signs of generated filler. The style is consistent.

---

## Findings

### AUD-1 — HIGH — Every pad's sound is fully decoded into RAM as 48 kHz stereo f32, with no length cap or memory budget
- `clip.rs:114-139`, `clip.rs:190-216`, `clip.rs:230-241`; `engine.rs:32`, `engine.rs:161-181`; caller `application/src/board.rs:98-111` (`preload_sounds` preloads every pad at startup).
- **What is wrong.** `SoundClip` stores interleaved f32 stereo at 48 kHz, which is 23 MB per minute of audio. `preload_sounds` decodes every sound on the board, and `clips` (a `HashMap`) keeps them until `forget()`. Nothing limits a pad's length, and there is no total budget (I found no size or duration limit in `catalog`, `application` or `domain`; the only one is `SampleTooLong` for cloning). Loading also costs more than the final size:
  - `decode` grows `stereo` by doubling with no `reserve`.
  - `resample` (`process_all`) builds a second full copy while the input copy is still alive.
  - So a non-48 kHz file briefly needs about 2–3× its final size.
- **Why it matters.** Soundboard users often drop whole songs on pads. Twenty 5-minute songs come to about 2.3 GB of resident memory after startup. One 1-hour file needs about 1.4 GB, plus the same again at peak while resampling.
- **Fix.**
  1. Cap the duration of a pad sound, or stream long files from disk through a decoder thread and an rtrb ring.
  2. Store `i16` instead of f32 (halves memory) and convert to f32 in `mix_into`; that costs one multiply per sample.
  3. Add an LRU byte budget to `clips` so only recently used sounds stay decoded.
  4. `reserve` from `track.num_frames` when it is known.
- **Related.** Finished voices (each holding an `Arc<SoundClip>`) are only collected on the next `add()` or `is_playing()` (`mixer.rs:107`, `mixer.rs:142-150`). So after `forget()`, a large clip stays in memory until then. `now_playing()` polling hides this today.

### AUD-2 — MEDIUM — `Worker::wake` silently drops every body after the first, so the AI "too slow" message is stuck on the first conversion type
- `worker.rs:23-36` together with `ai.rs:198-210`.
- **What is wrong.** `wake` runs `get_or_insert_with(|| spawn(body))`. Once the thread exists, the `body` passed in is dropped without running. `AiBridge::route` builds a fresh `Converting` with `too_slow` chosen by the conversion type (`AiTrainedTooSlow` for a trained voice, `AiTooSlow` for the instant engine). After the first route, though, that new value never reaches the running thread.
- **Why it matters.** Start with an instant (Timbre) voice, then switch to a trained voice that falls behind: the user sees `AiTooSlow` instead of `AiTrainedTooSlow`, which loses the "use the instant engine" hint. The reverse switch gives the wrong hint. The tests (`ai.rs` ~L775 `a_trained_voice_that_falls_behind_suggests_the_instant_engine`) each use a fresh bridge with only one conversion type, so they miss this.
- **Fix.** Keep `too_slow` in `AiShared`, for example as an `AtomicU8` written in `route()` before the generation bump, and have `serve()` read it in `give_up`. Alternatively, make `wake` take only the name and pass the body once at construction. That would also stop `route()` from building and dropping a `Converting` (three `Arc` clones) on every knob step.

### AUD-3 — MEDIUM — A panic in the converter thread leaves the microphone silent forever with no error
- `ai.rs:232-272`, `worker.rs:23-36`, `ai.rs:94-113`.
- **What is wrong.** `self.converter.convert(...)` (`ai.rs:257`) calls into a foreign implementation: ONNX Runtime in `voice-ai`. If it panics, the "voice conversion" thread dies. `Worker` still holds `Some(Thread)` for the dead thread, so later `wake()` calls only unpark a dead handle and never respawn it. `is_routing` stays `true`, so `AiStage` keeps routing. Nothing comes back from the converter, `is_primed` never becomes true, and the callback sends zeros to the game and monitor rings. No error event is sent. Turning the AI voice off and on again changes nothing, because `route` only bumps the generation and wakes the dead thread. Only restarting the app recovers.
- **Why it matters.** The user is muted in-game with no feedback, and the cause is outside this crate's control.
- **Fix.**
  1. Wrap the `convert` call in `std::panic::catch_unwind` (with `AssertUnwindSafe`). On a panic, call `give_up(generation, Message::text(TextKey::...))`, which turns routing off and sends the error.
  2. Also make `Worker` keep the `JoinHandle` and respawn when `is_finished()`.

  The same `Worker` weakness applies to the recording drain thread (`recording.rs:143`, `kept.lock().unwrap()`), although nothing in that path panics today.

### AUD-4 — MEDIUM — A timed-out device job still runs later and opens a "zombie" stream; queued retries also hold large ring buffers
- `role.rs:39-56`, `engine.rs:69-89`, `engine.rs:312-348`.
- **What is wrong.** `Role::run` stops waiting after 8 s, but the job stays in the role's queue. If the driver call (`build_input_stream`, `build_output_stream`, `play`) finally returns, the job stores a live stream in the role's `Option<Stream>`. The engine has already given up on it and cleared the reader slots. Concretely:
  - **Microphone:** a capture stream runs with nobody reading it. The OS "mic in use" indicator is on while the UI shows the device failed.
  - **Output:** a stream plays a mixer whose handle was dropped.
  - **Health:** the stale stream's `health` `Arc` may still be the current one in `HealthSlot`, so its errors are reported for the role.
  - **Memory:** each queued mic retry already owns its rings before it runs. That is `mic::ring` ×2 (384 KB each), AI rings ×2 (384 KB each) and the recorder ring (192 KB), about 1.7 MB per queued job, held until the driver unblocks. The `recovery` back-off limits how many build up.
- **Fix.** Give each job a shared `Arc<AtomicBool> abandoned` and set it when the timeout fires. The job checks it after opening and drops the stream immediately if it is set. Build the rings inside the job, or only after the previous job has answered, so queued jobs stay small.

### AUD-5 — MEDIUM — `load_speech` resamples both channels and then throws one away
- `clip.rs:122-139`.
- **What is wrong.** `load_speech` calls `load_stereo`: it decodes up to 601 s of stereo at the file's rate and resamples that stereo with a two-channel FFT resampler into a new `Vec`. Only then does it `downmix` into a third `Vec`. For a 10-minute 44.1 kHz take, that is about 212 MB (decoded stereo) plus about 230 MB (resampled stereo), both alive at once, then about 115 MB of mono. The resampler also does twice the necessary work.
- **Fix.** Downmix to mono right after decoding: `append_as_stereo` can take a target channel count, or downmix per packet. Then resample with `Fft::new(.., 1, ..)`. Peak memory drops by about 3×, and resampling CPU halves.

### AUD-6 — LOW — `expect` and slice-panic paths inside cpal callbacks
- `playback.rs:75,79,82`; `capture.rs:104,108,111`; `mic.rs:51`; `color.rs:275,287`; `playback.rs:55` / `capture.rs:81` (`chunks_exact(self.channels)` panics when `channels == 0`).
- **What is wrong.** These are invariant checks that should not fire with the current sizing. If one does fire inside a callback:
  - On CoreAudio, unwinding through an `extern "C"` callback aborts the whole app.
  - On ALSA and WASAPI it kills the stream thread without calling the error callback, so `StreamHealth` never reports it and recovery never runs.

  A device that reports 0 channels (broken driver or virtual device) is the one input-driven trigger.
- **Fix.** Reject `channels == 0` in `Conversion::new` and `CaptureProcessor::new`; they already return `Result`. In the callbacks, replace the `expect`s with `debug_assert!` plus a silent fallback: fill the block with zeros and mark `StreamHealth` as `FAILED`.

### AUD-7 — LOW — The mixer's mic hand-off uses `Mutex::try_lock` in the output callback
- `mixer.rs:190-208`, `mic.rs:12-23`, `engine.rs:314-315,345-346`.
- **What is wrong.** `try_lock` never blocks, and the control side only holds the lock for a single assignment, so this is mostly fine. But when the control thread is blocked in `lock()` while the callback holds the lock, the callback's unlock makes a futex/`ulock_wake` syscall on the RT thread. Also, the `AudioEngine` doc comment (`engine.rs:125-126`) says the callbacks have "nothing … that locks", which is not literally true.
- **Fix.** Hand the `MicReader` over through the mixer's existing command ring (`Command::SetMic(Option<MicReader>)`) and return the old one through a return ring. That makes the path truly lock-free and lets the `empty_slot` workaround go. Otherwise, correct the doc comment.

### AUD-8 — LOW — Race between `give_up` and `route` can turn off a conversion that just started
- `ai.rs:276-281` against `ai.rs:194-197`.
- **What is wrong.** `give_up` loads `generation`, compares it, then stores `is_routing = false`. If `route()` bumps the generation and sets `is_routing = true` between that load and the store, the new conversion is switched off, and the user gets an error that belonged to the previous one.
- **Fix.** Pack generation and the routing bit into one `AtomicU64` and use `compare_exchange` in `give_up`. Or take the `active` mutex in `give_up`, which `route` already holds.

### AUD-9 — LOW — Multi-field control updates are published with `Relaxed` ordering, so the callback can see a mix of old and new values
- `voice.rs:43-65` (`low`, `high`, `seconds`, `formant`, `color`, then `is_on`, all `Relaxed`); `controls.rs:12-14`.
- **What is wrong.** On weakly ordered CPUs (Apple Silicon), the callback can see `is_on == true` together with stale `low`, `high` or `formant` values. Between two chains it can also mix `low` from one with `high` from the other. The result is one 10 ms block at the wrong pitch, an audible chirp when a preset is switched.
- **Fix.** Use `Release` on the final `is_on` store and `Acquire` on its load (`voice.rs:99`). To make whole chains switch atomically, add a version counter (seqlock) or publish the chain through a small ring.

### AUD-10 — LOW — Mono output devices get only the left channel
- `playback.rs:512-526`; the test `a_mono_device_gets_the_left_side` pins this behaviour.
- **What is wrong.** For a 1-channel device, `frame[..1]` copies only L. Any content panned right in a stereo clip is lost: on mono virtual cables, headsets or Bluetooth HFP, it plays quieter or not at all.
- **Fix.** For `channels == 1`, write `(l + r) * 0.5`. Capture already does the reverse sensibly, duplicating mono to both sides.

### AUD-11 — LOW — The output stream is always opened as f32, even when the device's native format is not f32
- `devices.rs:116-153` (the closure takes `&mut [f32]`), compared with `devices.rs:188-198` (capture handles I16, I24 and I32).
- **What is wrong.** Outside Windows, a device whose default config is I16 or I32 gets asked for f32 anyway. ALSA `hw:` devices, which are listed because `output_devices()` enumerates every host's devices, usually refuse that. The user then sees `DeviceUnavailable`. I have not run this on hardware, so it needs confirming on Linux ALSA `hw:` devices.
- **Fix.** Use the same `match native.sample_format()` as capture, with a generic `build_output_stream::<T>`, and convert from f32 with `T::from_sample`.

### AUD-12 — LOW — NaN or Inf in a decoded file passes straight to the device
- `mixer.rs:228-247`, `clip.rs:33-43`.
- **What is wrong.** `soft_clip(NaN)` returns NaN. `peak.max(NaN)` ignores the NaN, so the meter shows nothing wrong. A float WAV holding non-finite samples sends NaN to the driver, which some drivers turn into full-scale noise or a muted stream.
- **Fix.** In `SoundClip::new`, which runs off the RT thread, replace non-finite samples with 0.0. Optionally, guard `limit()` with `if !sample.is_finite() { 0.0 }`, which costs one compare.

### AUD-13 — LOW — One late container error throws away a whole decoded sound
- `clip.rs:192-215`.
- **What is wrong.** `next_packet()?` and any decode error other than `DecodeError` return `Err`, even after minutes of good audio. `Error::ResetRequired` (chained Ogg, or a track list that changes mid-stream) is not handled. Symphonia's docs say that error requires re-creating the decoder, not failing.
- **Fix.** If `stereo` is not empty, end the loop on an IO error or `ResetRequired` and keep what was decoded, possibly with a warning.

### AUD-14 — LOW — Device enumeration is synchronous and repeats periodically
- `devices.rs:28-50`, called through `engine.rs:304-310`. Callers: `application/src/setup.rs:310-312` (`pick_devices`, from `load_devices`) and `bring_back` every `LOOK_TICKS` while a stand-in device is in use (`recovery.rs:111-114`).
- **What is wrong.** Stream opening is carefully isolated with a timeout, but listing devices calls cpal `input_devices()` and `output_devices()` across all hosts, plus `description()`, inline. On Linux, ALSA enumeration opens PCMs and can be slow, and can even cause pops on some hardware. A hung driver here blocks the device-watch thread, or the async task that calls `load_devices`.
- **Fix.** Enumerate on a role-style thread with a timeout, or cache the list and refresh it only on OS device-change notifications. At least use `spawn_blocking` on the async path.

### AUD-15 — LOW — `Recorder::peek` copies the whole recording on every call
- `recording.rs:120-124`; caller `application/src/voice.rs:566`, every 0.5 s while calibrating.
- **What is wrong.** Each tick clones up to 40 s of mono (about 7.7 MB) and the caller re-scans all of it. That is O(n²) work over the listening window.
- **Fix.** Add a "take since offset" or "drain new" API, so the caller only receives and processes new samples.

### AUD-16 — NIT/pattern — Duplicated resampler boilerplate
- `capture.rs:99-115` and `playback.rs:528-550`.
- **What is wrong.** Both files repeat the same `Fft::new(.., FixedSync::Input)`, `InterleavedSlice::new`/`new_mut` and three `expect` calls, sized from `input_frames_next()`/`output_frames_max()`.
- **Fix.** Extract a small `BlockResampler` (construct, `process(&[f32], &mut [f32]) -> usize`). Then the panic policy from AUD-6 only has to be fixed in one place.

### AUD-17 — NIT — Small amounts of wasted CPU in the callbacks
- `color.rs:175`: `drive.powf(0.75)` is computed for every sample although `drive` is fixed per block. Hoist it out of the loop.
- `voice.rs:133`: `self.stretch.process(self.mono, …)` copies the 480-float array by value. Pass `&self.mono` instead; the field borrows are disjoint.
- `ai.rs:91-93` and `recording.rs:376-380` push one sample at a time into `rtrb`, with an atomic load and store each time. Use `write_chunk_uninit`/`push_partial_slice` with a small downmix scratch buffer.
- `color.rs:75-78`: `Color::reset` resets only whisper and echo. The growl and brightness filter states keep stale values, which can cause a tiny click when the effect is turned back on.

---

## Things I checked and found correct (not findings)
- **Mic ring left/right alignment.** Every writer pushes even lengths, and every read (`MIC_BLOCK` 1024, device stereo blocks, resampler blocks) is even, so `backlog - PREBUFFER` stays frame-aligned. The invariant is implicit but holds.
- **`Conversion::fill`'s `while self.next == self.ready` loop** (`playback.rs:516`). It ends, because a `FixedSync::Input` FFT resampler always produces output after its start-up blocks; the tests cover 11,025 and 22,050 Hz.
- **Lock order.** `now_playing` takes `started` and then `voices`, and nothing takes them in the reverse order.
- **`Recorder` drain thread.** It polls every 20 ms only while recording, parks otherwise, and exits when `Recorder` drops. The AI thread polls every 10 ms only while routing. Both are acceptable, since the RT side should not `unpark`.
- **Overflow and casts.** None of the `as` casts in non-test code truncate in practice on 64-bit targets (`clip.rs:46`, `clip.rs:55`).
- **Stretch allocations.** The vendored signalsmith `process` path only resizes in `seek` and `flush`, which are not used; `findPeaks` uses a reserved `Vec`.
