//! Trains voices inside the app with the kit in `tools/voice-training`: downloads its tools when
//! the user agrees, and runs it as a child process. See ADR 0007.
//!
//! Everything lives in one folder:
//!
//! | Path | What it is |
//! | --- | --- |
//! | `uv`, `python`, `venv` | uv, the Python it installed, and the packages of the training |
//! | `rvc`, `downloads`, `bin` | RVC and its base models, and FFmpeg, from `voicekit.py setup` |
//! | `voicekit.py`, `tools.json` | the kit, and what was installed for which device |
//! | `voices/<training>` | the takes of a training and its log; RVC's own work is in `rvc/logs` |

mod download;
mod events;
mod pins;
mod process;
mod wav;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use instantpad_application::message::{Failure, Message};
use instantpad_application::ports::{
    Outcome, TrainingProgress, TrainingRun, TrainingStage, TrainingTools, VoiceTrainer,
};
use instantpad_domain::training::{GraphicsCard, System, TrainingDevice};
use serde::{Deserialize, Serialize};

use crate::download::{Interrupted, client, fetch, is_there};
use crate::events::KitEvent;

const BYTES_PER_MEGABYTE: u64 = 1_000_000;
/// The rate of the speech the app hands over, which the takes keep.
const SPEECH_RATE: u32 = 48_000;
/// `nvidia-smi` answers at once, unless the driver is in a bad state.
const CARD_TIMEOUT: Duration = Duration::from_secs(10);
/// Where RVC keeps what it makes of a training, under `rvc`.
const EXPERIMENTS: &str = "logs";
const WEIGHTS: &str = "assets/weights";
/// What a failure shows of the output of the kit, when it did not say what went wrong.
const LAST_LINES: usize = 3;

/// What is installed, kept in `tools.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Installed {
    on_card: bool,
    /// Of the pinned packages, so new pins install again.
    fingerprint: String,
    megabytes: u32,
}

pub struct KitTrainer {
    home: PathBuf,
    /// Set by `stop`, and cleared when work starts.
    stopping: AtomicBool,
    running: process::Slot,
    installed: Mutex<Option<Installed>>,
}

impl KitTrainer {
    pub fn new(home: &Path) -> Self {
        let installed = fs::read_to_string(home.join("tools.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        Self {
            home: home.to_owned(),
            stopping: AtomicBool::new(false),
            running: Mutex::new(None),
            installed: Mutex::new(installed),
        }
    }

    fn uv(&self) -> PathBuf {
        self.home
            .join("uv")
            .join(if cfg!(windows) { "uv.exe" } else { "uv" })
    }

    fn python(&self) -> PathBuf {
        let venv = self.home.join("venv");
        if cfg!(windows) {
            venv.join("Scripts").join("python.exe")
        } else {
            venv.join("bin").join("python")
        }
    }

    fn wheels(&self) -> PathBuf {
        self.home.join("wheels")
    }

    fn voice(&self, job: &str) -> PathBuf {
        self.home.join("voices").join(job)
    }

    /// A command with the folders of the training, and nothing of a Python the user may have.
    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.home)
            .env("INSTANTPAD_VOICE_HOME", &self.home)
            .env("UV_PYTHON_INSTALL_DIR", self.home.join("python"))
            .env("UV_CACHE_DIR", self.home.join("cache"))
            .env("UV_NO_CONFIG", "1")
            .env("UV_NO_CACHE", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env_remove("PYTHONPATH")
            .env_remove("PYTHONHOME")
            .env_remove("VIRTUAL_ENV");
        command
    }

    fn kit(&self) -> Result<Command, Failure> {
        let kit = self.home.join("voicekit.py");
        fs::write(&kit, pins::VOICEKIT).map_err(failed)?;
        let mut command = self.command(&self.python());
        command.arg(kit).arg("--events");
        Ok(command)
    }

    /// Bytes still to download for the device, from what is in the folder.
    fn missing_bytes(&self, on_card: bool) -> u64 {
        let environment = pins::environment(on_card);
        let wheels = environment
            .wheels
            .iter()
            .filter(|wheel| !is_there(&self.wheels().join(file_name(&wheel.url)), wheel))
            .map(|wheel| wheel.size)
            .sum::<u64>();
        let uv = if self.uv().exists() {
            0
        } else {
            pins::uv().file.size
        };
        let python = if self.python().exists() {
            0
        } else {
            pins::PYTHON_BYTES
        };
        let kit = if self.home.join("rvc").join(".commit").exists() {
            0
        } else {
            pins::KIT_DOWNLOAD_BYTES
        };
        uv + python + wheels + kit
    }

    fn installed_lock(&self) -> std::sync::MutexGuard<'_, Option<Installed>> {
        self.installed.lock().expect("tools lock poisoned")
    }

    fn stopped(&self) -> bool {
        self.stopping.load(Ordering::Relaxed)
    }

    async fn install_tools(
        &self,
        on_card: bool,
        progress: &(dyn Fn(u8) + Send + Sync),
    ) -> Result<(), Interrupted> {
        let environment = pins::environment(on_card);
        let uv = pins::uv();
        let total = uv.file.size
            + pins::PYTHON_BYTES
            + environment.wheel_bytes()
            + pins::KIT_DOWNLOAD_BYTES;
        let done = Mutex::new(0_u64);
        let shown = Mutex::new(0_u8);
        let advance = |bytes: u64| {
            let mut done = done.lock().expect("progress lock poisoned");
            *done = (*done + bytes).min(total);
            let percent = (*done * 100 / total) as u8;
            let mut shown = shown.lock().expect("progress lock poisoned");
            if percent != *shown {
                *shown = percent;
                progress(percent);
            }
        };
        let http = client()?;

        if !self.uv().exists() {
            let archive = self.home.join("downloads").join(file_name(&uv.file.url));
            fetch(&http, &uv.file, &archive, &self.stopping, &mut |bytes| {
                advance(bytes)
            })
            .await?;
            let target = self.uv();
            tokio::task::spawn_blocking(move || unpack_uv(&archive, uv.entry, &target))
                .await
                .map_err(|error| Interrupted::Failed(error.to_string()))??;
        } else {
            advance(uv.file.size);
        }

        if !self.python().exists() {
            let mut command = self.command(&self.uv());
            command
                .args(["venv", "--managed-python", "--no-project", "--python"])
                .arg(&environment.python)
                .arg(self.home.join("venv"));
            self.step(command, &mut |_| {}).await?;
        }
        advance(pins::PYTHON_BYTES);

        let mut files = Vec::new();
        for wheel in &environment.wheels {
            let target = self.wheels().join(file_name(&wheel.url));
            fetch(&http, wheel, &target, &self.stopping, &mut |bytes| {
                advance(bytes)
            })
            .await?;
            files.push(target);
        }
        let mut command = self.command(&self.uv());
        command
            .args([
                "pip",
                "install",
                "--no-deps",
                "--offline",
                "--no-index",
                "--python",
            ])
            .arg(self.python())
            .args(&files);
        self.step(command, &mut |_| {}).await?;

        // The kit downloads RVC and its base models itself, and says how far it got.
        let mut kit_files: HashMap<String, u64> = HashMap::new();
        let mut command = self
            .kit()
            .map_err(|Failure(reason)| Interrupted::Failed(format!("{reason:?}")))?;
        command.arg("setup");
        self.step(command, &mut |line| {
            if let Some(KitEvent::Download { file, done, .. }) = events::parse(line) {
                let before = kit_files.insert(file, done).unwrap_or(0);
                advance(done.saturating_sub(before));
            }
        })
        .await?;
        advance(pins::KIT_DOWNLOAD_BYTES);

        // The packages are installed, so their downloads only take room.
        let _ = fs::remove_dir_all(self.wheels());
        let _ = fs::remove_dir_all(self.home.join("cache"));
        let installed = Installed {
            on_card,
            fingerprint: pins::fingerprint(on_card),
            megabytes: pins::installed_megabytes(on_card),
        };
        let text = serde_json::to_string(&installed).expect("tools serialize");
        fs::write(self.home.join("tools.json"), text)?;
        *self.installed_lock() = Some(installed);
        Ok(())
    }

    /// Runs one program of the install, which must end well.
    async fn step(
        &self,
        command: Command,
        on_line: &mut (dyn FnMut(&str) + Send),
    ) -> Result<(), Interrupted> {
        if self.stopped() {
            return Err(Interrupted::Stopped);
        }
        let mut last = Vec::new();
        let status = process::run(command, &self.running, &mut |line| {
            remember(&mut last, line);
            on_line(line);
        })
        .await?;
        if self.stopped() {
            return Err(Interrupted::Stopped);
        }
        if status.success() {
            Ok(())
        } else {
            Err(Interrupted::Failed(last.join(" ")))
        }
    }

    /// Runs one step of the kit, logging its output and reporting its events. Returns the folder
    /// it exported, if it did.
    async fn run_kit(
        &self,
        arguments: &[String],
        log: &mut fs::File,
        progress: &(dyn Fn(TrainingProgress) + Send + Sync),
    ) -> Result<Outcome<Option<PathBuf>>, Failure> {
        let mut command = self.kit()?;
        command.args(arguments);
        let mut error = None;
        let mut exported = None;
        let mut last = Vec::new();
        let status = process::run(command, &self.running, &mut |line| {
            let _ = writeln!(log, "{line}");
            match events::parse(line) {
                Some(KitEvent::Stage(stage)) => progress(TrainingProgress::Stage(stage)),
                Some(KitEvent::Learning { epoch }) => {
                    progress(TrainingProgress::Stage(TrainingStage::Learning));
                    progress(TrainingProgress::Epoch {
                        epoch,
                        seconds: 0.0,
                    });
                }
                Some(KitEvent::Epoch { epoch, seconds }) => {
                    progress(TrainingProgress::Epoch { epoch, seconds });
                }
                Some(KitEvent::Exported(folder)) => exported = Some(PathBuf::from(folder)),
                Some(KitEvent::Error(message)) => error = Some(message),
                Some(KitEvent::Download { .. }) | None => remember(&mut last, line),
            }
        })
        .await
        .map_err(failed)?;
        if self.stopped() {
            return Ok(Outcome::Stopped);
        }
        if !status.success() {
            return Err(Failure(Message::plain(
                error.unwrap_or_else(|| last.join(" ")),
            )));
        }
        Ok(Outcome::Done(exported))
    }
}

/// Keeps the last lines a program wrote, to tell what went wrong.
fn remember(last: &mut Vec<String>, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if last.len() == LAST_LINES {
        last.remove(0);
    }
    last.push(line.to_owned());
}

/// The file name at the end of an address, with the `+` of PyTorch's versions decoded.
fn file_name(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).replace("%2B", "+")
}

/// A name the app made, which never leads out of its folder.
fn is_plain(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn failed(detail: impl ToString) -> Failure {
    Failure(Message::plain(detail.to_string()))
}

#[cfg(target_os = "windows")]
fn unpack_uv(archive: &Path, entry: &str, target: &Path) -> Result<(), Interrupted> {
    let mut archive = zip::ZipArchive::new(fs::File::open(archive)?)?;
    let mut program = archive.by_name(entry)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    std::io::copy(&mut program, &mut fs::File::create(target)?)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn unpack_uv(archive: &Path, entry: &str, target: &Path) -> Result<(), Interrupted> {
    use std::os::unix::fs::PermissionsExt;

    let file = fs::File::open(archive)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for found in archive.entries()? {
        let mut found = found?;
        if found.path()?.as_os_str() == entry {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut found, &mut fs::File::create(target)?)?;
            fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
            return Ok(());
        }
    }
    Err(Interrupted::Failed(format!(
        "{entry} is not in the archive of uv"
    )))
}

/// Deletes everything in `folder` except the entries named in `keep`.
fn remove_all_but(folder: &Path, keep: &[&str]) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        if keep.iter().any(|kept| entry.file_name() == *kept) {
            continue;
        }
        let path = entry.path();
        let _ = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
    }
}

#[async_trait]
impl VoiceTrainer for KitTrainer {
    fn is_supported(&self) -> bool {
        cfg!(all(
            any(target_os = "windows", target_os = "linux"),
            target_arch = "x86_64"
        ))
    }

    fn system(&self) -> System {
        if cfg!(target_os = "windows") {
            System::Windows
        } else {
            System::Linux
        }
    }

    async fn graphics_card(&self) -> Option<GraphicsCard> {
        let ask = tokio::task::spawn_blocking(|| {
            let mut command = Command::new("nvidia-smi");
            command.args([
                "--query-gpu=name,memory.total,driver_version",
                "--format=csv,noheader,nounits",
            ]);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            command.output()
        });
        let output = tokio::time::timeout(CARD_TIMEOUT, ask)
            .await
            .ok()?
            .ok()?
            .ok()?;
        if !output.status.success() {
            return None;
        }
        events::graphics_card(&String::from_utf8_lossy(&output.stdout))
    }

    fn tools(&self, device: TrainingDevice) -> TrainingTools {
        let on_card = device.is_card();
        let is_installed = self.installed_lock().as_ref().is_some_and(|kept| {
            kept.on_card == on_card && kept.fingerprint == pins::fingerprint(on_card)
        }) && self.python().exists();
        let missing = if is_installed {
            0
        } else {
            self.missing_bytes(on_card)
        };
        TrainingTools {
            is_installed,
            download_megabytes: missing.div_ceil(BYTES_PER_MEGABYTE) as u32,
            installed_megabytes: pins::installed_megabytes(on_card),
        }
    }

    fn installed_megabytes(&self) -> u32 {
        self.installed_lock()
            .as_ref()
            .map_or(0, |installed| installed.megabytes)
    }

    fn free_megabytes(&self) -> Option<u64> {
        // The folder may not be there yet: the room is that of the nearest folder that is.
        let existing = self.home.ancestors().find(|folder| folder.exists())?;
        let bytes = fs4::available_space(existing).ok()?;
        Some(bytes / BYTES_PER_MEGABYTE)
    }

    async fn install(
        &self,
        device: TrainingDevice,
        progress: &(dyn Fn(u8) + Send + Sync),
    ) -> Result<Outcome<()>, Failure> {
        self.stopping.store(false, Ordering::Relaxed);
        fs::create_dir_all(&self.home).map_err(failed)?;
        match self.install_tools(device.is_card(), progress).await {
            Ok(()) => Ok(Outcome::Done(())),
            Err(Interrupted::Stopped) => Ok(Outcome::Stopped),
            Err(Interrupted::Failed(reason)) => Err(failed(reason)),
        }
    }

    fn stop(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        process::end(&self.running);
    }

    fn remove_tools(&self) {
        *self.installed_lock() = None;
        for tool in [
            "tools.json",
            "voicekit.py",
            "uv",
            "python",
            "venv",
            "cache",
            "wheels",
            "downloads",
            "bin",
        ] {
            let path = self.home.join(tool);
            let _ = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
        }
        // RVC goes too, except what trainings in progress made with it.
        let rvc = self.home.join("rvc");
        remove_all_but(&rvc, &[EXPERIMENTS, "assets"]);
        remove_all_but(&rvc.join("assets"), &["weights"]);
        let _ = fs::remove_dir_all(rvc.join(EXPERIMENTS).join("mute"));
    }

    async fn keep_take(&self, job: &str, take: &str, speech: Vec<f32>) -> Result<(), Failure> {
        if !is_plain(job) || !is_plain(take) {
            return Err(failed("not a name the app made"));
        }
        let dataset = self.voice(job).join("dataset");
        let take = take.to_owned();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            fs::create_dir_all(&dataset)?;
            // Written aside and moved in once whole, so a crash never leaves half a take.
            let partial = dataset.join(format!("{take}.part"));
            let mut file = std::io::BufWriter::new(fs::File::create(&partial)?);
            wav::write(&mut file, &speech, SPEECH_RATE)?;
            file.into_inner()
                .map_err(|error| error.into_error())?
                .sync_all()?;
            fs::rename(&partial, dataset.join(format!("{take}.wav")))
        })
        .await
        .map_err(failed)?
        .map_err(failed)
    }

    fn remove_take(&self, job: &str, take: &str) {
        if is_plain(job) && is_plain(take) {
            let _ = fs::remove_file(self.voice(job).join("dataset").join(format!("{take}.wav")));
        }
    }

    async fn train(
        &self,
        run: &TrainingRun,
        progress: &(dyn Fn(TrainingProgress) + Send + Sync),
    ) -> Result<Outcome<PathBuf>, Failure> {
        self.stopping.store(false, Ordering::Relaxed);
        if !is_plain(&run.job) {
            return Err(failed("not a name the app made"));
        }
        if !self.tools(run.plan.device).is_installed {
            return Err(failed("the training tools are not installed"));
        }
        let folder = self.voice(&run.job);
        fs::create_dir_all(&folder).map_err(failed)?;
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join("training.log"))
            .map_err(failed)?;

        let plan = run.plan;
        let train = [
            "train".to_owned(),
            "--name".to_owned(),
            run.job.clone(),
            "--epochs".to_owned(),
            plan.epochs.to_string(),
            "--batch".to_owned(),
            plan.batch.to_string(),
            "--save-every".to_owned(),
            plan.save_every.to_string(),
        ];
        if self.run_kit(&train, &mut log, progress).await? == Outcome::Stopped {
            return Ok(Outcome::Stopped);
        }
        let export = [
            "export".to_owned(),
            "--name".to_owned(),
            run.job.clone(),
            "--title".to_owned(),
            run.title.clone(),
        ];
        match self.run_kit(&export, &mut log, progress).await? {
            Outcome::Stopped => Ok(Outcome::Stopped),
            Outcome::Done(exported) => Ok(Outcome::Done(
                exported.unwrap_or_else(|| folder.join("app")),
            )),
        }
    }

    fn discard(&self, job: &str) {
        if !is_plain(job) {
            return;
        }
        let rvc = self.home.join("rvc");
        let _ = fs::remove_dir_all(self.voice(job));
        let _ = fs::remove_dir_all(rvc.join(EXPERIMENTS).join(job));
        let _ = fs::remove_file(rvc.join(WEIGHTS).join(format!("{job}.pth")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_names_the_app_made_reach_the_disk() {
        assert!(is_plain("voice-0a1b"));
        assert!(is_plain("take-2c3d"));
        assert!(!is_plain(""));
        assert!(!is_plain("../tools"));
        assert!(!is_plain("a/b"));
        assert!(!is_plain(r"a\b"));
    }

    #[test]
    fn the_file_of_a_wheel_is_named_as_pip_expects() {
        assert_eq!(
            file_name(
                "https://download-r2.pytorch.org/whl/cu128/torch-2.7.1%2Bcu128-cp312-cp312-win_amd64.whl"
            ),
            "torch-2.7.1+cu128-cp312-cp312-win_amd64.whl"
        );
    }

    #[test]
    fn a_failure_shows_the_last_lines_the_program_wrote() {
        let mut last = Vec::new();
        for line in ["one", "", "two", "three", "four"] {
            remember(&mut last, line);
        }

        assert_eq!(last, ["two", "three", "four"]);
    }
}
