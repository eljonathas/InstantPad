# Architecture

Dependencies only point this way:

```
domain ◄── application ◄── audio, catalog, storage, platform ◄── apps/desktop/src-tauri
```

- `domain` holds the pad board and hotkey rules, with no I/O and no async.
- `application` holds the use cases and the ports in `ports.rs`. All app state lives here, so global hotkeys work without the webview.
- The other crates implement the ports. Only `src-tauri/src/composition.rs` chooses which implementation goes behind each port.
- The React interface draws what the events and `StartView` send, and calls commands. It only knows `src/shared/ipc`.

`crates/test-support/tests/dependency_direction.rs` fails if `domain` or `application` reach `tauri`, `cpal`, `reqwest` or `windows`.

To add a platform, implement `SystemAudio` and `VirtualCable` in `crates/platform` and pick them in `composition.rs`.
