//! Events the interface listens to.

use instantpad_application::events::{AppEvent, MeterFrame};
use instantpad_application::ports::EventSink;
use serde::Serialize;
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

#[derive(Clone, Serialize, Type, Event)]
pub struct BackendEvent(pub AppEvent);

/// Levels and lit pads, about 30 times per second.
#[derive(Clone, Serialize, Type, Event)]
pub struct MeterEvent(pub MeterFrame);

pub struct TauriEvents(pub AppHandle);

impl EventSink for TauriEvents {
    fn emit(&self, event: AppEvent) {
        // Only fails while the window is closing, when nobody is listening.
        let _ = BackendEvent(event).emit(&self.0);
    }
}
