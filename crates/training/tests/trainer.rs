//! The kit trainer against the port's contract, and what it keeps on disk. Nothing here
//! downloads or trains: no network, card or Python.

use std::fs;

use instantpad_application::ports::VoiceTrainer;
use instantpad_test_support::contract;
use instantpad_training::KitTrainer;

#[tokio::test]
async fn the_kit_trainer_keeps_the_contract() {
    let home = tempfile::tempdir().unwrap();

    contract::voice_trainer(&KitTrainer::new(home.path())).await;
}

#[tokio::test]
async fn a_take_is_a_wav_in_the_folder_of_its_training() {
    let home = tempfile::tempdir().unwrap();
    let trainer = KitTrainer::new(home.path());

    trainer
        .keep_take("voice-1", "take-1", vec![0.0; 4800])
        .await
        .unwrap();

    let take = home.path().join("voices/voice-1/dataset/take-1.wav");
    assert_eq!(fs::metadata(&take).unwrap().len(), 44 + 4800 * 2);
    trainer.remove_take("voice-1", "take-1");
    assert!(!take.exists());
}

#[tokio::test]
async fn names_that_lead_out_of_the_folder_are_refused() {
    let home = tempfile::tempdir().unwrap();
    let trainer = KitTrainer::new(home.path());

    let kept = trainer
        .keep_take("../voice-1", "take-1", vec![0.0; 10])
        .await;

    assert!(kept.is_err());
}

#[test]
fn removing_the_tools_keeps_what_trainings_made() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path();
    for folder in [
        "venv/Scripts",
        "uv",
        "rvc/infer",
        "rvc/assets/rmvpe",
        "rvc/assets/weights",
        "rvc/logs/mute",
        "rvc/logs/voice-1",
        "voices/voice-1/dataset",
    ] {
        fs::create_dir_all(path.join(folder)).unwrap();
    }
    fs::write(path.join("tools.json"), "{}").unwrap();
    fs::write(path.join("rvc/assets/weights/voice-1.pth"), "").unwrap();

    KitTrainer::new(path).remove_tools();

    for gone in [
        "venv",
        "uv",
        "tools.json",
        "rvc/infer",
        "rvc/assets/rmvpe",
        "rvc/logs/mute",
    ] {
        assert!(!path.join(gone).exists(), "{gone} should be gone");
    }
    for kept in [
        "rvc/logs/voice-1",
        "rvc/assets/weights/voice-1.pth",
        "voices/voice-1/dataset",
    ] {
        assert!(path.join(kept).exists(), "{kept} should be kept");
    }
}

#[test]
fn discarding_a_training_deletes_everything_it_made() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path();
    for folder in [
        "rvc/logs/voice-1",
        "rvc/logs/voice-2",
        "rvc/assets/weights",
        "voices/voice-1",
    ] {
        fs::create_dir_all(path.join(folder)).unwrap();
    }
    fs::write(path.join("rvc/assets/weights/voice-1.pth"), "").unwrap();

    KitTrainer::new(path).discard("voice-1");

    assert!(!path.join("voices/voice-1").exists());
    assert!(!path.join("rvc/logs/voice-1").exists());
    assert!(!path.join("rvc/assets/weights/voice-1.pth").exists());
    assert!(path.join("rvc/logs/voice-2").exists());
}
