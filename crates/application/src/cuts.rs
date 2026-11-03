//! The cut editor: a sound drawn so the user can mark cue points on it, the way the hot cues of
//! a DJ player split a track, and the pieces between them put on pads of their own. The file
//! is never changed: each pad keeps the piece it plays.

use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::cuts::{self, Cut};
use instantpad_domain::sounds::SoundSource;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::board::{BoardSession, file_stem};
use crate::events::AppEvent;
use crate::library::SoundLibrary;
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, PlaybackTag, SoundFileStore, SoundPlayer, Waveform};

/// The levels the detection of the sounds reads, one per this many milliseconds.
const LEVEL_FRAME_MS: u32 = 10;
const FULL_VOLUME: f32 = 1.0;

/// What the user hears from the editor, apart from any pad.
fn preview_tag() -> PlaybackTag {
    PlaybackTag::of("cut editor")
}

/// Where the sound to cut comes from.
#[derive(Clone, Debug, PartialEq, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CutSource {
    /// A sound of MyInstants, which is downloaded first.
    Catalog { source: SoundSource },
    /// A file on the computer, which is copied into the app first, like any imported sound.
    File { path: String },
    /// The sound on a pad of the interface on screen.
    Pad { slot: u32 },
}

/// What the editor opens with.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CutEditorView {
    pub name: String,
    pub color: String,
    pub duration_ms: u32,
    /// The cue points to start from: the start of the sound, or the cut of the pad.
    pub cues: Vec<u32>,
    /// How many pads of the interface on screen have room for a piece.
    pub free_pads: u32,
}

/// A piece the user wants on a pad.
#[derive(Clone, Debug, PartialEq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CutPiece {
    pub name: String,
    pub color: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

/// The sound open in the editor.
#[derive(Clone)]
struct Editing {
    path: String,
    duration_ms: u32,
}

pub struct CutSession {
    board: Arc<BoardSession>,
    library: Arc<SoundLibrary>,
    files: Arc<dyn SoundFileStore>,
    player: Arc<dyn SoundPlayer>,
    events: Arc<dyn EventSink>,
    editing: Mutex<Option<Editing>>,
}

impl CutSession {
    pub fn new(
        board: Arc<BoardSession>,
        library: Arc<SoundLibrary>,
        files: Arc<dyn SoundFileStore>,
        player: Arc<dyn SoundPlayer>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            board,
            library,
            files,
            player,
            events,
            editing: Mutex::new(None),
        }
    }

    /// Opens a sound in the editor, closing the one that was open. A sound that is not on the
    /// computer yet is downloaded, and a file of the computer is copied into the app.
    pub async fn open(&self, source: CutSource) -> Result<CutEditorView, Failure> {
        self.close();
        let (path, name, color, cut) = match source {
            CutSource::Catalog { source } => {
                self.library.ensure_downloaded(&source).await?;
                (source.path, source.name, source.color, None)
            }
            CutSource::File { path } => {
                let stored = self.files.import(&path)?;
                (stored, file_stem(&path), String::new(), None)
            }
            CutSource::Pad { slot } => {
                let sound = self
                    .board
                    .sound_on_pad(slot)
                    .ok_or(Failure::text(TextKey::SoundFileMissing))?;
                (sound.path, sound.name, sound.color, sound.cut)
            }
        };

        let duration_ms = match self.player.waveform(&path, 0, 0, 0).await {
            Ok(drawn) => drawn.duration_ms,
            Err(failure) => {
                // Nothing keeps a file that cannot be opened.
                self.board.release_file(&path);
                return Err(failure);
            }
        };
        let cues = match cut {
            Some(cut) if cut.end_ms < duration_ms => vec![cut.start_ms, cut.end_ms],
            Some(cut) => vec![cut.start_ms],
            None => vec![0],
        };
        *self.editing() = Some(Editing { path, duration_ms });
        Ok(CutEditorView {
            name,
            color,
            duration_ms,
            cues,
            free_pads: self.board.free_pad_count(),
        })
    }

    /// How loud each of `bars` stretches from `start_ms` to `end_ms` of the open sound gets.
    pub async fn waveform(
        &self,
        start_ms: u32,
        end_ms: u32,
        bars: u32,
    ) -> Result<Waveform, Failure> {
        let editing = self.open_sound()?;
        self.player
            .waveform(&editing.path, start_ms, end_ms, bars)
            .await
    }

    /// Cue points at the start of each sound of the open recording, found by its pauses.
    pub async fn detect(&self) -> Result<Vec<u32>, Failure> {
        let editing = self.open_sound()?;
        let levels = self.player.levels(&editing.path, LEVEL_FRAME_MS).await?;
        Ok(cuts::onsets(&levels, LEVEL_FRAME_MS))
    }

    /// Plays the piece from `start_ms` to `end_ms` of the open sound, only for the user.
    pub async fn preview(&self, start_ms: u32, end_ms: u32) -> Result<(), Failure> {
        let editing = self.open_sound()?;
        let Some(cut) = Cut::within(start_ms, end_ms, editing.duration_ms) else {
            self.stop_preview();
            return Ok(());
        };
        self.player
            .preview(&editing.path, FULL_VOLUME, preview_tag(), Some(cut))
            .await
    }

    pub fn stop_preview(&self) {
        self.player.stop(preview_tag());
    }

    /// Puts each piece on a free pad of the interface on screen, in order, and closes the
    /// editor. Pads with a sound are never replaced: when the free pads run out, or a piece
    /// cannot be placed, the rest are not, and the user is told. Returns how many were placed.
    pub async fn place(&self, pieces: Vec<CutPiece>) -> Result<u32, Failure> {
        let editing = self.open_sound()?;
        self.stop_preview();
        let mut placed = 0;
        for piece in pieces {
            let Some(cut) = Cut::within(piece.start_ms, piece.end_ms, editing.duration_ms) else {
                continue;
            };
            let Some(target) = self.board.free_pad() else {
                self.events
                    .emit(AppEvent::error(Message::text(TextKey::InterfaceFullHint)));
                break;
            };
            let name = piece.name.trim();
            let name = if name.is_empty() { "-" } else { name };
            let work = self
                .board
                .place(&target, name, &editing.path, &piece.color, Some(cut));
            // The pieces already placed stay, and placing again would put them twice.
            if let Err(Failure(reason)) = self.board.while_loading(&target, work).await {
                self.events.emit(AppEvent::error(reason));
                break;
            }
            placed += 1;
        }
        *self.editing() = None;
        if placed > 0 {
            self.events.emit(AppEvent::show(Message::with(
                TextKey::CutsPlaced,
                [Message::plain(placed.to_string())],
            )));
        }
        // A file nothing took is not kept, such as a download that was only cut.
        self.board.release_file(&editing.path);
        Ok(placed)
    }

    /// Closes the editor without placing anything. A file that came for the editor alone, a
    /// download or a copy of a file of the computer, goes with it.
    pub fn close(&self) {
        let Some(editing) = self.editing().take() else {
            return;
        };
        self.stop_preview();
        self.board.release_file(&editing.path);
    }

    fn open_sound(&self) -> Result<Editing, Failure> {
        self.editing()
            .clone()
            .ok_or(Failure::text(TextKey::SoundFileMissing))
    }

    fn editing(&self) -> MutexGuard<'_, Option<Editing>> {
        self.editing.lock().expect("cut editor lock poisoned")
    }
}
