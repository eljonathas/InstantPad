//! Behavior every adapter of a port must share. Run these against each implementation.

use instantpad_application::hotkeys::HotkeyCommand;
use instantpad_application::ports::{
    AudioDevice, BoardRepository, FavoritesRepository, HotkeyHost, PlaybackTag, SettingsRepository,
    SoundFileStore, SoundPlayer, VirtualCable, VoiceLibraryRepository,
};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_domain::cloning::{
    ClonedVoice, DesignedVoice, VoiceLibrary, VoiceRecipe, Voiceprint,
};
use instantpad_domain::cuts::{Cut, MIN_CUT_MS};
use instantpad_domain::hotkeys::{Hotkey, HotkeyModifiers};
use instantpad_domain::sounds::SoundSource;

/// `path` must be a sound that `player` can decode.
pub async fn sound_player(player: &dyn SoundPlayer, path: &str) {
    let (first, second) = (PlaybackTag::of("first"), PlaybackTag::of("second"));

    player.preload(path).await.expect("preload a good file");
    player
        .play(path, 1.0, first, None)
        .await
        .expect("play a good file");
    player
        .preview(path, 1.0, second, None)
        .await
        .expect("preview a good file");
    assert!(player.is_playing(first));
    assert!(player.is_playing(second));

    player.stop(first);
    assert!(!player.is_playing(first));
    assert!(player.is_playing(second));

    player.stop_all();
    assert!(!player.is_playing(second));

    // A cut plays only its piece, and the drawing of a sound has as many bars as asked.
    let cut = Cut {
        start_ms: 0,
        end_ms: MIN_CUT_MS,
    };
    player
        .play(path, 1.0, first, Some(cut))
        .await
        .expect("play a cut of a good file");
    assert!(player.is_playing(first));
    player.stop_all();
    let drawn = player
        .waveform(path, 0, u32::MAX, 32)
        .await
        .expect("draw a good file");
    assert!(drawn.duration_ms > 0);
    assert_eq!(drawn.peaks.len(), 32);
    assert!(drawn.peaks.iter().all(|peak| (0.0..=1.0).contains(peak)));
    let levels = player.levels(path, 10).await.expect("measure a good file");
    assert!(levels.iter().all(|level| (0.0..=1.0).contains(level)));
}

/// `free` must be a key no other program holds.
pub fn hotkey_host(host: &dyn HotkeyHost, free: &Hotkey) {
    assert!(host.register(free, HotkeyCommand::StopAll));
    host.clear();
    assert!(
        host.register(free, HotkeyCommand::StopAll),
        "clear releases the keys"
    );
    host.clear();
}

pub fn board_repository(repository: &dyn BoardRepository) {
    let mut board = PadBoard::default();
    board.ensure_interface("Main");
    let id = board.interfaces()[0].id.clone();
    let sound = board.place(&id, 3, "a", "a.mp3", "#ff0000").id.clone();
    board.sound_mut(&sound).unwrap().hotkey = Some(Hotkey::new("F1", HotkeyModifiers::CTRL));

    repository.save(&board).expect("save the board");

    assert_eq!(repository.load(), board);
}

pub fn favorites_repository(repository: &dyn FavoritesRepository) {
    let favorites = [SoundSource {
        name: "x".to_owned(),
        color: "#ffffff".to_owned(),
        path: "y.mp3".to_owned(),
        sound_url: Some("z".to_owned()),
    }];

    repository.save(&favorites).expect("save the favorites");

    assert_eq!(repository.load(), favorites);
}

pub fn settings_repository(repository: &dyn SettingsRepository) {
    let settings = AppSettings {
        game_volume: 0.25,
        language: Some("pt".to_owned()),
        stop_hotkey: Some(Hotkey::new("F9", HotkeyModifiers::ALT)),
        ..AppSettings::default()
    };

    repository.save(&settings).expect("save the settings");

    assert_eq!(repository.load(), settings);
}

pub fn voice_library_repository(repository: &dyn VoiceLibraryRepository) {
    let library = VoiceLibrary {
        own: Some(Voiceprint::new(vec![0.5, -0.25])),
        clones: vec![ClonedVoice {
            id: "a".to_owned(),
            name: "Ana".to_owned(),
            voiceprint: Voiceprint::new(vec![0.25, 0.75]),
        }],
        designs: vec![DesignedVoice {
            id: "d".to_owned(),
            name: "Mix".to_owned(),
            recipe: VoiceRecipe {
                base_id: "a".to_owned(),
                timbre: 0.25,
                blend_id: Some("b".to_owned()),
                blend: 0.25,
            },
        }],
    };

    repository.save(&library).expect("save the voices");

    assert_eq!(repository.load(), library);
}

/// `render` and `capture` are the two sides of a cable, `real` is a physical device.
pub fn virtual_cable(
    cable: &dyn VirtualCable,
    render: &AudioDevice,
    capture: &AudioDevice,
    real: &AudioDevice,
) {
    assert!(cable.is_virtual(render));
    assert!(cable.is_virtual(capture));
    assert!(!cable.is_virtual(real));

    let found = cable.find_game_output(&[real.clone(), render.clone()]);
    assert_eq!(found.as_ref(), Some(render));
    assert_eq!(cable.find_game_output(std::slice::from_ref(real)), None);
    assert!(!cable.game_microphone_name(Some(render)).is_empty());
}

/// `outside` must be an existing file, and `destination` a place that can be written.
pub fn sound_file_store(store: &dyn SoundFileStore, outside: &str, destination: &str) {
    let stored = store.import(outside).expect("import a file");
    assert!(store.exists(&stored));
    assert_eq!(
        store.import(&stored).as_ref(),
        Ok(&stored),
        "already stored"
    );

    store
        .export(&stored, destination)
        .expect("export a stored file");

    store.delete(&stored);
    assert!(!store.exists(&stored));

    let target = store.path_for_download("https://site/media/sounds/som.mp3");
    assert!(target.ends_with("som.mp3"));
    assert!(!store.exists("this file does not exist.mp3"));

    let preview = store.path_for_preview("https://site/media/sounds/som.mp3");
    assert_ne!(preview, target, "a sound being tried out lives apart");
    store.clear_previews();
    assert!(!store.exists(&preview));
}
