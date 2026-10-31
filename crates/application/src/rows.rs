use std::sync::Arc;

use instantpad_domain::sounds::SoundSource;

use crate::board::BoardSession;
use crate::events::AppEvent;
use crate::favorites::Favorites;
use crate::library::SoundLibrary;
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, PlaybackTag, SoundPlayer};

const FULL_VOLUME: f32 = 1.0;

/// What the buttons of a sound row do, shared by the search results and the favorites.
pub struct SoundRows {
    board: Arc<BoardSession>,
    library: Arc<SoundLibrary>,
    favorites: Arc<Favorites>,
    player: Arc<dyn SoundPlayer>,
    events: Arc<dyn EventSink>,
}

impl SoundRows {
    pub fn new(
        board: Arc<BoardSession>,
        library: Arc<SoundLibrary>,
        favorites: Arc<Favorites>,
        player: Arc<dyn SoundPlayer>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            board,
            library,
            favorites,
            player,
            events,
        }
    }

    pub async fn preview(&self, source: SoundSource) {
        let result = async {
            self.downloading(&source).await?;
            let tag = PlaybackTag::of(&source.path);
            self.player.preview(&source.path, FULL_VOLUME, tag).await
        };
        self.report(&source, result.await, TextKey::Listening);
    }

    pub async fn play_in_game(&self, source: SoundSource) {
        let result = async {
            self.downloading(&source).await?;
            let tag = PlaybackTag::of(&source.path);
            self.player.play(&source.path, FULL_VOLUME, tag).await
        };
        self.report(&source, result.await, TextKey::InGameSound);
    }

    /// Without a `slot`, the sound goes to the pad the user picked or to the first free one.
    pub async fn add_to_pad(&self, source: SoundSource, slot: Option<u32>) {
        let Some(slot) = slot.or_else(|| self.board.pad_for_new_sound()) else {
            let hint = Message::text(TextKey::InterfaceFullHint);
            self.events.emit(AppEvent::error(hint));
            return;
        };

        let result = async {
            self.downloading(&source).await?;
            self.board
                .place(slot, &source.name, &source.path, &source.color)
                .await
        };
        if let Err(failure) = result.await {
            self.show_failure(&source, failure);
        }
    }

    pub async fn toggle_favorite(&self, source: SoundSource) {
        self.show_downloading(&source);
        match self.favorites.toggle(&source).await {
            Ok(is_favorite) => {
                let mark = if is_favorite { "★ " } else { "" };
                self.show(Message::plain(format!("{mark}{}", source.name)));
                self.events.emit(AppEvent::Favorites(self.favorites.list()));
                self.board.publish();
            }
            Err(failure) => self.show_failure(&source, failure),
        }
    }

    pub async fn save_to_computer(&self, source: SoundSource, destination: &str) {
        self.show_downloading(&source);
        match self.library.save_to_computer(&source, destination).await {
            Ok(()) => {
                let file = destination
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(destination);
                self.show(Message::with(TextKey::Saved, [Message::plain(file)]));
            }
            Err(failure) => self.show_failure(&source, failure),
        }
    }

    async fn downloading(&self, source: &SoundSource) -> Result<(), Failure> {
        self.show_downloading(source);
        self.library.ensure_downloaded(source).await
    }

    fn show_downloading(&self, source: &SoundSource) {
        self.show(Message::with(
            TextKey::Downloading,
            [Message::plain(source.name.clone())],
        ));
    }

    fn report(&self, source: &SoundSource, result: Result<(), Failure>, success: TextKey) {
        match result {
            Ok(()) => self.show(Message::with(
                success,
                [Message::plain(source.name.clone())],
            )),
            Err(failure) => self.show_failure(source, failure),
        }
    }

    fn show_failure(&self, source: &SoundSource, Failure(reason): Failure) {
        self.events.emit(AppEvent::error(Message::with(
            TextKey::FailedSound,
            [Message::plain(source.name.clone()), reason],
        )));
    }

    fn show(&self, message: Message) {
        self.events.emit(AppEvent::show(message));
    }
}
