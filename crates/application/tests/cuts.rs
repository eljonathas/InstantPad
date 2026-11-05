mod support;

use std::sync::Arc;

use instantpad_application::cuts::{CutPiece, CutSession, CutSource};
use instantpad_application::events::AppEvent;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::SoundFileStore;
use instantpad_domain::cuts::Cut;
use instantpad_domain::sounds::SoundSource;
use support::World;

struct Editor {
    world: World,
    cuts: CutSession,
}

impl Editor {
    fn new() -> Self {
        let world = World::new();
        let cuts = CutSession::new(
            world.board.clone(),
            world.library.clone(),
            world.files.clone(),
            world.player.clone(),
            world.events.clone(),
        );
        Self { world, cuts }
    }

    /// Opens a file of the computer that lasts `duration_ms`.
    async fn opens_a_file(&self, name: &str, duration_ms: u32) {
        let stored = format!("store/{name}.mp3");
        self.world
            .player
            .durations
            .lock()
            .unwrap()
            .insert(stored, duration_ms);
        self.cuts
            .open(CutSource::File {
                path: format!("/home/me/{name}.mp3"),
            })
            .await
            .unwrap();
    }

    fn pad(&self, slot: u32) -> Option<(String, String, Option<Cut>)> {
        let sound = self.world.board.sound_on_pad(slot)?;
        Some((sound.name, sound.path, sound.cut))
    }

    fn saw(&self, event: &AppEvent) -> bool {
        self.world.events.0.lock().unwrap().contains(event)
    }
}

fn piece(name: &str, start_ms: u32, end_ms: u32) -> CutPiece {
    CutPiece {
        name: name.to_owned(),
        color: "#ff8800".to_owned(),
        start_ms,
        end_ms,
    }
}

fn cut(start_ms: u32, end_ms: u32) -> Option<Cut> {
    Some(Cut { start_ms, end_ms })
}

fn missing() -> Failure {
    Failure::text(TextKey::SoundFileMissing)
}

#[tokio::test]
async fn a_sound_of_myinstants_is_downloaded_and_opens_from_its_start() {
    let editor = Editor::new();
    let source = SoundSource {
        name: "Bruh".to_owned(),
        color: "#ff0000".to_owned(),
        path: "store/bruh.mp3".to_owned(),
        sound_url: Some("https://site/bruh.mp3".to_owned()),
    };

    let view = editor
        .cuts
        .open(CutSource::Catalog { source })
        .await
        .unwrap();

    assert_eq!(
        *editor.world.catalog.downloaded.lock().unwrap(),
        ["https://site/bruh.mp3"]
    );
    assert_eq!(view.name, "Bruh");
    assert_eq!(view.color, "#ff0000");
    assert_eq!(view.duration_ms, 10_000);
    assert_eq!(view.cues, [0]);
    assert_eq!(view.free_pads, 24);
}

#[tokio::test]
async fn a_file_of_the_computer_is_copied_into_the_app_and_named_after_it() {
    let editor = Editor::new();

    let view = editor
        .cuts
        .open(CutSource::File {
            path: "/home/me/memes.mp3".to_owned(),
        })
        .await
        .unwrap();

    assert!(editor.world.files.exists("store/memes.mp3"));
    assert_eq!(view.name, "memes");
}

#[tokio::test]
async fn a_pad_opens_with_the_cue_points_of_its_cut() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;
    editor
        .cuts
        .place(vec![piece("Memes 1", 1000, 2500)])
        .await
        .unwrap();

    let view = editor.cuts.open(CutSource::Pad { slot: 0 }).await.unwrap();

    assert_eq!(view.name, "Memes 1");
    assert_eq!(view.cues, [1000, 2500]);
    assert_eq!(view.free_pads, 23);
}

#[tokio::test]
async fn an_empty_pad_has_nothing_to_open() {
    let editor = Editor::new();

    let failure = editor.cuts.open(CutSource::Pad { slot: 5 }).await;

    assert_eq!(failure, Err(missing()));
}

#[tokio::test]
async fn a_file_that_cannot_be_opened_is_not_kept() {
    let editor = Editor::new();
    editor
        .world
        .player
        .broken
        .lock()
        .unwrap()
        .insert("store/broken.mp3".to_owned());

    let failure = editor
        .cuts
        .open(CutSource::File {
            path: "/home/me/broken.mp3".to_owned(),
        })
        .await;

    assert_eq!(failure, Err(Failure::text(TextKey::SoundHasNoAudio)));
    assert!(!editor.world.files.exists("store/broken.mp3"));
    assert!(editor.saw(&AppEvent::error(Message::text(TextKey::SoundHasNoAudio))));
}

#[tokio::test]
async fn nothing_is_drawn_heard_detected_or_placed_with_no_sound_open() {
    let editor = Editor::new();

    assert_eq!(editor.cuts.waveform(0, 1000, 10).await, Err(missing()));
    assert_eq!(editor.cuts.detect().await, Err(missing()));
    assert_eq!(editor.cuts.preview(0, 1000).await, Err(missing()));
    assert_eq!(
        editor.cuts.place(vec![piece("a", 0, 1000)]).await,
        Err(missing())
    );
}

#[tokio::test]
async fn the_open_sound_is_drawn_with_as_many_bars_as_asked() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;

    let drawn = editor.cuts.waveform(0, 4000, 64).await.unwrap();

    assert_eq!(drawn.peaks.len(), 64);
    assert_eq!(drawn.duration_ms, 4000);
}

#[tokio::test]
async fn detecting_puts_a_cue_at_the_start_of_each_sound() {
    let editor = Editor::new();
    let pattern = [(0.5, 30), (0.0, 40), (0.5, 20)];
    let levels: Vec<f32> = pattern
        .iter()
        .flat_map(|&(level, frames)| std::iter::repeat_n(level, frames))
        .collect();
    editor
        .world
        .player
        .levels
        .lock()
        .unwrap()
        .insert("store/memes.mp3".to_owned(), levels);
    editor.opens_a_file("memes", 900).await;

    assert_eq!(editor.cuts.detect().await.unwrap(), [0, 690]);
}

#[tokio::test]
async fn a_piece_is_heard_alone_and_only_by_the_user() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;

    editor.cuts.preview(1000, 2000).await.unwrap();

    assert_eq!(
        *editor.world.player.previewed.lock().unwrap(),
        ["store/memes.mp3"]
    );
    assert!(editor.world.player.played.lock().unwrap().is_empty());
    assert_eq!(*editor.world.player.cuts.lock().unwrap(), [cut(1000, 2000)]);
}

#[tokio::test]
async fn a_piece_too_short_to_hear_stops_what_was_playing() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;
    editor.cuts.preview(1000, 2000).await.unwrap();

    editor.cuts.preview(1000, 1010).await.unwrap();

    assert_eq!(editor.world.player.previewed.lock().unwrap().len(), 1);
    assert!(editor.world.player.playing.lock().unwrap().is_empty());
}

#[tokio::test]
async fn each_piece_goes_to_the_next_free_pad_with_its_cut_and_the_editor_closes() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;

    let placed = editor
        .cuts
        .place(vec![piece("Bruh", 0, 1000), piece("Boom", 1000, 4000)])
        .await
        .unwrap();

    assert_eq!(placed, 2);
    let path = "store/memes.mp3".to_owned();
    assert_eq!(
        editor.pad(0),
        Some(("Bruh".to_owned(), path.clone(), cut(0, 1000)))
    );
    assert_eq!(
        editor.pad(1),
        Some(("Boom".to_owned(), path, cut(1000, 4000)))
    );
    assert!(editor.saw(&AppEvent::show(Message::with(
        TextKey::CutsPlaced,
        [Message::plain("2")]
    ))));
    // Both pads share the one file, which stays.
    assert!(editor.world.files.exists("store/memes.mp3"));
    assert_eq!(editor.cuts.preview(0, 1000).await, Err(missing()));
}

#[tokio::test]
async fn pads_with_a_sound_are_never_replaced() {
    let editor = Editor::new();
    editor.opens_a_file("first", 4000).await;
    editor
        .cuts
        .place(vec![piece("Old", 0, 1000)])
        .await
        .unwrap();
    editor.world.board.press_pad(3).await;
    editor.opens_a_file("second", 4000).await;

    editor
        .cuts
        .place(vec![piece("New 1", 0, 1000), piece("New 2", 1000, 2000)])
        .await
        .unwrap();

    assert_eq!(editor.pad(0).unwrap().0, "Old");
    assert_eq!(editor.pad(1).unwrap().0, "New 1");
    assert_eq!(editor.pad(2).unwrap().0, "New 2");
}

#[tokio::test]
async fn when_the_free_pads_run_out_the_rest_is_left_and_the_user_is_told() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 30_000).await;
    let pieces: Vec<CutPiece> = (0..26)
        .map(|index| piece(&format!("Piece {index}"), index * 1000, (index + 1) * 1000))
        .collect();

    let placed = editor.cuts.place(pieces).await.unwrap();

    assert_eq!(placed, 24);
    assert!(editor.saw(&AppEvent::error(Message::text(TextKey::InterfaceFullHint))));
}

#[tokio::test]
async fn a_piece_shorter_than_a_click_or_outside_the_sound_is_skipped() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;

    let placed = editor
        .cuts
        .place(vec![
            piece("Click", 0, 10),
            piece("After the end", 5000, 6000),
            piece("Good", 1000, 2000),
            piece("  ", 2000, 3000),
        ])
        .await
        .unwrap();

    assert_eq!(placed, 2);
    assert_eq!(editor.pad(0).unwrap().0, "Good");
    // A piece with no name still gets one, so its pad is not blank.
    assert_eq!(editor.pad(1).unwrap().0, "-");
}

#[tokio::test]
async fn a_pad_with_a_cut_plays_only_its_piece_and_shows_where_it_is() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;
    editor
        .cuts
        .place(vec![piece("Boom", 1200, 2500)])
        .await
        .unwrap();

    editor.world.board.press_pad(0).await;

    assert_eq!(
        editor.world.player.cuts.lock().unwrap().last(),
        Some(&cut(1200, 2500))
    );
    let view = editor.world.board.view();
    let shown = view.pads[0].sound.as_ref().unwrap();
    assert_eq!(shown.cut.as_deref(), Some("0:01.2-0:02.5"));
}

#[tokio::test]
async fn closing_drops_a_copy_that_no_pad_took() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;
    editor.cuts.preview(0, 1000).await.unwrap();

    editor.cuts.close();

    assert!(!editor.world.files.exists("store/memes.mp3"));
    assert!(editor.world.player.playing.lock().unwrap().is_empty());
    assert_eq!(editor.cuts.detect().await, Err(missing()));
}

#[tokio::test]
async fn closing_keeps_the_file_of_a_pad() {
    let editor = Editor::new();
    editor.opens_a_file("memes", 4000).await;
    editor
        .cuts
        .place(vec![piece("Boom", 0, 1000)])
        .await
        .unwrap();
    editor.cuts.open(CutSource::Pad { slot: 0 }).await.unwrap();

    editor.cuts.close();

    assert!(editor.world.files.exists("store/memes.mp3"));
}

#[tokio::test]
async fn opening_another_sound_closes_the_one_that_was_open() {
    let editor = Editor::new();
    editor.opens_a_file("first", 4000).await;

    editor.opens_a_file("second", 4000).await;

    assert!(!editor.world.files.exists("store/first.mp3"));
    assert!(editor.world.files.exists("store/second.mp3"));
}

#[tokio::test]
async fn closing_while_the_pieces_are_placed_keeps_the_file_they_need() {
    let editor = Arc::new(Editor::new());
    editor.opens_a_file("memes", 4000).await;
    let closer = editor.clone();
    *editor.world.player.meanwhile.lock().unwrap() = Some(Box::new(move || closer.cuts.close()));

    let placed = editor
        .cuts
        .place(vec![piece("Bruh", 0, 1000), piece("Boom", 1000, 2000)])
        .await
        .unwrap();

    assert_eq!(placed, 2);
    assert!(editor.world.files.exists("store/memes.mp3"));
    assert_eq!(editor.pad(1).unwrap().0, "Boom");
    // The placing closed the editor once it was done.
    *editor.world.player.meanwhile.lock().unwrap() = None;
    assert_eq!(editor.cuts.detect().await, Err(missing()));
}

#[tokio::test]
async fn a_second_create_while_the_first_places_does_nothing() {
    let editor = Arc::new(Editor::new());
    editor.opens_a_file("memes", 4000).await;
    let again = editor.clone();
    *editor.world.player.meanwhile.lock().unwrap() = Some(Box::new(move || {
        let placed = support::poll_once(again.cuts.place(vec![piece("Again", 2000, 3000)]));
        assert_eq!(placed, Some(Ok(0)));
    }));

    editor
        .cuts
        .place(vec![piece("Bruh", 0, 1000)])
        .await
        .unwrap();

    assert_eq!(editor.pad(0).unwrap().0, "Bruh");
    assert!(editor.pad(1).is_none());
}
