//! Behavior every adapter of a port must share. Run these against each implementation.

use instantpad_application::hotkeys::HotkeyCommand;
use instantpad_application::ports::{
    AudioDevice, AudioRouter, BoardRepository, FavoritesRepository, HotkeyHost, PlaybackTag,
    SettingsRepository, SoundFileStore, SoundPlayer, TrainingRepository, TrainingRun, VirtualCable,
    VoiceLibraryRepository, VoiceTrainer,
};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_domain::cloning::{
    ClonedVoice, DesignedVoice, TrainedModel, VoiceLibrary, VoiceRecipe, Voiceprint,
};
use instantpad_domain::cuts::{Cut, MIN_CUT_MS};
use instantpad_domain::hotkeys::{Hotkey, HotkeyModifiers};
use instantpad_domain::sounds::SoundSource;
use instantpad_domain::training::{
    GraphicsCard, ProcessorReason, TrainingDevice, TrainingJob, TrainingPlan,
};

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

/// Needs no device: it only closes the roles, which every router can do.
pub async fn audio_router(router: &dyn AudioRouter) {
    assert!(router.take_stopped().is_empty(), "nothing stopped yet");

    router.set_microphone(None).await.expect("close the mic");
    router.set_game_output(None).await.expect("close the game");
    router
        .set_monitor_output(None)
        .await
        .expect("close the monitor");

    assert!(
        router.take_stopped().is_empty(),
        "a role closed on purpose did not stop by itself"
    );
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
        trained: [(
            "a".to_owned(),
            TrainedModel {
                id: "m".to_owned(),
                name: "ana".to_owned(),
                credit: "Recordings of Ana".to_owned(),
                pitch_hz: 212.5,
            },
        )]
        .into(),
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

pub fn training_repository(repository: &dyn TrainingRepository) {
    let card = GraphicsCard {
        name: "NVIDIA GeForce RTX 3070 Ti".to_owned(),
        memory_megabytes: 8192,
        driver: "610.88".to_owned(),
    };
    let mut job = TrainingJob::new("ana", TrainingPlan::new(TrainingDevice::Card, Some(&card)));
    job.add_take(300);
    job.is_started = true;
    job.epoch = 35;

    repository.save(Some(&job)).expect("save the training");
    assert_eq!(repository.load(), Some(job));

    repository.save(None).expect("forget the training");
    assert_eq!(repository.load(), None);
}

/// `trainer` must have no tools installed. Nothing here downloads or trains, so it needs no
/// network, card or Python.
pub async fn voice_trainer(trainer: &dyn VoiceTrainer) {
    let processor = TrainingDevice::Processor {
        reason: ProcessorReason::NoCard,
    };
    let on_card = trainer.tools(TrainingDevice::Card);
    let on_processor = trainer.tools(processor);
    for tools in [on_card, on_processor] {
        assert!(!tools.is_installed);
        assert!(tools.download_megabytes > 0);
        assert!(tools.installed_megabytes > 0);
    }
    assert!(
        on_card.download_megabytes > on_processor.download_megabytes,
        "PyTorch for the card is far bigger"
    );
    assert_eq!(trainer.installed_megabytes(), 0);

    // Stopping with nothing running, and deleting what is not there, do nothing.
    trainer.stop();
    trainer.remove_tools();
    trainer.discard("voice-never-trained");

    let job = TrainingJob::new("ana", TrainingPlan::new(processor, None));
    trainer
        .keep_take(&job.id, "take-1", vec![0.1; 48_000])
        .await
        .expect("write a take");
    trainer.remove_take(&job.id, "take-1");
    trainer.remove_take(&job.id, "take-never-written");

    let run = TrainingRun {
        job: job.id.clone(),
        plan: job.plan,
        title: "Ana".to_owned(),
    };
    let trained = trainer.train(&run, &|_| {}).await;
    assert!(trained.is_err(), "no training without its tools");
    trainer.discard(&job.id);
}
