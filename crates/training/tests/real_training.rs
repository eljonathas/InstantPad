//! Trains a real voice from nothing: downloads the tools (about 4 GB with an NVIDIA card), takes
//! the WAV files of a folder as the speech, and exports the model. It needs the network and an
//! hour or more, so it only runs when asked:
//!
//! ```text
//! INSTANTPAD_TRAINING_HOME=<empty folder> INSTANTPAD_TRAINING_SPEECH=<folder of WAV files> \
//!   cargo test -p instantpad-training --test real_training -- --ignored --nocapture
//! ```
//!
//! `INSTANTPAD_TRAINING_EPOCHS` (2 by default) sets how long it learns.

use std::path::PathBuf;
use std::sync::Mutex;
use std::{env, fs};

use instantpad_application::ports::{Outcome, TrainingRun, VoiceTrainer};
use instantpad_domain::training::{TrainingDevice, TrainingPlan};
use instantpad_training::KitTrainer;

fn folder(variable: &str) -> PathBuf {
    PathBuf::from(env::var_os(variable).unwrap_or_else(|| panic!("set {variable}")))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads gigabytes and trains for an hour or more"]
async fn a_voice_trains_from_nothing() {
    let home = folder("INSTANTPAD_TRAINING_HOME");
    let speech = folder("INSTANTPAD_TRAINING_SPEECH");
    let epochs = env::var("INSTANTPAD_TRAINING_EPOCHS").map_or(2, |epochs| epochs.parse().unwrap());
    let trainer = KitTrainer::new(&home);
    assert!(trainer.is_supported());

    let card = trainer.graphics_card().await;
    let device = TrainingDevice::choose(card.as_ref(), trainer.system());
    println!("card {card:?}, training on {device:?}");
    let tools = trainer.tools(device);
    println!("downloads {} MB", tools.download_megabytes);

    let shown = Mutex::new(None);
    let installed = trainer
        .install(device, &|percent| {
            let mut shown = shown.lock().unwrap();
            if *shown != Some(percent) {
                *shown = Some(percent);
                println!("installing {percent}%");
            }
        })
        .await
        .expect("install the tools");
    assert_eq!(installed, Outcome::Done(()));
    assert!(trainer.tools(device).is_installed);
    println!("installed, {} MB", trainer.installed_megabytes());

    let job = "voice-real-training";
    let dataset = home.join("voices").join(job).join("dataset");
    fs::create_dir_all(&dataset).unwrap();
    for file in fs::read_dir(&speech).unwrap().flatten() {
        if file
            .path()
            .extension()
            .is_some_and(|extension| extension == "wav")
        {
            fs::copy(file.path(), dataset.join(file.file_name())).unwrap();
        }
    }

    let mut plan = TrainingPlan::new(device, card.as_ref());
    plan.epochs = epochs;
    plan.save_every = 1;
    let run = TrainingRun {
        job: job.to_owned(),
        plan,
        title: "Real training".to_owned(),
    };
    let trained = trainer
        .train(&run, &|progress| println!("{progress:?}"))
        .await
        .expect("train the voice");

    let Outcome::Done(exported) = trained else {
        panic!("the training stopped");
    };
    println!("exported to {}", exported.display());
    assert!(exported.join("voice.json").exists());
}
