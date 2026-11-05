# Audit: `crates/voice-ai` and `crates/training`

Scope: every `.rs` file in both crates, plus the call sites that change how they behave
(`crates/audio/src/ai.rs`, `crates/application/src/{voice,training}.rs`,
`apps/desktop/src-tauri/src/{lib,composition}.rs`, `crates/storage/src/data_folder.rs`). I also
read the ort 2.0.0-rc.15 and reqwest 0.13.5 sources in `~/.cargo/registry` to check how they
load libraries and follow redirects, and the matching parts of `tools/voice-training/voicekit.py`.
Context came from ADR 0003/0006/0007 and docs/architecture/README.md. Nothing was built or
changed. Line numbers are for the current working tree.

Severity scale: critical / high / medium / low. **No critical or high findings.** The supply-chain
basics are solid: every download is pinned, size-capped and hashed before use, and there is no
archive path traversal. What remains is mostly robustness, cleanup and resource use.

---

## Summary table

| ID | Sev | Where | One line |
|---|---|---|---|
| VAI-1 | medium | voice-ai/src/runtime.rs:57-66 | A model ORT will not open is **deleted**, including user-trained synthesizers and HuBERT, which cannot be downloaded again |
| VAI-2 | medium | storage/src/data_folder.rs:23 (feeds voice-ai) | The models and the native ORT library are kept in the **roaming** profile, which goes against ADR 0007's own reasoning |
| VAI-3 | medium | voice-ai/src/lib.rs:41-43, runtime.rs:22-50 | Cached sessions (tone_color, HuBERT) are never unloaded; the doc comment says otherwise |
| VAI-4 | low | voice-ai/src/pack.rs:228-235 + runtime.rs:81 | The native library is loaded after only a size check (hash is checked only at install time) |
| VAI-5 | low | voice-ai/src/runtime.rs:43,77; lib.rs:66,257,258,262,266,281 | `lock().unwrap()`: one panic during inference poisons the lock, and every later call panics |
| VAI-6 | low | voice-ai/src/trained.rs:78-96 | TOCTOU: the source is hashed, then copied again without checking the copy |
| VAI-7 | low | voice-ai/src/trained.rs:87, lib.rs:163 | `id` is not checked in `add`/`synthesizer_path`, though `remove` checks it |
| VAI-8 | low | voice-ai/src/pack.rs:244; training/src/download.rs:35 | No `https_only(true)`, so a redirect can downgrade to http |
| VAI-9 | low | voice-ai/src/pack.rs:172-182 | `sw_vers` is spawned through PATH, synchronously, from `is_installed()` |
| VAI-10 | low/nit | lib.rs:233, rvc.rs:151/166, yin.rs:60 | Magic `192` duplicated; small allocations per block/frame on the converter thread |
| TRN-1 | medium | training/src/process.rs:77-82 | `run` waits for pipe EOF before child exit, so a lingering descendant hangs it (and on Windows, the Job that would kill it closes only after `run` returns) |
| TRN-2 | medium | training/src/process.rs:110-133 | Unix: nothing cleans up if the app dies; descendants left after a normal exit are never killed |
| TRN-3 | medium | training/src/lib.rs:171,222,399-429; pins.rs:95 | uv is extracted straight to its final path, and `exists()` is the only check: a truncated uv sticks forever, and a new uv pin is never applied |
| TRN-4 | low | training/src/process.rs:26-28, 59-61 | Stop on Unix shells out to `kill` through PATH, ignores failure, and blocks inside an async fn |
| TRN-5 | low | training/src/process.rs:52-55, 119-127 | Windows: the child is assigned to the Job after it starts; anything it spawns in that gap escapes |
| TRN-6 | low | training/src/download.rs:42-59; lib.rs:255-267 | Wheels already on disk are accepted by size alone and installed without hashes |
| TRN-7 | low | training/src/lib.rs:516-533 | On `nvidia-smi` timeout, the process and the blocking thread are leaked |
| TRN-8 | low | training/src/lib.rs:90-94,156,285-293,340,588,677-682 | Blocking `std::fs` in async fns; a full tree walk at app start |
| TRN-9 | low | training/src/lib.rs:137-152 | Child processes inherit the whole app environment (deny-list, not allow-list) |
| TRN-10 | low | training/src/lib.rs:703-704 | `--title <user text>`: a title starting with `-` breaks argparse |
| TRN-11 | low | training/src/process.rs:65; voicekit.py:183-187 | Lines are split only on `\n`; `\r` progress output piles up into one long line |
| TRN-12 | low | training/src/wav.rs:7-10 | `36 + data` can overflow after the `u32` check |
| TRN-13 | low | training/src/lib.rs:579 | Windows path-length limit counts WTF-8 bytes, not UTF-16 units |
| TRN-14 | low | training/src/lib.rs:678-682 | `training.log` is appended to forever |
| TRN-15 | low | training/src/events.rs:31; lib.rs:353 | Events (including `exported`) are trusted from any line on the shared pipe |
| TRN-16 | nit | lib.rs:208-219, 373-382; download.rs:126-139 | Two Mutexes for progress; `Vec::remove(0)`; stop check by polling |
| DUP-1 | low | pack.rs / download.rs / trained.rs / pins.rs / lib.rs | Download, hash, hex and single-entry-unzip code written 2-4 times, with different strictness |
| INFO | — | training/Cargo.toml:14 | `fs4` is used only for `available_space`; there is **no** file locking (single-instance plugin covers it) |

---

## voice-ai

### VAI-1 (medium): a model ORT cannot open is deleted, even when it cannot be downloaded again
`crates/voice-ai/src/runtime.rs:57-66`
```rust
if matches!(error.code(), ErrorCode::InvalidProtobuf | ErrorCode::InvalidGraph | ErrorCode::NoModel) {
    let _ = std::fs::remove_file(&file);
}
```
- `session()` is used for every model: the pack files, **and** `content.onnx` (HuBERT, which only
  comes from the user's training export, trained.rs:79-93), **and**
  `trained/<id>/synthesizer.onnx` (lib.rs:123, 156, through `Running::start`).
- The comment says "the interface offers the download again". That is only true for the two
  OpenVoice files in `pack.rs`. A trained voice (hours of GPU time) or the shared HuBERT has no
  download. An `InvalidGraph` can come from a runtime/opset mismatch rather than file damage
  (for example a model exported for a newer opset than ORT 1.23 supports). One such error
  silently deletes user data, and HuBERT is shared by every trained voice, so all of them break.
- Fix: delete only the files `pack.rs` owns. Have `pack` expose `is_pack_file(name)`, or move the
  delete into `Cached`/`voiceprint` for pack models. For trained/HuBERT files, return
  `TrainedModelInvalid` and leave the file alone. In `add`, the rollback through
  `trained::remove` already cleans up.

### VAI-2 (medium): models and the native ORT library are in the roaming profile
`crates/storage/src/data_folder.rs:23` (`voice_models = root.join("voice-models")`, where `root` is
`app_data_dir()`, which is `%APPDATA%` (Roaming) on Windows), used by
`apps/desktop/src-tauri/src/composition.rs:89`.
- This folder holds about 180 MB (onnxruntime.dll ~15.7 MB, tone_color 157 MB), plus every trained
  synthesizer and HuBERT (hundreds of MB). ADR 0007 deliberately puts training in the *local*
  folder "not in the roaming one, which a domain profile copies around". The same reasoning
  applies here, and is stronger: a **native DLL** syncs from a profile server and then loads
  after only a size check (see VAI-4).
- Fix: use `self.local.join("voice-models")`. Migrate once by moving the old folder, or let
  `remove_retired` delete the old one and download again.

### VAI-3 (medium): cached ONNX sessions stay loaded for the life of the app
`crates/voice-ai/src/lib.rs:41-43`, `runtime.rs:22-50`
- The doc comment on `content` says it "stays loaded while one of them speaks". Nothing ever
  clears `Cached::session`: there is no `unload`, `start()` does not reset it, and switching to the
  Timbre engine or turning AI off leaves it. Once a trained voice has been used, HuBERT (typically
  ~190 MB on disk, more in memory with ORT arenas) stays resident until exit. `tone_color` (157 MB
  file) does the same after one timbre conversion.
- Fix: add `Cached::unload()`, and call it from `SpeechConverter::start` when the new conversion
  does not need that model (and when routing stops), or keep the session inside `Running` so it
  drops with the conversion. At minimum, correct the comment.

### VAI-4 (low): the native library is loaded with only a size check
`pack.rs:228-235` (`is_installed` checks sizes only), `runtime.rs:54,81` (`ort::init_from(library)`).
- The SHA-256 is checked only during `install`. A DLL/so of the right size that was replaced
  afterwards (roaming sync, VAI-2, or another process running as the same user) is loaded into
  the process. The threat model is same-user, so this is low. Combined with VAI-2 it is the main
  integrity gap in the crate.
- Checked in ort source (`ort-2.0.0-rc.15/src/lib.rs:117-133`): the path is absolute, so
  libloading does not search for the main library. The ort `api()` fallback to `ORT_DYLIB_PATH`
  or a bare `onnxruntime.dll` is never reached, because every ort call here comes after
  `environment()`. That part is good.
- Fix: hash the library once per process before `init_from` (~16-30 MB, so cheap, and already on
  a blocking thread), then cache the verdict in the `OnceLock` next to `ENVIRONMENT`.

### VAI-5 (low): lock poisoning turns one panic into permanent panics
`runtime.rs:43` (the `Cached::session` lock is held while inference runs), `runtime.rs:77`,
`lib.rs:66, 257, 258, 262, 266, 281`.
- `convert` holds `conversion` and `Cached::session` while it runs code that can panic on bad
  model output: slice indexing in `rvc::join` (rvc.rs:213-214), `frames_of`, `follow_pitch`
  (rvc.rs:185-186), or a panic inside ort. After that, every `lock().unwrap()` panics. The
  converter thread in `crates/audio/src/ai.rs:231` has no `catch_unwind`, so the AI voice dies for
  the session. `start`/`retune` run on UI/command threads, so a poisoned `requested`/`retuned`
  would also panic Tauri commands.
- Fix: `lock().unwrap_or_else(PoisonError::into_inner)` (the state is reset on the next `start`
  anyway), or `parking_lot::Mutex`. On the converter side, consider `catch_unwind` that maps to
  `give_up`.

### VAI-6 (low): hash, then copy without checking the copy (TOCTOU)
`crates/voice-ai/src/trained.rs:78-96`. `checked()` hashes `source/synthesizer.onnx`, then
`copy()` (line 162) runs `fs::copy` on it again. If the source changes in between (the export
folder is user-writable, and the trainer may still be writing it), the copied file is never
checked. Fix: hash while copying, as `pack::copy_hashed` already does, and compare the hash of
the `.part` before the rename.

### VAI-7 (low): the model id is checked in one place only
`trained.rs:87` (`models.join(TRAINED).join(id)`) and `lib.rs:163`
(`format!("{}/{}/{}", TRAINED, model.id, SYNTHESIZER)`) take `id` as given. `trained::remove`
(trained.rs:114) refuses `/ \ .`. In `add`, ids come from `TrainedModel::new_id()` (a uuid), but in
`Running::start` the `TrainedModel` comes from persisted settings, so an edited settings file can
point model loading at any `.onnx`. Same-user, so low. Fix: one `fn is_plain_id(&str)` used in all
three places (training has `is_plain` at training/src/lib.rs:390).

### VAI-8 (low): redirects may downgrade to http
`pack.rs:244-248`, `training/src/download.rs:34-39`. reqwest 0.13 defaults to `https_only: false`
(`reqwest-0.13.5/src/redirect.rs:279,324`) and follows up to 10 redirects. The pinned hash stops
substitution, so the only effects are privacy and wasted bandwidth (capped at the pinned size).
Fix: `.https_only(true)` on both builders. GitHub, PyPI and HF redirect https to https.

### VAI-9 (low): `sw_vers` lookup
`pack.rs:172-182`. It runs `Command::new("sw_vers")` through PATH, synchronously, the first time
`runtime()` runs. That happens inside `is_installed()`, which `install_ai` calls under a std
Mutex on an async path (`application/src/voice.rs:453`). It runs once per process and is fast, but
use `/usr/bin/sw_vers`, or read `/System/Library/CoreServices/SystemVersion.plist`.

### VAI-10 (low/nit): magic numbers and small per-block allocations
- `lib.rs:233` `words.noise.len() / 192` repeats `rvc::FEATURES_OF_FLOW` (rvc.rs:225, private).
  Export the constant.
- `rvc.rs:151` collects `heard.drain(..)` into a temporary Vec before `extend`. Use
  `self.window.extend(self.heard.drain(..BLOCK*FRAME))`. `rvc.rs:166` allocates the noise Vec per
  block, and `yin.rs:60` allocates `normalized` per frame (36 per block). These could be reused
  buffers. They are cheap at 300 ms blocks, so this is a nit.
- `resample.rs:17-21,40,60-67` and `stft.rs` use `expect` on invariants (fixed rates, sized
  buffers). That is acceptable; each message states the invariant.

### voice-ai: done well
- Every download is pinned by URL (HF pinned to a commit), size and SHA-256 (pack.rs:77-156).
  Writing streams the hash and **aborts as soon as more bytes than pinned arrive**
  (pack.rs:380-383), so there is no disk exhaustion. The `.part` file becomes the real file only
  after size and hash match (pack.rs:390, 350), and is always cleaned up (pack.rs:356-358).
- The wheel zip is opened only **after** its own hash matches, so the zip parser only ever sees
  trusted bytes. One entry is read by exact name (`by_name`) into a fixed path, so zip-slip is
  impossible. The extracted library is hashed again (pack.rs:398-412).
- `trained.rs::checked` refuses any listed file that is not a single `Component::Normal`
  (absolute, `..`, nested), with tests (trained.rs:121-145, 325-336).
- Hashing and inference run in `spawn_blocking` (pack.rs:270, 344; lib.rs:70-77) or on the
  dedicated converter thread. No async fn holds a std Mutex across `.await`.
- `ENVIRONMENT` is initialized once and the API is only touched after it, so ort never falls back
  to its PATH/CWD search.
- Thorough unit tests with an in-process HTTP server for the pinned/short/oversized/wrong-entry
  cases.

---

## training

### TRN-1 (medium): `process::run` can hang after the child has exited
`crates/training/src/process.rs:63-82`
```rust
while let Some(line) = received.recv().await { on_line(&line); }   // waits for EOF on the pipe
let status = tokio::task::spawn_blocking(move || child.wait()).await
```
- EOF needs **every** inherited write end closed. Any descendant that outlives the kit and
  inherited its stdout/stderr keeps `run` blocked forever, with the slot still holding the
  group. Examples: a uv helper, a stray `ffmpeg`, a DataLoader worker left after a crash. On
  Windows the Job that would kill that descendant (KILL_ON_JOB_CLOSE) closes only when the last
  `Arc<Group>` drops, which is **after** `run` returns: a circular wait. On Unix nothing kills it
  except a user Stop. The UI shows "training" with no progress.
- Fix: wait for the child's exit first, or `select!` on both. Once the child has exited, call
  `group.end()` to kill leftovers (this is also the right cleanup on Unix, see TRN-2), then drain
  the remaining lines with a short timeout. That is also the place to join the reader thread
  (currently detached).

### TRN-2 (medium): Unix has no cleanup for crashes or leftover processes
`process.rs:110-116` (`process_group(0)`), `131-133`, `20-29`.
- A graceful exit is handled: `RunEvent::Exit`, then `Core::shut_down`, then `training.stop()`
  (apps/desktop/src-tauri/src/lib.rs:197, composition.rs:75). If the app is SIGKILLed, crashes,
  is OOM-killed, or the session logs out, the training process group (PyTorch on the GPU for
  hours) keeps running orphaned, and the next start may run a second training over the same
  `rvc/logs/<job>`. Windows gets this for free from KILL_ON_JOB_CLOSE; Unix has no equivalent.
- Also on Unix, descendants still alive after the leader exits normally are never killed (on
  Windows the Job drop kills them).
- Fix, with no `unsafe`: give the kit `stdin(Stdio::piped())` and keep the write end in `Group`.
  The kit watches stdin for EOF in a daemon thread and kills its process tree (`os.killpg`). The
  OS closes the pipe on any app death. Also call `group.end()` after a normal exit (TRN-1).

### TRN-3 (medium): uv install is not atomic and never updated
`lib.rs:222-234` (gated by `self.uv().exists()`), `lib.rs:171`, `lib.rs:399-407` (Windows),
`lib.rs:410-429` (Linux).
- `unpack_uv` does `fs::File::create(target)` + `io::copy` **directly into the final path**. If
  the app is killed mid-copy, or the disk fills, a truncated `uv/uv(.exe)` is left. Every later
  install skips extraction because `exists()` is true, then fails at `uv venv` with an obscure
  error until the user runs "Remove training tools". On Linux there is a second window between
  `create` and `set_permissions(0o755)`.
- `UV_VERSION`/`UV_SHA256` are not part of `fingerprint()` (pins.rs:95-110 hashes only the
  environment JSON), and `exists()` never compares versions. So a new uv pin in a later app
  version never replaces an installed uv. The same applies to the venv's Python
  (`self.python().exists()`, lib.rs:236): a changed `environment.python` reuses the old
  interpreter.
- Fix: extract to `uv.part`, `sync_all`, set mode, rename. Store the uv version (and Python
  version) in `tools.json`, or include them in the fingerprint, and reinstall when they differ.
  Optionally hash the extracted binary, as voice-ai does for its library.

### TRN-4 (low): Unix stop shells out to `kill`, ignores failure and blocks
`process.rs:26-28`. `Command::new("kill")` is resolved through PATH. Spawn failure or non-zero
status is ignored (`let _ =`), so a stop that failed looks like success (the UI goes to
"stopped" while training continues). `Group::end` is also called inside the async `run`
(process.rs:59-61), which blocks the tokio worker for a fork+exec. Fix: use a safe wrapper that
works under `deny(unsafe_code)` (`rustix::process::kill_process_group(pid, Signal::Kill)` or
`nix::sys::signal::killpg`), and log or return the error.

### TRN-5 (low): Windows Job assignment happens after the process starts
`process.rs:52-55, 119-127`. The child runs freely between `spawn()` and
`AssignProcessToJobObject`, and anything it spawns in that window is outside the Job. For
`uv`/`python` the window is short and grandchildren usually come later, so this is low. The
fully correct form is `CREATE_SUSPENDED`, then assign, then resume the main thread (std `Child`
does not expose the thread handle). Another option is to put the app process itself in a
KILL_ON_JOB_CLOSE job once at startup so every child inherits it (nested jobs need Win8+).
Either way, document the race.

### TRN-6 (low): wheels already on disk are trusted by size alone
`download.rs:42-44, 56-59` returns early when `target` has the pinned size, and the hash is
checked "when it is downloaded". Then `uv pip install --no-deps --offline --no-index <files>`
(lib.rs:255-267) installs whatever is there. A wheel damaged on disk or replaced with the same
size goes into the venv unchecked, even though the hashes are available. Fix: write a
requirements file with `name @ file://… --hash=sha256:…` and pass `--require-hashes`, or hash in
`fetch` before the early return (the wheels are deleted after install anyway, lib.rs:285, so this
costs little).

### TRN-7 (low): `nvidia-smi` timeout leaks the process
`lib.rs:516-533`. `tokio::time::timeout` around `spawn_blocking(|| command.output())` gives up
waiting, but the blocking thread and the `nvidia-smi` process keep running, which is exactly the
"driver in a bad state" case the timeout is for. Fix: `tokio::process::Command` (the `process`
feature is already enabled) with `.kill_on_drop(true)` and `timeout(..., cmd.output())`.

### TRN-8 (low): blocking filesystem work on async threads and at startup
- In async fns: `kit()` writes voicekit.py (lib.rs:156); `remove_dir_all(wheels)` (GBs) and
  `cache` (lib.rs:285-286); `fs::write(tools.json)` (lib.rs:293); `writeln!(log, …)` per output
  line inside the `run_kit` callback (lib.rs:340); `create_dir_all` and log open (lib.rs:588,
  677-682). Most are small; the `remove_dir_all` of the wheels and the per-line log writes are the
  ones worth moving to `spawn_blocking` or `tokio::fs`.
- `KitTrainer::new` (lib.rs:86-102) calls `tools_on_disk(home)` synchronously when `tools.json` is
  absent. That is a recursive walk of a partial install (a venv with PyTorch has tens of
  thousands of files), run from `composition::build` during Tauri `setup`, so it delays app start.
  Fix: start with 0 and call `measure_leftover()` lazily or in the background.

### TRN-9 (low): child processes inherit the whole app environment
`lib.rs:137-152` removes only `PYTHONPATH`, `PYTHONHOME` and `VIRTUAL_ENV`. Everything else in the
app's environment reaches uv, Python, RVC and third-party code, including any `UV_*` the user has
set (e.g. `UV_PYTHON_INSTALL_MIRROR` affects `uv venv --managed-python`; `UV_NO_CONFIG` stops
config files, not environment variables), plus `LD_PRELOAD`/`PYTHONSTARTUP`-style variables and
anything secret the app inherited. Same user, so low, but the stated intent is "nothing of a
Python the user may have". Fix: `env_clear()` plus an allow-list (PATH, SystemRoot, TEMP/TMP,
HOME/USERPROFILE, LOCALAPPDATA, proxy variables, CUDA_*/NVIDIA_*, LANG).

### TRN-10 (low): a title starting with `-` breaks argparse
`lib.rs:699-705` passes `"--title", run.title` as two argv entries. There is no shell, so no
injection (good). But argparse treats a value that starts with `-` (e.g. a voice named "-x" or
"--name") as an option, and the export step fails after the whole training. Fix:
`format!("--title={}", run.title)`.

### TRN-11 (low): lines split only on `\n`
`process.rs:65` (`split(b'\n')`). The kit prints download progress with a bare `\r` and
`end=""` (voicekit.py:183-187), so each file's progress arrives as one growing line of
`\r…\r…` fragments. `remember()` keeps it, and it can become the error text shown to the user
(lib.rs:319). Bounded (~KBs), but split on `\r` as well and drop empty fragments.

### TRN-12 (low): WAV header overflow
`wav.rs:7-10`. `data` is checked to fit in `u32`, but `36 + data` is then computed in `u32`: it
panics in debug and wraps in release for `data > u32::MAX - 36`. Takes are seconds long, so
this is theoretical. Fix: `u32::try_from(len*2).ok().filter(|d| d <= &(u32::MAX - 36))`, or
`checked_add`.

### TRN-13 (low): Windows path-length check counts the wrong unit
`lib.rs:579` uses `self.home.as_os_str().len()`, which on Windows is the WTF-8 **byte** length.
MAX_PATH counts UTF-16 units, so a non-ASCII profile name (CJK: 3 bytes per unit) is refused
early. Fix: `self.home.as_os_str().encode_wide().count()`.

### TRN-14 (low): `training.log` grows without limit
`lib.rs:678-682` opens it in append mode on every train/resume and writes every line of RVC
output. Cap it (rotate at N MB) or truncate on a fresh (non-resume) training.

### TRN-15 (low): events are trusted from any line on the shared pipe
`events.rs:31`, `lib.rs:341-355`. The kit forwards RVC's own output verbatim on the same pipe, so
any library line starting `@instantpad {"exported": "<path>"}` or `{"error": …}` is accepted. The
exported folder is then checked by voice-ai only against the `voice.json` inside that same folder
(self-attesting). The impact is low (same user, trusted toolchain). Making it robust would mean
events on a separate fd or a per-run nonce in the prefix.

### TRN-16 (nit): idioms
- `lib.rs:208-219`: two `Mutex`es (locked nested, in a consistent order, so no deadlock) plus
  `expect`. One `Mutex<(u64, u8)>` or an `AtomicU64` is enough.
- `lib.rs:373-382`: `Vec::remove(0)` on a ring of 3 lines. `VecDeque` states the intent.
- `download.rs:126-139`: `unless_stopped` re-arms a 200 ms `timeout` in a loop. It works and is
  cheap, but a `tokio::sync::Notify`/`CancellationToken` set by `stop()`, used with `select!`,
  removes the polling and the latency.
- Training failures are `Message::plain` strings (lib.rs:394-396), while voice-ai uses `TextKey`s.
  Untranslated, mixed-language errors reach the UI.
- `pins.rs:44-45` `KIT_DOWNLOAD_BYTES` sums sizes copied by hand from voicekit.py. A test that
  parses the kit's pins would catch drift (it only affects progress).

### training: done well
- `fetch` caps the size while streaming (`have > file.size` deletes and fails, download.rs:103-107),
  resumes with `Range` and falls back correctly on a non-206 reply (download.rs:79-88), hashes on a
  blocking thread, and renames only after the size and SHA-256 match (download.rs:114-121).
- The uv archive is hash-checked before it is opened. One entry is matched by exact name and
  written to a fixed path (zip and tar). There is no `unpack()`/`extractall`, so no `..`,
  absolute-path or symlink traversal. The kit's own `unpack_rvc` checks `resolve().parents`
  (voicekit.py:296-308).
- `pip install --no-deps --offline --no-index` installs only the files the app downloaded and
  checked.
- **The `unsafe` code is sound.** It sits in one `#[allow(unsafe_code)]` module with SAFETY
  comments. `CreateJobObjectW(None, null)` gives a non-inheritable handle.
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is set through `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`
  with the correct size. The child handle is borrowed for `AssignProcessToJobObject`.
  `CloseHandle` happens exactly once in `Drop`, and `Job` is neither Clone nor Copy. No handle
  leaks on any error path: a failed `SetInformationJobObject` or assign drops the `Job`. Storing
  the handle as `isize` to get `Send + Sync` is fine for a kernel handle. A failed assign kills
  the child tree with `taskkill /T /F`.
- The stop race is handled correctly. `stop()` stores `stopping` (SeqCst) and then locks the
  slot. `run()` publishes to the slot and then loads `stopping`. The mutex orders the two, so one
  side always ends the program (process.rs:56-61, lib.rs:603-606). There is a test for it.
- `drop(command)` after spawn, so the pipe's write end closes (process.rs:54). Unix process group
  plus `kill -- -pgid`.
- Arguments go through argv, never a shell. `is_plain` checks `job`/`take` before any path is
  joined. `remove_dir_all` does not follow symlinks (std >= 1.58.1), and `disk_bytes` uses
  `symlink_metadata`, so there is no symlink loop.
- No std Mutex guard is held across `.await` anywhere in the crate (checked: the guards at
  process.rs:56/84/96 and lib.rs:190/211/214/294/542 are all statement-scoped or taken after the
  last await).
- Takes are written to `.part`, then `sync_all`, then renamed, and cleaned up on error
  (lib.rs:635-652).

---

## DUP-1 (low): duplicated download, hash and unzip code across the crates
- **Download to `.part`, cap size, check SHA-256, rename** exists twice:
  `voice-ai/src/pack.rs:328-395` and `training/src/download.rs:49-122`. Only the training copy
  can resume or be stopped (voice-ai's 157 MB file restarts from zero after any network error).
- **SHA-256 of a file** exists three times: `pack.rs:279-297` (64 KiB blocks),
  `trained.rs:147-159` (1 MiB, `{:x}`), `download.rs:146-161` (1 MiB, hand-rolled hex).
  **Hex encoding** is hand-rolled with one `format!` per byte in `pack.rs:207-209`,
  `download.rs:153-157` and `pins.rs:104-108`, while `trained.rs:158` uses `format!("{:x}", digest)`.
- **Single-entry unzip:** `pack.rs:398-412` hashes the extracted entry; `training/src/lib.rs:399-407`
  does not, and writes non-atomically (TRN-3).
- **Same constants and setup:** `STALLED_AFTER` (pack.rs:33, download.rs:16), `BYTES_PER_MEGABYTE`
  (pack.rs:34, lib.rs:39), the `Client::builder()` timeouts (pack.rs:244, download.rs:34), the
  percent `Progress` logic (pack.rs:300-325 and lib.rs:208-219).
- Fix: one small internal crate (e.g. `instantpad-fetch`) with
  `Pinned { url, size, sha256 }`, `fetch(resumable, stop)`, `sha256_file`,
  `extract_entry_verified`, and a `Progress` type. Both crates then share the stricter behaviour
  (resume and stop for voice-ai; atomic, hash-checked extraction for training).

## INFO: no file locks
`fs4` (training/Cargo.toml:14) is used only for `fs4::available_space` (lib.rs:567). There is no
`lock_exclusive`/`try_lock` anywhere in either crate, so the "lock files never released" concern
does not apply. Cross-instance races on `training/` and `voice-models/` are covered by
`tauri-plugin-single-instance` (apps/desktop/src-tauri/src/lib.rs:176). Within one instance,
`install_ai` (application/src/voice.rs:451-457) and the training state machine serialize the
work. Nothing protects against a second process outside the app (e.g. the ignored real-model
tests run against a live data folder). That is acceptable, but worth a comment.
