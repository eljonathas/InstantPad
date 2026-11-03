use instantpad_application::cuts::{CutEditorView, CutPiece, CutSource};
use instantpad_application::message::Failure;
use instantpad_application::ports::Waveform;
use tauri::State;

use crate::composition::Core;

#[tauri::command]
#[specta::specta]
pub async fn open_cut_editor(
    core: State<'_, Core>,
    source: CutSource,
) -> Result<CutEditorView, Failure> {
    core.cuts.open(source).await
}

#[tauri::command]
#[specta::specta]
pub async fn cut_waveform(
    core: State<'_, Core>,
    start_ms: u32,
    end_ms: u32,
    bars: u32,
) -> Result<Waveform, Failure> {
    core.cuts.waveform(start_ms, end_ms, bars).await
}

#[tauri::command]
#[specta::specta]
pub async fn detect_cues(core: State<'_, Core>) -> Result<Vec<u32>, Failure> {
    core.cuts.detect().await
}

#[tauri::command]
#[specta::specta]
pub async fn preview_cut(core: State<'_, Core>, start_ms: u32, end_ms: u32) -> Result<(), Failure> {
    core.cuts.preview(start_ms, end_ms).await
}

#[tauri::command]
#[specta::specta]
pub fn stop_cut_preview(core: State<'_, Core>) {
    core.cuts.stop_preview();
}

#[tauri::command]
#[specta::specta]
pub async fn place_cuts(core: State<'_, Core>, pieces: Vec<CutPiece>) -> Result<u32, Failure> {
    core.cuts.place(pieces).await
}

#[tauri::command]
#[specta::specta]
pub fn close_cut_editor(core: State<'_, Core>) {
    core.cuts.close();
}
