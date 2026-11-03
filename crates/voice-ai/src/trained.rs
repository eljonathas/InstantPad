//! Trained models the user adds from what the kit in `tools/voice-training` exported: a folder
//! with `voice.json`, the synthesizer of the voice and HuBERT, which every voice shares.

use std::fs;
use std::io::Read;
use std::path::Path;

use instantpad_application::message::{Failure, TextKey};
use instantpad_domain::cloning::TrainedModel;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::rvc::{CONTENT, SYNTHESIZER, TRAINED};

const VOICE: &str = "voice.json";
/// The layout of `voice.json` this version reads.
const FORMAT: u32 = 1;

/// What the kit writes in `voice.json`, as far as the app needs it.
#[derive(Deserialize)]
struct Voice {
    format: u32,
    engine: String,
    name: String,
    sample_rate: u32,
    pitch: Pitch,
    content: File,
    synthesizer: Synthesizer,
    #[serde(default)]
    credit: String,
}

#[derive(Deserialize)]
struct Pitch {
    median_hz: f32,
}

#[derive(Deserialize)]
struct File {
    file: String,
    sha256: String,
    size: u64,
}

#[derive(Deserialize)]
struct Synthesizer {
    #[serde(flatten)]
    file: File,
    speaker: u32,
    flow_context: u32,
}

fn invalid(detail: impl ToString) -> Failure {
    Failure::because(TextKey::TrainedModelInvalid, detail)
}

/// Checks the voice in `source` and copies it to the model `id` in `models`, along with HuBERT
/// when the app has none yet. Nothing is left behind when it fails.
pub(crate) fn add(models: &Path, source: &Path, id: &str) -> Result<TrainedModel, Failure> {
    let text = fs::read_to_string(source.join(VOICE))
        .map_err(|error| invalid(format!("{VOICE}: {error}")))?;
    let voice: Voice =
        serde_json::from_str(&text).map_err(|error| invalid(format!("{VOICE}: {error}")))?;
    if voice.format != FORMAT
        || voice.engine != "rvc"
        || voice.sample_rate != 40_000
        || voice.synthesizer.speaker != 0
        || voice.synthesizer.flow_context != 24
    {
        return Err(invalid(
            "the voice was exported by another version of the kit",
        ));
    }
    if !(50.0..=1100.0).contains(&voice.pitch.median_hz) {
        return Err(invalid("the pitch of the voice is out of range"));
    }

    let synthesizer = checked(source, &voice.synthesizer.file)?;
    let content = models.join(CONTENT);
    // Every export of HuBERT hears alike, so the one the app has serves every voice.
    let new_content = if content.exists() {
        None
    } else {
        Some(checked(source, &voice.content)?)
    };

    let folder = models.join(TRAINED).join(id);
    let copied = (|| -> std::io::Result<()> {
        fs::create_dir_all(&folder)?;
        copy(&synthesizer, &folder.join(SYNTHESIZER))?;
        fs::write(folder.join(VOICE), &text)?;
        if let Some(new_content) = &new_content {
            copy(new_content, &content)?;
        }
        Ok(())
    })();
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&folder);
        return Err(Failure::because(TextKey::SaveFailed, error));
    }

    Ok(TrainedModel {
        id: id.to_owned(),
        name: voice.name,
        credit: voice.credit,
        pitch_hz: voice.pitch.median_hz,
    })
}

/// Deletes a model's folder.
pub(crate) fn remove(models: &Path, id: &str) {
    // Only a plain folder name, never a path out of the models.
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return;
    }
    let _ = fs::remove_dir_all(models.join(TRAINED).join(id));
}

/// The path of a file the kit listed, once its size and SHA-256 are the ones listed.
fn checked(source: &Path, listed: &File) -> Result<std::path::PathBuf, Failure> {
    let name = Path::new(&listed.file);
    if name.components().count() != 1 {
        return Err(invalid(format!(
            "{} is not a file of the folder",
            listed.file
        )));
    }
    let path = source.join(name);
    let size = fs::metadata(&path)
        .map_err(|error| invalid(format!("{}: {error}", listed.file)))?
        .len();
    if size != listed.size || sha256(&path)? != listed.sha256.to_lowercase() {
        return Err(invalid(format!(
            "{} is not the file the kit exported",
            listed.file
        )));
    }
    Ok(path)
}

fn sha256(path: &Path) -> Result<String, Failure> {
    let mut file = fs::File::open(path).map_err(invalid)?;
    let mut digest = Sha256::new();
    let mut block = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut block).map_err(invalid)?;
        if read == 0 {
            break;
        }
        digest.update(&block[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Copies through a file of its own, so a copy that stops halfway never looks whole.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    let part = to.with_extension("part");
    fs::copy(from, &part)?;
    fs::rename(&part, to)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use instantpad_application::message::Message;

    use super::*;

    struct Folders {
        root: PathBuf,
    }

    impl Folders {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("instantpad-trained-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("export")).unwrap();
            fs::create_dir_all(root.join("models")).unwrap();
            Self { root }
        }

        fn export(&self) -> PathBuf {
            self.root.join("export")
        }

        fn models(&self) -> PathBuf {
            self.root.join("models")
        }
    }

    impl Drop for Folders {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    /// What the kit exports, with small files standing for the models.
    fn export(folder: &Path, change: impl FnOnce(&mut serde_json::Value)) {
        let synthesizer = b"the voice".as_slice();
        let content = b"what is said".as_slice();
        fs::write(folder.join("synthesizer.onnx"), synthesizer).unwrap();
        fs::write(folder.join("content.onnx"), content).unwrap();
        let mut voice = serde_json::json!({
            "format": 1,
            "engine": "rvc",
            "name": "feminine",
            "sample_rate": 40000,
            "frame_rate": 100,
            "pitch": {"median_hz": 210.5},
            "content": {"file": "content.onnx", "sha256": digest(content),
                        "size": content.len(), "sample_rate": 16000, "frame_rate": 50},
            "synthesizer": {"file": "synthesizer.onnx", "sha256": digest(synthesizer),
                            "size": synthesizer.len(), "speaker": 0, "flow_context": 24},
            "credit": "Hi-Fi TTS, reader 92",
            "trained": {"epochs": 200},
            "rvc": "81eed5e"
        });
        change(&mut voice);
        fs::write(folder.join(VOICE), voice.to_string()).unwrap();
    }

    fn key(failure: &Failure) -> TextKey {
        let Failure(Message::Text { key, .. }) = failure else {
            panic!("expected a translated message");
        };
        *key
    }

    #[test]
    fn a_voice_the_kit_exported_is_copied_with_hubert() {
        let folders = Folders::new("added");
        export(&folders.export(), |_| {});

        let model = add(&folders.models(), &folders.export(), "abc").unwrap();

        assert_eq!(
            model,
            TrainedModel {
                id: "abc".to_owned(),
                name: "feminine".to_owned(),
                credit: "Hi-Fi TTS, reader 92".to_owned(),
                pitch_hz: 210.5,
            }
        );
        let folder = folders.models().join("trained/abc");
        assert_eq!(
            fs::read(folder.join("synthesizer.onnx")).unwrap(),
            b"the voice"
        );
        assert!(folder.join(VOICE).exists());
        assert_eq!(
            fs::read(folders.models().join(CONTENT)).unwrap(),
            b"what is said"
        );
        assert!(!folder.join("synthesizer.part").exists());
    }

    #[test]
    fn hubert_the_app_has_is_kept_and_need_not_come_along() {
        let folders = Folders::new("kept");
        export(&folders.export(), |_| {});
        fs::remove_file(folders.export().join("content.onnx")).unwrap();
        fs::write(folders.models().join(CONTENT), b"the first one").unwrap();

        add(&folders.models(), &folders.export(), "abc").unwrap();

        assert_eq!(
            fs::read(folders.models().join(CONTENT)).unwrap(),
            b"the first one"
        );
    }

    #[test]
    fn a_folder_without_a_voice_of_the_kit_is_refused() {
        let folders = Folders::new("empty");

        let failure = add(&folders.models(), &folders.export(), "abc").unwrap_err();

        assert_eq!(key(&failure), TextKey::TrainedModelInvalid);
        assert!(!folders.models().join("trained/abc").exists());
    }

    #[test]
    fn a_file_that_is_not_the_one_exported_is_refused() {
        let folders = Folders::new("changed");
        export(&folders.export(), |_| {});
        fs::write(folders.export().join("synthesizer.onnx"), b"the vocie").unwrap();

        let failure = add(&folders.models(), &folders.export(), "abc").unwrap_err();

        assert_eq!(key(&failure), TextKey::TrainedModelInvalid);
        assert!(!folders.models().join("trained/abc").exists());
    }

    #[test]
    fn a_voice_of_another_format_or_engine_is_refused() {
        for change in [
            |voice: &mut serde_json::Value| voice["format"] = 2.into(),
            |voice: &mut serde_json::Value| voice["engine"] = "so-vits".into(),
            |voice: &mut serde_json::Value| voice["sample_rate"] = 48_000.into(),
            |voice: &mut serde_json::Value| voice["pitch"]["median_hz"] = 0.into(),
        ] {
            let folders = Folders::new("other");
            export(&folders.export(), change);

            let failure = add(&folders.models(), &folders.export(), "abc").unwrap_err();

            assert_eq!(key(&failure), TextKey::TrainedModelInvalid);
        }
    }

    #[test]
    fn a_listed_file_outside_the_folder_is_refused() {
        let folders = Folders::new("outside");
        export(&folders.export(), |voice| {
            voice["synthesizer"]["file"] = "../synthesizer.onnx".into();
        });

        let failure = add(&folders.models(), &folders.export(), "abc").unwrap_err();

        assert_eq!(key(&failure), TextKey::TrainedModelInvalid);
    }

    #[test]
    fn removing_a_model_deletes_its_folder_and_nothing_else() {
        let folders = Folders::new("removed");
        export(&folders.export(), |_| {});
        add(&folders.models(), &folders.export(), "abc").unwrap();

        remove(&folders.models(), "..");
        remove(&folders.models(), "abc");

        assert!(!folders.models().join("trained/abc").exists());
        assert!(folders.models().join(CONTENT).exists());
    }
}
