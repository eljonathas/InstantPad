# 0001: State lives in Rust

The WPF app kept its board, active interface and playback rules in view models. A global hotkey has to play a pad while a game has focus and the window may be minimized, and a browser engine can throttle a page in that state.

So all of that moved to `crates/application`. The interface draws snapshots and sends intents, and a hotkey never passes through the webview. Translations stay in the frontend, so the backend sends a `Message` with a key and arguments, and text follows a language change without a restart.
