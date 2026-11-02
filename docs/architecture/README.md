# Architecture

Dependencies only point this way:

```
domain ◄── application ◄── audio, catalog, storage, platform, voice-ai ◄── apps/desktop/src-tauri
```

- `domain` holds the pad board, hotkey, voice and voice cloning rules, with no I/O and no async.
- `application` holds the use cases and the ports in `ports.rs`. All app state lives here, so global hotkeys work without the webview.
- The other crates implement the ports. Only `src-tauri/src/composition.rs` chooses which implementation goes behind each port.
- The React interface draws what the events and `StartView` send, and calls commands. It only knows `src/shared/ipc`.

`crates/test-support/tests/dependency_direction.rs` fails if `domain` or `application` reach `tauri`, `cpal`, `reqwest`, `windows` or `ort`.

`voice-ai` runs the AI voices with ONNX Runtime. The audio engine only sees it through the `SpeechConverter` port, on a thread of its own, so the audio callbacks never wait for a model. [ADR 0003](../adr/0003-local-ai-voices.md) explains why the models run locally.

To add a platform, implement `SystemAudio` and `VirtualCable` in `crates/platform` and pick them in `composition.rs`.
