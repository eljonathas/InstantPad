# Audit: `crates/application/src` and `crates/domain/src`

Read-only review, done by reading the code (no builds). Adapter and desktop code was read only to
confirm how a finding plays out (who calls what, from which thread, what the adapter does with a
value). Line numbers refer to the files as they are now.

Severity scale: critical / high / medium / low.

---

## Summary table

| ID | Sev | Where | One line |
|----|-----|-------|----------|
| APP-1 | high | library.rs:49-63, 88-95; rows.rs:40-115; favorites.rs:36-50 | `SoundSource.path`/`sound_url` from IPC are trusted as filesystem destination/source and download URL |
| APP-2 | medium | cuts.rs:175-185 | `bars` from IPC is unbounded, so the adapter runs `vec![0.0; bars]`: up to 16 GiB and the process aborts |
| APP-3 | medium | hotkeys.rs:95-106 | The pause counter gets stuck if the webview reloads while paused: global hotkeys stay off until restart |
| APP-4 | medium | board.rs:492-494, 568 | Pad volume (f64 from IPC) is never clamped or checked for finite, then persisted and cast `as f32` |
| APP-5 | medium | board.rs:311-323, 397-403, 442 | Unvalidated `press_pad` slot becomes `target_slot`; `place` then silently does nothing forever and orphans files |
| APP-6 | medium | setup.rs:106-123, 298-307 | `load_devices` overwrites the user's chosen devices with fallbacks, which breaks the stand-in design |
| APP-7 | medium | board.rs:115, settings.rs:83, favorites.rs:43-49, voice_library.rs:28-39, voice.rs:312-390, training.rs:746-749, board.rs:149-157 | Disk I/O and port calls happen while shared locks are held |
| APP-8 | medium | every `lock().expect("… poisoned")` | One panic under a lock permanently breaks the session (and the meter/learn threads) |
| APP-9 | low-med | voice.rs:394-447, 787-795, 929-954 | Read-modify-write of `settings.voice` across separate lock acquisitions loses knob/engine updates |
| APP-10 | low-med | voice.rs:247-273 | VoiceSession state is spread over 6 mutexes + 4 atomics; invariants hold only through careful ordering |
| APP-11 | low | voice.rs:510-515; cloning.rs:93, 249; training.rs:257-280, 466, 482 | `is_microphone_held` is one bool shared by two dialogs, so one dialog releases the other's hold |
| APP-12 | low-med | updates.rs:55-64 | Update progress is not deduplicated: one IPC event per downloaded chunk |
| APP-13 | low | favorites.rs:36-50 | TOCTOU in `toggle`: the check happens before an `.await` and the push after it, so entries can duplicate |
| APP-14 | low | board.rs:297-309 | TOCTOU in `release_file`: the board, favorites and delete steps are not atomic |
| APP-15 | low | board.rs:246, 259; voice.rs:682; cuts.rs:215-217 | Silent failures (BoardError, "model busy", "already placing") |
| APP-16 | low | mixer.rs:42-63; voice.rs:405-408, 951; board.rs:622-626 | Mixer, voice and active interface settings are saved only at shutdown; a crash loses them |
| APP-17 | low | voice.rs:561-570; recorder.rs:51-57 | Calibration polling copies the whole recording (up to 7.7 MB) and reruns VAD every 0.5 s |
| APP-18 | low | board.rs:366-379; voice.rs:450-474, 711-715; cuts.rs:215-219 | Busy flags are not cancel-safe (no RAII guard) |
| APP-19 | low | training.rs:218-232, 351-373, 429-437 | A stop pressed just before install/train starts is lost (the adapter resets the flag) |
| APP-20 | low | training.rs:545-547 | `keep` can return with `Activity::Saving` still set (latent stuck dialog) |
| APP-21 | low | voice.rs:435, 628; voice_library.rs:30 | Whole VoiceLibrary or map cloned for trivial work |
| APP-22 | low | hotkeys.rs:86-122; board.rs:536-537 | Lock held across `host.register` (main-thread marshalled); full unregister and re-register on every edit |
| APP-23 | low | ports.rs:48, 57-64; events.rs:14-15, 24 etc. | `#[specta(type = u32)]` on floats, and a "never NaN" claim that nothing enforces |
| APP-24 | low | message.rs:142-143 | `Failure(Message)`: stringly errors; `Display` prints `Debug` |
| APP-25 | low | board.rs:361; voice.rs:889; cuts.rs:21/rows.rs:12; voice_library.rs:22; settings.rs:79 | Small duplication and aliases; pub mutable guards bypass `change()` |
| APP-26 | low | setup.rs:350-354 | Every settings tweak also re-emits the routing LCD message |
| APP-27 | low | board.rs:401-403, 442-447; cuts.rs:160-164 | Files left orphaned in the store on some failure or race paths |
| DOM-1 | medium | domain/board.rs:260-275 | `swap` does not bound `to_slot` by capacity, so a sound can vanish off-grid |
| DOM-2 | low | domain/board.rs:204-208, 223-228 | `i32` overflow on IPC `offset` (panics in debug, under the board lock, so the lock poisons) |
| DOM-3 | med-low | domain/board.rs:75-88; domain/cuts.rs:22-38 | Invariants are bypassable through pub fields (colour rule, volume, `Cut` min length) |
| DOM-4 | low | domain/board.rs:45-46; voice.rs:248-253; training.rs:221-232 | Persisted data is not validated on load (capacity overflow, raw param values, ids used as paths) |
| DOM-5 | low | domain/voice.rs:271-273 (+ app voice.rs:776-782) | `engines` map grows with arbitrary IPC ids that are never forgotten |
| DOM-6 | low | domain/cuts.rs:56; domain/cloning.rs:40 | Pub API used only by tests (`pieces`, `similarity`) |
| DOM-7 | low | domain/sounds.rs:21-23 | `is_same_sound_as` allocates 2 Strings per compare; called per pad × favorite under 3 locks |
| DOM-8 | low | domain/board.rs:37-53; cloning.rs:201-229, 274-284 | No bound on name/accent length, and empty interface names are allowed |
| DOM-9 | low | domain/trending.rs:26-33 | A region derived from the locale is not checked against `TRENDING_REGIONS` |

---

## Findings in detail

### APP-1 (high): IPC-supplied `SoundSource.path` / `sound_url` are trusted for filesystem and network operations

**Where:** `library.rs:49-63` (`ensure_downloaded`), `library.rs:67-81` (`file_to_hear`),
`library.rs:88-95` (`save_to_computer`), `rows.rs:40-115` (all row actions), `favorites.rs:36-50`.
The IPC commands deserialize `SoundSource` straight from the webview
(`apps/desktop/src-tauri/src/commands/browser.rs:22-59`).

**What is wrong:**
- `self.catalog.download(url, &source.path)` (library.rs:62) writes the HTTP body to
  `{source.path}.N.part` and then renames it to `source.path`, a path the frontend chose. The
  adapter (`crates/catalog/src/lib.rs:75-116`) GETs any URL, with no host check and no size cap.
- `self.files.keep_preview(&preview, &source.path)` (library.rs:60) is `std::fs::rename(preview,
  source.path)`, again to any path.
- `save_to_computer` → `files.export(&source.path, destination)` (library.rs:94) copies any
  readable file on disk to `destination`.
- `file_to_hear` → `files.exists(&source.path)` → plays or decodes any file on disk.
- `toggle_favorite` persists the foreign path in `favorites.json`, and `add_to_pad` puts it on a
  pad, so it lives on across restarts.

**Why it matters:** Tauri's model treats the webview as less trusted than the core. The catalog
injects third-party HTML-derived names into the UI. If the frontend is ever compromised (XSS
through a sound name, a malicious dev dependency), the result is arbitrary file write, arbitrary
file read/exfiltration and GETs to any host. The "the backend owns the rules" principle in ADR 0001
is not applied to paths.

**Fix:** In `SoundLibrary`, do not take `path` from the caller.
- For `sound_url: Some(url)`, recompute `path = files.path_for_download(url)`, and have the catalog
  port check the URL (`SoundCatalog::owns(&url)`: https plus the catalog host).
- For `sound_url: None` (imported files), accept only a path that the store owns
  (`SoundFileStore::is_stored(path)`) or that is already a favorite or on a pad.
- Better still, have the frontend send an opaque id or url, and keep `SoundSource` construction in
  Rust.

### APP-2 (medium): unbounded `bars` makes the waveform allocate without limit

**Where:** `cuts.rs:175-185` passes `bars: u32` from IPC (`commands/cuts.rs:19-25`) to
`SoundPlayer::waveform`. The adapter (`crates/audio/src/clip.rs:76-89`) runs
`vec![0.0; bars]` or `(0..bars).map(..).collect()`.

**Why it matters:** `bars = u32::MAX` means a 16 GiB allocation, and the process aborts. Any
large value stalls the async worker. The application layer is the right place to enforce the
range.

**Fix:** In `CutSession::waveform`, clamp the request with
`let bars = bars.clamp(1, MAX_WAVEFORM_BARS)` (for example 4096). Also normalize
`start_ms <= end_ms <= duration_ms` with `Cut::within`-style logic before calling the port.
Consider putting the limit in `Limits` so the UI knows it.

### APP-3 (medium): the hotkey pause counter breaks when the webview reloads

**Where:** `hotkeys.rs:95-106` (`pause_count += 1` / `saturating_sub(1)`). The frontend already
reference-counts and calls `pauseHotkeys` only on 0→1 and `resumeHotkeys` only on 1→0
(`apps/desktop/src/shared/ui/hotkeyPause.ts:9-15`). The backend outlives a reload (see the
comment in `commands/app.rs`).

**What goes wrong:** Suppose a text field or dialog is open when the window reloads (dev reload,
crash recovery, Ctrl+R). Then the backend's `pause_count` stays at 1. The new frontend starts at
`holds = 0`, so every later pause/resume pair moves the count 1→2→1 and it never reaches 0.
Global hotkeys stay off until the app restarts, which defeats ADR 0001's reason for keeping
hotkeys in Rust.

**Fix:** Counting is already done in the frontend, so make the backend state a boolean
(`paused: bool`), or reset `pause_count` in the `start` command or `BoardSession::start`.

### APP-4 (medium): pad volume is never validated

**Where:** `board.rs:492-494`: `set_volume(slot, volume: f64)` stores the raw value. The value
reaches the player as `sound.volume as f32` (board.rs:568). Compare `MixerSession`, which clamps
to 0..1 (`mixer.rs:43, 49, 55`).

**Why it matters:** JSON cannot carry NaN or Infinity, but `1e6` or `-5` arrive unchanged, and
`1e39` becomes `f32::INFINITY` after the cast. The engine multiplies samples by the gain and
relies on its final `soft_clip` (`crates/audio/src/mixer.rs:231-238`), so the game microphone gets
a full-scale, clipped blast. The value is persisted to `board.json`, so the problem survives
restarts. Also, `SoundView.volume` is exported to TypeScript with the claim "never NaN"
(events.rs:14-15).

**Fix:** Enforce the range in the domain, for example
`Sound::set_volume(v) { if v.is_finite() { self.volume = v.clamp(0.0, MAX_PAD_GAIN) } }`, and make
the field private (see DOM-3). Clamp again on load.

### APP-5 (medium): an out-of-range `target_slot` makes every later add a silent no-op

**Where:**
- `board.rs:315`: `press_pad` stores any `slot` as `target_slot` with no capacity check.
- `board.rs:333-342`: `pad_for_new_sound` prefers `target_slot`.
- `board.rs:397-403`: `place` returns `Ok(())` when the slot does not fit, without clearing
  `target_slot` and without telling the user.
- `board.rs:442`: `import_files` has already copied the file into the store. `rows.rs:65-85` has
  already downloaded it.

**Result:** After `press_pad(999)` on a 4×6 grid, every "add to pad" or "import" without an
explicit slot does nothing, shows nothing and leaves an orphaned file, until the user switches
interface. Legitimate UI paths clear `target_slot` on reconfigure or delete, so this needs a
malformed or out-of-date IPC call, but the backend should not trust the slot.

**Fix:**
- Validate `slot < capacity` in `press_pad` and `pad_on_screen` (return `Option`).
- In `place`, return a `Failure` (for example `InterfaceFull` or a new `PadGone`), clear
  `target_slot`, and `release_file(path)` for the file that was not placed.

### APP-6 (medium): loading devices overwrites the user's chosen devices

**Where:** `setup.rs:106-123` → `refresh_devices` (setup.rs:298-307) writes `choice.of(role)` into
`settings.microphone_id` / `game_output_id` / `monitor_output_id`. The fields doc (setup.rs:27-29)
says the opposite: "The settings keep the chosen one, which comes back as soon as it is there
again."

**Scenario:** The user's USB headset is unplugged when the app starts. `pick_devices` falls back
to the system default, `refresh_devices` makes that the chosen id, and the next settings save
persists it. Recovery is reset to `Running` and there is no stand-in, so nothing ever looks for the
headset again. The test suite covers the runtime case
(`a_device_that_left_is_replaced_until_it_comes_back_and_stays_chosen`) but not the startup case.

**Fix:** In `load_devices`, open the picks as stand-ins. Where `choice.of(role)` differs from the
saved id and the saved id is `Some`, set `stand_ins[role]` and call
`recovery.reopened(role, /*stand_in*/ true)`, and leave settings alone. Only fill in a saved id
when it was `None`.

### APP-7 (medium): I/O and port calls while shared locks are held

All of these are sync calls on hot, shared locks. Tauri runs non-async commands on the main
thread, the meter thread takes the board lock at about 30 Hz, and global hotkeys take the board
and settings locks.

| Site | Lock held | Work under the lock |
|------|-----------|---------------------|
| board.rs:115 `self.repository.save(&self.lock().board)` | board | JSON serialization plus a disk write. In edition 2024 the `if let` scrutinee temporary also lives through the `emit` in the then-branch |
| settings.rs:83-85 `self.repository.save(&self.lock())` | settings (the most shared lock: board.view, trigger, voice.view, setup.view) | disk write |
| favorites.rs:43-49 | favorites | disk write. `BoardSession::view` (board.rs:120-146) holds board → settings while calling `favorites.is_favorite` per pad, so a favorites save stalls the board view and with it hotkey playback (`find_sound`/`sound_in_active` need the board lock) |
| voice_library.rs:28-39 `change` | library | full `clone()` of the library plus a disk write. `VoiceSession::view` holds settings → library |
| voice.rs:312-390 `view` | settings + library | `models.is_installed()` and `models.download_megabytes()`, each a `metadata()` on every model file (`crates/voice-ai/src/pack.rs:217-235`), on **every** publish, which includes every knob step |
| training.rs:746-749 `publish` (always called with the state guard) | training.state | `trainer.tools()` (stats files, `missing_bytes`), `trainer.free_megabytes()` (statvfs), then `voice.show_training` → full `VoiceView` |
| setup.rs:65-76 `view` | settings | `system.system_output_id()` (an OS audio-endpoint query) |
| board.rs:149-157 `playing_slots` | board, 30×/s | `player.is_playing(tag)` per sound, and each call takes both output mixer locks in the engine |

**Why it matters:** Hotkey-to-sound latency and UI stutter both follow disk latency, which matters
for antivirus-scanned AppData on Windows. It also widens every contention window.

**Fix:**
- Snapshot under the lock, drop the guard, then do the I/O. To avoid writes landing out of order,
  either serialize the snapshot under the lock and write it outside, or send snapshots to a single
  writer thread or task with "latest wins" semantics.
- Cache `is_installed` in an `AtomicBool` that is refreshed after `install_ai`.
- Add a `SoundPlayer::playing_tags() -> Vec<PlaybackTag>` (one lock per frame) and match it against
  the board after releasing the lock.

### APP-8 (medium): poisoning turns one panic into a permanently broken session

**Where:** every lock helper, for example `board.rs:663`, `settings.rs:80`, `voice.rs:1065-1075`,
`training.rs:752`, `cloning.rs:261`, `hotkeys.rs` and so on: `.expect("… lock poisoned")`.

**Why it matters:** The state behind these locks is plain data, but any panic while a guard is
held poisons the lock, and from then on every call panics. Examples are DOM-2 in a debug build,
DOM-3/`Cut::duration_ms` underflow, or an adapter panicking inside `player.is_playing`, which is
called under the board lock. The meter thread (`lib.rs:start_meters`) and the learn and
device-watch threads then die on their next iteration, and the board is unusable until restart.

**Fix:** Use a helper `fn lock<T>(m: &Mutex<T>) -> MutexGuard<T> { m.lock().unwrap_or_else(PoisonError::into_inner) }`,
or `parking_lot::Mutex`. Keep invariants valid at every point where a panic could happen, which the
code mostly does already.

### APP-9 (low-medium): lost updates in VoiceSession read-modify-write

**Where:**
- `select`, `select_clone`, `toggle` and `select_next` copy `self.settings.lock().voice.clone()`
  (voice.rs:394, 400, 425, 437) and run `conversion()`, which takes the library lock and stats
  model files.
- `switch_to` then does `self.settings.lock().voice = candidate` (voice.rs:951), replacing the
  whole `VoiceSettings`.
- `reconsider` (voice.rs:787-795) and `relearn` (491-506) have the same shape.

**Scenario:** The user drags a knob (`set_param`, a sync command on the main thread) while pressing
the "next voice" hotkey (an async task). The knob value, or an engine choice from `set_engine`,
written between the clone and the write-back is lost.

**Fix:** Commit under the guard, for example with
`self.settings.lock().voice.select(effect)` after deciding, or an
`update(|voice| …)` helper that re-applies the *choice* (effect, clone_id) to the live settings
instead of overwriting the struct. `Choice` already exists for this.

### APP-10 (low-medium): VoiceSession state is fragmented

**Where:** voice.rs:247-273 has `download`, `calibration`, `carried`, `waiting`, `designer`,
`training` (6 `Mutex`) plus `is_adding_model`, `is_hearing`, `heard_percent`,
`is_microphone_held` (atomics).

Several invariants span them: calibration Listening implies the recorder owner is Calibration and
`carried`/`heard_percent` are meaningful; `waiting` versus calibration. They are kept consistent
only by comments ("Checked again under the lock…", voice.rs:530-532) and by acquisition order.
`view()` takes 7 locks in sequence, and `wants_own_voice` nests library → waiting inside a tail
expression. `TrainingSession` shows the better pattern: one `State` under one mutex.

**Fix:** Group the listening state (calibration, carried, heard_percent) into one struct under one
mutex. Do the same for waiting and designer. This removes most of the "checked again" logic.

### APP-11 (low): the microphone hold is a bool, not an owner set

`hold_microphone(bool)` (voice.rs:510-515) is set by the cloning dialog (`cloning.rs:93`) and by
training takes (`training.rs:257`), and cleared by `cloning.rs:249` (`end`, which runs on every
close, even when cloning never held it) and by training (`training.rs:259, 280, 466, 482`). If
both dialogs are in use, closing one clears the hold. The next `learn()` tick can then start
calibration, take the recorder, and make the other dialog's start fail with "MicrophoneBusy".
The UI may make this hard to reach, but the backend allows it.

**Fix:** Track holders as a counter or bitset keyed by `Recording`.

### APP-12 (low-medium): update progress event spam

`updates.rs:55-64` emits an `AppEvent::Update` (and clones `label`) on every progress callback.
The updater adapter calls it once per downloaded chunk
(`apps/desktop/src-tauri/src/updater.rs:61-65`), which is thousands of identical IPC messages for
an update. Training (`training.rs:223-231`) and the AI pack (`voice-ai/src/pack.rs:317-323`)
deduplicate, so the behaviour is inconsistent across ports.

**Fix:** In `UpdateSession`, store the last percent in an `AtomicU8` and emit only when it
changes. Document "called only when the value changes" on the port, or deduplicate in every
use case.

### APP-13 (low): TOCTOU in `Favorites::toggle`

`add = !self.is_favorite(..)` (favorites.rs:37), then `.await` the download, then lock and
`push` (favorites.rs:43-45). Two quick clicks, or a click from favorites and one from search,
both compute `add = true`, and the favorite is stored twice. The UI then shows duplicates and the
first un-favorite removes both.

**Fix:** Re-check `is_same_sound_as` under the lock before pushing.

### APP-14 (low): TOCTOU in `BoardSession::release_file`

board.rs:297-309 checks the board (one lock), then favorites (another lock), then deletes the
file. If a concurrent `place` of the same path (`add_to_pad` of the same catalog sound) or a
`toggle_favorite` lands between the check and the delete, the file is gone while a pad or
favorite references it. The decoded clip hides this until restart, after which the user sees
"file missing".

**Fix:** Keep one owner of "is this path referenced", for example a small file-reference registry
used by board, favorites and cuts, and do the check and delete under it.

### APP-15 (low): silent failures

- `add_interface` / `configure_interface` drop `BoardError` (board.rs:246, 259). A grid that is
  too small or invalid does nothing and gives no message. `BoardError` already has thiserror
  messages, and `TextKey` has no matching key.
- `add_trained_model` ignores `Ok(false)`, "another model is being added" (voice.rs:682).
- `CutSession::place` returns `Ok(0)` when already placing (cuts.rs:215-217).

**Fix:** Map these to `TextKey`s and emit errors, as `attach_trained_model` already does with
`TrainedModelBusy`.

### APP-16 (low): settings persistence is inconsistent

`SetupSession` saves on every change. `MixerSession::set_*` (mixer.rs:42-63), voice selection and
params (voice.rs:405-408, 951), `set_engine`, and the active interface (`activate`,
board.rs:622-626) only change memory. They reach disk only at the next unrelated save or at
graceful shutdown (`composition.rs:78`), so a crash or forced kill loses them.

**Fix:** Add a dirty flag and a debounced save in `Settings` (for example `Settings::touch()`),
called from every mutation, written by a background writer. This also fixes the I/O-under-lock
part of APP-7 for settings.

### APP-17 (low): calibration polling is O(recording) every 0.5 s

`keep_listening` (voice.rs:561-570) calls `recorder.peek`, which runs
`VoiceRecorder::recorded() -> Vec<f32>`: a full copy of up to `LISTEN_LIMIT_SECONDS` × 48 kHz, or
1.92 M samples (about 7.7 MB). It then runs `voiced()` over all of it, sorting the frame levels,
on every 500 ms tick.

**Fix:** Add `recorded_len()`, or `recorded_since(offset)`, to the port. Run VAD only on what is
new, or only when enough new audio has arrived to change `percent`.

### APP-18 (low): busy flags are not cancel-safe

These flags are set before an `.await` and reset after it:
- `while_loading` (`loading` entry, board.rs:366-379)
- `install_ai` (`download = Some(0)`, voice.rs:450-474)
- `attach` (`is_adding_model`, voice.rs:711-715)
- `CutSession::place` (`is_placing`, cuts.rs:215-219)
- the training `Activity` states

If the future is dropped (cancelled), the flag stays set for the life of the process: the pad
shows "loading" forever, or the AI can never be installed. Tauri runs commands to completion
today, so this is latent, but the API is `pub async`.

**Fix:** Use a small RAII guard (`struct Busy<'a>(&'a AtomicBool)` with `Drop`, or a closure-based
`scopeguard`).

### APP-19 (low): a stop signal can be lost

`TrainingSession::accept` and `train` set `Activity::Installing` / `Training` under the lock,
release it, then call `trainer.install` / `train` (training.rs:218-232, 351-373). The adapter's
first action is `self.stopping.store(false)` (`crates/training/src/lib.rs:576`). A `stop()`
(training.rs:429-437) that lands between the unlock and that store sets the flag, which is then
cleared, and the stop is ignored.

**Fix:** Pass a per-run cancellation token (an `Arc<AtomicBool>` created under the session lock)
in `TrainingRun`, or have the session reset the flag (`trainer.arm()`) while still holding its
lock.

### APP-20 (low): `keep` can leave the dialog stuck in Saving

training.rs:545-547: `let Some(job) = state.job.clone() else { return; };` returns without resetting
`activity` (currently `Saving`). It is unreachable today, because `voice_deleted` and `discard`
refuse to remove the job while it is Saving, but a later refactor would leave the dialog
permanently "busy".

**Fix:** `else { drop(state); return self.finish_saving(None); }`.

### APP-21 (low): unnecessary deep clones

- `select_next` clones the whole `VoiceLibrary`, with every voiceprint and model, on each hotkey
  press only to get ids (voice.rs:435-436). Collecting the ids as owned `String`s from one lock
  would do.
- `delete_clone` clones the whole `trained` map (voice.rs:628) to recover the models that
  `VoiceLibrary::remove` drops. Have `remove` return `(Vec<String>, Vec<TrainedModel>)`.
- `VoiceLibraryStore::change` clones the library for rollback on every change
  (voice_library.rs:30). This is acceptable for a rare path, but a closure that returns "changed"
  could avoid the clone and the deep `!=`.
- `send_to_engine` / `conversion` clone voiceprints on every knob step.

### APP-22 (low): hotkey registration under a lock, and full re-registration

`HotkeyService::apply` / `resume` hold `state` while `register_all` calls `host.clear()` plus
`host.register()` for every binding (hotkeys.rs:86-122). The Tauri global-shortcut plugin
marshals these calls onto the main thread. This is safe today only because every caller is a sync
command already on the main thread. A future async caller, combined with a concurrent sync
`pause_hotkeys` (main thread blocked on `state`), would deadlock.

Separately, every pad removal or hotkey clear (board.rs:526-540) unregisters **all** keys and
registers them again. A key pressed in that window is missed, or reaches the game instead.

**Fix:** Compute the bindings under the lock, drop the lock, then apply them to the host, with a
generation number so that a late apply is skipped. Diff the old and new bindings and only
(un)register what changed.

### APP-23 (low): float typing and NaN claims

`Waveform.peaks` (ports.rs:48), `NowPlaying` (ports.rs:57-64), `SoundView.volume`, `MixerView`,
`ParamView`, `MeterFrame` and `DesignerView` are exported with `#[specta(type = u32)]`, so the TS
types claim integers. The events.rs:14-15 comment says these are never NaN, but nothing enforces
it: adapter-provided levels, raw `board.json` volume and `VoiceSettings.values` loaded from disk
can all be NaN. serde_json serializes NaN as `null`, which the TS type does not allow.

**Fix:** Use a `Finite` newtype that sanitizes at construction (NaN→0) with
`#[specta(type = f64)]`-style `number` typing, or validate at the boundary.

### APP-24 (low): stringly error type

`Failure(pub Message)` with `#[error("{0:?}")]` (message.rs:142-143). Every port error is
user-facing text, so use cases cannot branch on the kind of failure. `attach_trained_model`
re-derives "deleted" versus "busy" by re-reading state (voice.rs:695-701). `Display` emits `Debug`
output, which is unhelpful in logs.

**Fix:** Acceptable for a UI-message architecture, but consider `Failure { kind: FailureKind, message: Message }`
for the few kinds that drive logic (busy, missing, timed out), and a readable `Display`.

### APP-25 (low): small quality items

- `BoardSession::sound_on_pad` is an alias of `sound_in_active` (board.rs:360-363).
  `VoiceSession::refresh` is just `publish` (voice.rs:889-891).
- `FULL_VOLUME` is defined twice (cuts.rs:21, rows.rs:12).
- `Settings::lock` (settings.rs:79) and `VoiceLibraryStore::lock` (voice_library.rs:22) hand out
  `MutexGuard`s that allow mutation, so they bypass `change()`'s "save or roll back" guarantee
  (voice_library.rs:26-27). All current uses of the library guard only read, but the API invites
  misuse. Return a read-only guard (`impl Deref<Target = VoiceLibrary>`) or `with(|l| …)`.
- `routing::{choose, real_devices, virtual_devices}`, `hotkeys::resolve`/`Binding` and
  `board::pad_label` are `pub` only for tests or one internal caller. `pub(crate)` with
  `#[cfg(test)]` re-exports would keep the surface small. Low priority.
- The orchestration "delete clone, then stop its training" lives in the Tauri command
  (`commands/voice.rs` `delete_cloned_voice`) instead of in a use case. This is business logic in
  the adapter layer.

### APP-26 (low): routing LCD noise

`SetupSession::publish` (setup.rs:350-354) also emits `DisplayEvent::Routing` on every settings
change (quiet updates, language, one-at-a-time…). This overwrites whatever the LCD showed with the
routing status. Emit Routing only from routing changes.

### APP-27 (low): orphaned files in the sound store

- `place` returns early on "does not fit" (board.rs:401-403), and preload can fail after
  `files.import` (board.rs:442-447). Either way the copied or downloaded file is never released.
- `CutSession::try_open` with a placing in progress does not release the previous editing file
  (cuts.rs:160-164). If two opens happen during one placing, the middle file is leaked.

**Fix:** Call `release_file(path)` on those error and early-return paths.

### DOM-1 (medium): `PadBoard::swap` allows a target slot outside the grid

`domain/board.rs:260-275` only checks `from_slot` has a sound. `swap_pads(0, 9999)` (IPC) sets
`sound.slot = 9999`. After that:
- `pads()` iterates `0..capacity`, so the sound disappears from the UI.
- It still has its hotkey, which still plays.
- It counts toward `GridTooSmall` in `reconfigure` (it is in `sounds_of`), so the grid cannot
  shrink without an obvious reason.
- Its file is never released.

This domain invariant ("slot < capacity") is enforced in `place`'s caller but not in the domain.

**Fix:** In `swap`, look up the interface and require `to_slot < capacity`, returning `Result` or
`bool`. Ideally enforce `slot < capacity` in every domain mutator.

### DOM-2 (low): `i32` overflow on IPC offsets

`can_move_interface`: `from as i32 + offset` (domain/board.rs:206). `neighbor`:
`(index + offset).rem_euclid(count)` (domain/board.rs:227). `offset` comes straight from IPC
(`step_interface`, `move_interface`). With `offset = i32::MAX` a debug build panics, under the
board lock, which poisons it (APP-8). Release builds wrap and happen to produce a valid index.

**Fix:** Use `i64` arithmetic or `checked_add`. For `neighbor`, `offset.rem_euclid(count)` first.

### DOM-3 (medium-low): domain invariants can be bypassed through pub fields

- `Sound` has all-`pub` fields (domain/board.rs:75-88). The application's `set_color`
  (app board.rs:496-498) writes `sound.color` directly, skipping the `palette::visible_color` rule
  that `place` applies ("A nearly black pad would look switched off"). `volume` has no bounds
  (APP-4). `slot` can be set to anything.
- `Cut` documents "It always lasts at least MIN_CUT_MS" (domain/cuts.rs:18-19). Its fields are
  `pub` and it derives `Deserialize`, so a `Cut { start_ms: 900, end_ms: 100 }` from `board.json`
  makes `duration_ms()` (`end_ms - start_ms`, cuts.rs:36-38) panic in debug or wrap in release.
  The audio adapter also builds `Cut { .. }` literals (`crates/audio/src/clip.rs:71-74`).

**Fix:** Make the fields private and provide constructors and setters. Use
`#[serde(try_from = "RawCut")]` so deserialization goes through `Cut::within`. Use
`saturating_sub` in `duration_ms` as a belt-and-braces measure.

### DOM-4 (low): persisted data is not validated on load

- `PadBoard` from disk is not checked: `rows * columns` overflow (domain/board.rs:45-46), sounds
  pointing at missing interfaces, duplicate `(interface, slot)` pairs.
- `VoiceSettings::get` returns the raw stored value (voice.rs:248-253). `set` clamps, but a
  hand-edited `settings.json` with `"values": {"thinIntensity": 50}` gives a +352-semitone chain.
- `TrainingJob.id` and `Take.id` (domain/training.rs:221-232, 201-207) become folder and file
  names in the trainer (`discard`, `remove_take`) with no format check. A tampered `training.json`
  with `"id": "../.."` turns `discard` into a recursive delete outside the training home.

**Fix:** Use `try_from` validation on deserialization, clamp in `get`, and validate ids with
`^voice-[0-9a-f]{32}$` / `^take-[0-9a-f]{32}$`.

### DOM-5 (low): `engines` grows with arbitrary keys

`VoiceSettings::set_engine` (domain/voice.rs:271-273) inserts any key. The application's
`set_engine` (app voice.rs:776-782) builds `VoiceTarget::Own(clone_id)` from IPC without checking
that the voice exists. `forget_clone` only removes ids that existed, so junk keys persist in
settings forever.

**Fix:** Check `library.takes_trained_model(&target)` before setting.

### DOM-6 (low): pub API that only tests use

`cuts::pieces` (domain/cuts.rs:56) and `Voiceprint::similarity` (domain/cloning.rs:40) are used
only by tests. The frontend presumably computes pieces itself, and it then sends `CutPiece`s.
Either use `pieces` in `CutSession::place`, with the backend splitting at cues, which would also
remove trust in client-computed ranges, or drop it.

### DOM-7 (low): allocation in a hot comparison

`SoundSource::is_same_sound_as` does `self.path.to_lowercase() == path.to_lowercase()`
(domain/sounds.rs:21-23), which allocates 2 Strings per comparison. `BoardView` calls it for every
pad × every favorite, under board + settings + favorites locks, on every board publish.

**Fix:** Compare without allocating (`a.chars().flat_map(char::to_lowercase).eq(b.chars().flat_map(char::to_lowercase))`),
or store a normalized key once.

### DOM-8 (low): no limits on names

`InterfaceLayout.name` can be empty or any length, and `accent` is not validated
(domain/board.rs:37-53). Clone and design names are trimmed but have no maximum
(cloning.rs:201-229, 274-284). Pad names via `place` are not trimmed (only `rename` trims).
These are minor, but the UI and the LCD draw them.

**Fix:** Add a `MAX_NAME_CHARS` and trim in one domain helper.

### DOM-9 (low): the locale region is not checked against the catalog list

`trending_region` (domain/trending.rs:26-33) validates the *chosen* region but not the one derived
from the system locale. `en-XX` gives `xx`, so the catalog returns 404 and the trending list is
empty instead of falling back to `us`.

**Fix:** Apply `.filter(|r| is_trending_region(r))` to the locale branch too.

---

## Things checked and found sound

- **No std `MutexGuard` is held across an `.await`.** I checked every `.await` in board, cuts,
  rows, favorites, voice, cloning, training, setup and updates. Guards are dropped in inner blocks
  or as statement temporaries before the await. The one lock held across awaits is the
  intentional `tokio::sync::Mutex<Recovery>` in `SetupSession`, which serializes routing changes.
- **Lock ordering is consistent and I found no inversions.** The orders are board → settings →
  favorites; settings → library → (download, waiting, designer, calibration, voice.training);
  training.state → library / voice locks; cloning.state → library and recorder owner. The
  recorder owner lock is a leaf.
- **Check-and-set is done atomically under one lock** where it matters: `install_ai`
  (voice.rs:451-458), `TrainingSession::train` / `accept` (activity transitions), `CloningSession::listen`,
  `SharedRecorder::start` / `stop`.
- **Stale async results are dropped well:** the `generation` counter in cloning, `Editing.id` in
  cuts, `PadTarget` carrying `interface_id` across awaits and re-checking that the slot fits, and
  `keep` comparing `job.id`.
- **`VoiceLibraryStore::change`** saves or rolls back under one lock, so the in-memory library is
  never ahead of disk.
- **`recovery.rs`** is a pure, tick-driven state machine with capped exponential backoff, a
  "steady run forgives failures" rule and good unit tests.
- **Resource caps exist in the adapters** (the recording cap `MAX_RECORDED`, decode truncation),
  and training deduplicates install progress. These caps are not written into the port contracts
  (`VoiceRecorder::recorded` / `decode_file`); state them there.
- **The domain is genuinely pure:** no I/O, `saturating_*` in training math, `Cut::within`
  clamping, NaN-safe sorting with `total_cmp` in `voiced`, and `onsets` guarding `frame_ms == 0`.
- **`async_trait` is justified:** the ports are used as `Arc<dyn …>`, and native async-fn-in-trait
  is not dyn-compatible.
- **There is very little AI slop.** Comments explain *why* rather than *what*, traits exist only
  for real ports, and there are almost no speculative abstractions. The main quality debt is the
  size and lock fragmentation of `VoiceSession` (APP-10) and the trust placed in IPC-provided
  values (APP-1, -2, -4, -5, DOM-1).
