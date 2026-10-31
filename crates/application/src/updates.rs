use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::events::{AppEvent, UpdateView};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{AppUpdater, EventSink};

/// Updates in the background. The new version never interrupts a match: it takes over when
/// the user asks for a restart or closes the app.
pub struct UpdateSession {
    updater: Arc<dyn AppUpdater>,
    events: Arc<dyn EventSink>,
    is_ready: AtomicBool,
}

impl UpdateSession {
    pub fn new(updater: Arc<dyn AppUpdater>, events: Arc<dyn EventSink>) -> Self {
        Self {
            updater,
            events,
            is_ready: AtomicBool::new(false),
        }
    }

    pub async fn check_and_download(&self) {
        if !self.updater.is_release_build() {
            self.publish(Message::text(TextKey::DevelopmentVersion));
            return;
        }

        let current = Message::plain(self.updater.current_version());
        self.publish(Message::with(TextKey::Version, [current]));

        if let Err(Failure(reason)) = self.download_update().await {
            self.publish(reason);
        }
    }

    pub fn restart_now(&self) {
        self.apply(true);
    }

    /// Closing the app with a version ready installs it, without opening the app again.
    pub fn apply_on_exit(&self) {
        self.apply(false);
    }

    async fn download_update(&self) -> Result<(), Failure> {
        let Some(version) = self.updater.find_update().await? else {
            return Ok(());
        };

        let label = version.clone();
        let events = self.events.clone();
        let progress = move |percent: u8| {
            let args = [
                Message::plain(label.clone()),
                Message::plain(percent.to_string()),
            ];
            events.emit(AppEvent::Update(UpdateView {
                status: Message::with(TextKey::DownloadingVersion, args),
                is_ready_to_restart: false,
            }));
        };
        self.updater.download(&progress).await?;

        self.is_ready.store(true, Ordering::Relaxed);
        self.publish(Message::with(
            TextKey::VersionInstalled,
            [Message::plain(version)],
        ));
        Ok(())
    }

    fn apply(&self, restart: bool) {
        if !self.is_ready.load(Ordering::Relaxed) {
            return;
        }
        if let Err(Failure(reason)) = self.updater.apply(restart) {
            self.publish(reason);
        }
    }

    fn publish(&self, status: Message) {
        self.events.emit(AppEvent::Update(UpdateView {
            status,
            is_ready_to_restart: self.is_ready.load(Ordering::Relaxed),
        }));
    }
}
