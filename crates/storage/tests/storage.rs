use std::fs;
use std::path::Path;

use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{
    BoardRepository, FavoritesRepository, SettingsRepository, SoundFileStore,
};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_storage::{
    AppDataFolder, FolderSoundStore, JsonBoardRepository, JsonFavoritesRepository,
    JsonSettingsRepository,
};
use instantpad_test_support::contract;
use tempfile::TempDir;

fn folder() -> (TempDir, AppDataFolder) {
    let dir = tempfile::tempdir().unwrap();
    let folder = AppDataFolder::new(dir.path().to_path_buf()).unwrap();
    (dir, folder)
}

fn failed_with(result: Result<impl std::fmt::Debug, Failure>, key: TextKey) {
    let failure = result.expect_err("should fail");
    assert!(
        format!("{failure:?}").contains(&format!("{key:?}")),
        "{failure:?}"
    );
}

#[test]
fn the_data_folder_creates_the_sounds_folder() {
    let (dir, _) = folder();

    assert!(dir.path().join("sounds").is_dir());
}

#[test]
fn the_repositories_keep_what_they_save() {
    let (_dir, folder) = folder();

    contract::board_repository(&JsonBoardRepository::new(&folder));
    contract::favorites_repository(&JsonFavoritesRepository::new(&folder));
    contract::settings_repository(&JsonSettingsRepository::new(&folder));
}

#[test]
fn a_missing_file_gives_the_default() {
    let (_dir, folder) = folder();

    assert_eq!(
        JsonBoardRepository::new(&folder).load(),
        PadBoard::default()
    );
    assert!(JsonFavoritesRepository::new(&folder).load().is_empty());
    assert_eq!(
        JsonSettingsRepository::new(&folder).load(),
        AppSettings::default()
    );
}

#[test]
fn a_damaged_file_is_kept_as_a_backup_and_gives_the_default() {
    let (dir, folder) = folder();
    fs::write(dir.path().join("settings.json"), "{ not json").unwrap();

    let settings = JsonSettingsRepository::new(&folder).load();

    assert_eq!(settings, AppSettings::default());
    assert_eq!(
        fs::read_to_string(dir.path().join("settings.json.bak")).unwrap(),
        "{ not json"
    );
}

#[test]
fn a_file_that_is_not_text_is_kept_as_a_backup_too() {
    let (dir, folder) = folder();
    fs::write(dir.path().join("board.json"), [0xff, 0xfe, 0x00]).unwrap();

    JsonBoardRepository::new(&folder).load();

    assert!(dir.path().join("board.json.bak").is_file());
}

#[test]
fn saving_leaves_no_partial_file_behind() {
    let (dir, folder) = folder();

    JsonSettingsRepository::new(&folder)
        .save(&AppSettings::default())
        .unwrap();

    assert!(dir.path().join("settings.json").is_file());
    assert!(!dir.path().join("settings.json.part").exists());
}

#[test]
fn settings_with_absent_fields_take_their_defaults() {
    let (dir, folder) = folder();
    fs::write(dir.path().join("settings.json"), r#"{ "micVolume": 0.5 }"#).unwrap();

    let settings = JsonSettingsRepository::new(&folder).load();

    assert_eq!(
        settings,
        AppSettings {
            mic_volume: 0.5,
            ..AppSettings::default()
        }
    );
}

#[test]
fn a_save_that_cannot_be_written_is_reported() {
    let (dir, folder) = folder();
    let repository = JsonSettingsRepository::new(&folder);
    drop(dir);

    failed_with(
        repository.save(&AppSettings::default()),
        TextKey::SaveFailed,
    );
}

#[test]
fn export_copies_the_sound_to_the_chosen_place() {
    let (dir, folder) = folder();
    let store = FolderSoundStore::new(&folder);
    let stored = dir.path().join("sounds").join("a.mp3");
    fs::write(&stored, b"audio").unwrap();
    let chosen = dir.path().join("copy.mp3");

    store
        .export(stored.to_str().unwrap(), chosen.to_str().unwrap())
        .unwrap();

    assert_eq!(fs::read(chosen).unwrap(), b"audio");
}

#[test]
fn export_to_an_invalid_place_fails() {
    let (dir, folder) = folder();
    let store = FolderSoundStore::new(&folder);
    let stored = dir.path().join("sounds").join("a.mp3");
    fs::write(&stored, b"audio").unwrap();
    let invalid = dir.path().join("missing").join("copy.mp3");

    failed_with(
        store.export(stored.to_str().unwrap(), invalid.to_str().unwrap()),
        TextKey::SoundSaveFailed,
    );
}

#[test]
fn a_download_lands_inside_the_store_with_a_safe_name() {
    let (dir, folder) = folder();
    let store = FolderSoundStore::new(&folder);

    let path = store.path_for_download("https://site/media/sounds/som%3Fa.mp3");

    assert_eq!(
        Path::new(&path),
        dir.path().join("sounds").join("som_a.mp3")
    );
}

#[test]
fn importing_copies_the_file_into_the_store() {
    let (dir, folder) = folder();
    let store = FolderSoundStore::new(&folder);
    let outside = dir.path().join("outside.mp3");
    fs::write(&outside, b"audio").unwrap();

    let stored = store.import(outside.to_str().unwrap()).unwrap();

    assert_eq!(Path::new(&stored), dir.path().join("sounds/outside.mp3"));
    assert!(store.exists(&stored));
    assert_eq!(fs::read(stored).unwrap(), b"audio");
}

#[test]
fn importing_a_file_already_in_the_store_keeps_it_intact() {
    let (dir, folder) = folder();
    let store = FolderSoundStore::new(&folder);
    let inside = dir.path().join("sounds").join("a.mp3");
    fs::write(&inside, b"audio").unwrap();

    let stored = store.import(inside.to_str().unwrap()).unwrap();

    assert_eq!(fs::read(stored).unwrap(), b"audio");
}

#[test]
fn importing_a_missing_file_fails() {
    let (dir, folder) = folder();
    let store = FolderSoundStore::new(&folder);
    let missing = dir.path().join("nothing.mp3");

    failed_with(
        store.import(missing.to_str().unwrap()),
        TextKey::SoundCopyFailed,
    );
}
