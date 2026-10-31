//! System-wide keys, so sounds fire while a game has focus.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use instantpad_application::hotkeys::HotkeyCommand;
use instantpad_application::ports::HotkeyHost;
use instantpad_domain::hotkeys::Hotkey;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{
    Builder, Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::composition::Core;

pub struct GlobalHotkeys {
    app: AppHandle,
    commands: Mutex<HashMap<u32, HotkeyCommand>>,
}

impl GlobalHotkeys {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            commands: Mutex::default(),
        }
    }

    fn command_for(&self, shortcut: &Shortcut) -> Option<HotkeyCommand> {
        let commands = self.commands.lock().expect("hotkey lock poisoned");
        commands.get(&shortcut.id()).cloned()
    }
}

impl HotkeyHost for GlobalHotkeys {
    fn register(&self, hotkey: &Hotkey, command: HotkeyCommand) -> bool {
        let Some(shortcut) = to_shortcut(hotkey) else {
            return false;
        };
        let registered = self.app.global_shortcut().register(shortcut).is_ok();
        if registered {
            let mut commands = self.commands.lock().expect("hotkey lock poisoned");
            commands.insert(shortcut.id(), command);
        }
        registered
    }

    fn clear(&self) {
        let _ = self.app.global_shortcut().unregister_all();
        self.commands.lock().expect("hotkey lock poisoned").clear();
    }
}

fn to_shortcut(hotkey: &Hotkey) -> Option<Shortcut> {
    let code = Code::from_str(&hotkey.key).ok()?;
    let held = hotkey.modifiers;
    let mut modifiers = Modifiers::empty();
    modifiers.set(Modifiers::CONTROL, held.ctrl);
    modifiers.set(Modifiers::ALT, held.alt);
    modifiers.set(Modifiers::SHIFT, held.shift);
    modifiers.set(Modifiers::META, held.meta);
    Some(Shortcut::new(Some(modifiers), code))
}

/// The plugin, wired to play what each pressed key asks for.
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let Some(command) = app
                .try_state::<Arc<GlobalHotkeys>>()
                .and_then(|hotkeys| hotkeys.command_for(shortcut))
            else {
                return;
            };

            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(core) = app.try_state::<Core>() {
                    core.board.on_hotkey(command).await;
                }
            });
        })
        .build()
}
