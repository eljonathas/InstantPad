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

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{
    Outcome, TrainingProgress, TrainingRun, TrainingStage, TrainingTools, VoiceTrainer,
};
use instantpad_application::sync::lock;
use instantpad_domain::training::{GraphicsCard, System, TrainingDevice};
use serde::{Deserialize, Serialize};

use instantpad_download::Progress;

use crate::download::{Interrupted, client, fetch, is_there};
use crate::events::KitEvent;

const BYTES_PER_MEGABYTE: u64 = 1_000_000;
/// The rate of the speech the app hands over, which the takes keep.
const SPEECH_RATE: u32 = 48_000;
/// A log longer than this is set aside when the next training opens it.
const LOG_LIMIT_BYTES: u64 = 4_000_000;
/// Not measured yet, as `leftover_megabytes` has it.
const UNMEASURED: u32 = u32::MAX;
/// `nvidia-smi` answers at once, unless the driver is in a bad state.
const CARD_TIMEOUT: Duration = Duration::from_secs(10);
/// Where RVC keeps what it makes of a training, under `rvc`.
const EXPERIMENTS: &str = "logs";
const WEIGHTS: &str = "assets/weights";
/// What a failure shows of the output of the kit, when it did not say what went wrong.
const LAST_LINES: usize = 3;
/// Windows loads no library from a path longer than 260 characters, and the deepest library of
/// the tools is 120 characters into the folder.
#[cfg(target_os = "windows")]
const MAX_HOME_CHARACTERS: usize = 130;
/// The tools in the folder, besides RVC, which `remove_tools` deletes.
const TOOLS: [&str; 9] = [
    "tools.json",
    "voicekit.py",
    "uv",
    "python",
    "venv",
    "cache",
    "wheels",
    "downloads",
    "bin",
];

/// What is installed, kept in `tools.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Installed {
    on_card: bool,
    /// Of the pinned packages, so new pins install again.
    fingerprint: String,
    /// Of uv, so a new pin replaces the one installed.
    #[serde(default)]
    uv: String,
    megabytes: u32,
}

pub struct KitTrainer {
    home: PathBuf,
    /// Set by `stop`, and cleared when work starts.
    stopping: AtomicBool,
    running: process::Slot,
    installed: Mutex<Option<Installed>>,
    /// What an install that stopped or failed left on disk, which removing the tools frees.
    /// Measured when first asked, since walking a partial install delays the start of the app.
    leftover_megabytes: AtomicU32,
}

impl KitTrainer {
    pub fn new(home: &Path) -> Self {
        let installed: Option<Installed> = fs::read_to_string(home.join("tools.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        let leftover = if installed.is_some() { 0 } else { UNMEASURED };
        Self {
            home: home.to_owned(),
            stopping: AtomicBool::new(false),
            running: Mutex::new(None),
            installed: Mutex::new(installed),
            leftover_megabytes: AtomicU32::new(leftover),
        }
    }

    /// Measures what an install left, once it ended without the tools.
    async fn measure_leftover(&self) {
        let home = self.home.clone();
        let megabytes = tokio::task::spawn_blocking(move || tools_on_disk(&home))
            .await
            .unwrap_or(0);
        self.leftover_megabytes.store(megabytes, Ordering::Relaxed);
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

    /// A command with the folders of the training, and none of the settings of a Python, pip,
    /// uv or loader the user may have.
    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command.env_clear();
        for (name, value) in std::env::vars_os() {
            if name.to_str().is_some_and(passes_to_programs) {
                command.env(name, value);
            }
        }
        command
            .current_dir(&self.home)
            .env("INSTANTPAD_VOICE_HOME", &self.home)
            .env("UV_PYTHON_INSTALL_DIR", self.home.join("python"))
            .env("UV_CACHE_DIR", self.home.join("cache"))
            .env("UV_NO_CONFIG", "1")
            .env("UV_NO_CACHE", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8");
        command
    }

    async fn kit(&self) -> Result<Command, Failure> {
        let kit = self.home.join("voicekit.py");
        tokio::fs::write(&kit, pins::VOICEKIT)
            .await
            .map_err(failed)?;
        let mut command = self.command(&self.python());
        command.arg(kit).arg("--events");
        // The kit ends its own group when the app dies: see `process::Group`.
        #[cfg(not(target_os = "windows"))]
        command.arg("--end-with-input");
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
        let uv = if self.uv_is_current() {
            0
        } else {
            pins::uv().file.size
        };
        let python = if self.python_is_current(on_card) {
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
        lock(&self.installed)
    }

    /// Whether the uv on disk is the pinned one. One from an earlier install is replaced when
    /// the app pins another.
    fn uv_is_current(&self) -> bool {
        self.uv().exists()
            && self
                .installed_lock()
                .as_ref()
                .is_none_or(|kept| kept.uv == pins::uv_id())
    }

    /// Whether the Python of the venv is the one the packages for the device pin.
    fn python_is_current(&self, on_card: bool) -> bool {
        self.python().exists()
            && self
                .installed_lock()
                .as_ref()
                .is_none_or(|kept| kept.fingerprint == pins::fingerprint(on_card))
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
        let mut advance = Progress::new(progress, total);
        let http = client()?;

        // What a new pin changes is installed again: the interpreter of a venv made by another
        // Python, or a uv of another version.
        let stale = self.installed_lock().as_ref().map(|kept| {
            (
                kept.uv != pins::uv_id(),
                kept.fingerprint != pins::fingerprint(on_card),
            )
        });
        if let Some((new_uv, new_packages)) = stale {
            let home = self.home.clone();
            tokio::task::spawn_blocking(move || {
                if new_uv {
                    let _ = fs::remove_dir_all(home.join("uv"));
                }
                if new_packages {
                    let _ = fs::remove_dir_all(home.join("venv"));
                }
            })
            .await
            .map_err(|error| Interrupted::Failed(error.to_string()))?;
        }

        if !self.uv().exists() {
            let archive = self.home.join("downloads").join(file_name(&uv.file.url));
            fetch(&http, &uv.file, &archive, &self.stopping, &mut |bytes| {
                advance.advance(bytes)
            })
            .await?;
            let target = self.uv();
            tokio::task::spawn_blocking(move || unpack_uv(&archive, uv.entry, &target))
                .await
                .map_err(|error| Interrupted::Failed(error.to_string()))??;
        } else {
            advance.advance(uv.file.size);
        }

        if !self.python().exists() {
            let mut command = self.command(&self.uv());
            command
                .args(["venv", "--managed-python", "--no-project", "--python"])
                .arg(&environment.python)
                .arg(self.home.join("venv"));
            self.step(command, &mut |_| {}).await?;
        }
        advance.advance(pins::PYTHON_BYTES);

        let mut files = Vec::new();
        for wheel in &environment.wheels {
            let target = self.wheels().join(file_name(&wheel.url));
            fetch(&http, wheel, &target, &self.stopping, &mut |bytes| {
                advance.advance(bytes)
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
            .await
            .map_err(|Failure(reason)| Interrupted::Failed(format!("{reason:?}")))?;
        command.arg("setup");
        self.step(command, &mut |line| {
            if let Some(KitEvent::Download { file, done, .. }) = events::parse(line) {
                let before = kit_files.insert(file, done).unwrap_or(0);
                advance.advance(done.saturating_sub(before));
            }
        })
        .await?;
        advance.advance(pins::KIT_DOWNLOAD_BYTES);

        let installed = Installed {
            on_card,
            fingerprint: pins::fingerprint(on_card),
            uv: pins::uv_id(),
            megabytes: pins::installed_megabytes(on_card),
        };
        let text = serde_json::to_string(&installed).expect("tools serialize");
        let home = self.home.clone();
        // Deleting gigabytes is no work for a thread that serves other tasks. The packages are
        // installed, so their downloads only take room.
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            let _ = fs::remove_dir_all(home.join("wheels"));
            let _ = fs::remove_dir_all(home.join("cache"));
            fs::write(home.join("tools.json"), text)
        })
        .await
        .map_err(|error| Interrupted::Failed(error.to_string()))??;
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
        let mut last = VecDeque::new();
        let status = process::run(command, &self.running, &self.stopping, &mut |line| {
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
            Err(Interrupted::Failed(join(&last)))
        }
    }

    /// Runs one step of the kit, logging its output and reporting its events. Returns the folder
    /// it exported, if it did.
    async fn run_kit(
        &self,
        arguments: &[String],
        log: &mut BufWriter<fs::File>,
        progress: &(dyn Fn(TrainingProgress) + Send + Sync),
    ) -> Result<Outcome<Option<PathBuf>>, Failure> {
        if self.stopped() {
            return Ok(Outcome::Stopped);
        }
        let mut command = self.kit().await?;
        command.args(arguments);
        let mut error = None;
        let mut exported = None;
        let mut last = VecDeque::new();
        let status = process::run(command, &self.running, &self.stopping, &mut |line| {
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
        .map_err(failed);
        // Whatever the kit wrote is in the log, whether it ended well or not.
        let _ = log.flush();
        let status = status?;
        if self.stopped() {
            return Ok(Outcome::Stopped);
        }
        if !status.success() {
            return Err(Failure(Message::plain(
                error.unwrap_or_else(|| join(&last)),
            )));
        }
        Ok(Outcome::Done(exported))
    }
}

/// Keeps the last lines a program wrote, to tell what went wrong.
fn remember(last: &mut VecDeque<String>, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if last.len() == LAST_LINES {
        last.pop_front();
    }
    last.push_back(line.to_owned());
}

fn join(lines: &VecDeque<String>) -> String {
    lines
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a variable of the app's environment reaches the programs of the training. All of it
/// does, which is what Windows, the driver of the card and a proxy need, except what would point
/// Python, pip, uv or the loader somewhere else than the folders the app installed.
fn passes_to_programs(name: &str) -> bool {
    const PREFIXES: [&str; 6] = [
        "PYTHON",
        "PIP_",
        "UV_",
        "CONDA",
        "VIRTUAL_ENV",
        "LD_PRELOAD",
    ];
    let name = name.to_ascii_uppercase();
    !PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// The file name at the end of an address, with the `+` of PyTorch's versions decoded.
fn file_name(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).replace("%2B", "+")
}

/// The folder the kit exported, which is trusted only where the kit puts it. Any line of RVC's
/// output can look like the kit's event, and the folder is added to the voices afterwards.
fn exported_folder(training: &Path, reported: Option<PathBuf>) -> PathBuf {
    let expected = training.join("app");
    match reported {
        Some(reported) if reported == expected => reported,
        _ => expected,
    }
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
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    instantpad_download::extract_zip_entry(archive, entry, None, target)
}

#[cfg(target_os = "linux")]
fn unpack_uv(archive: &Path, entry: &str, target: &Path) -> Result<(), Interrupted> {
    use std::os::unix::fs::PermissionsExt;

    use instantpad_download::write_atomic;

    let file = fs::File::open(archive)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for found in archive.entries()? {
        let mut found = found?;
        if found.path()?.as_os_str() == entry {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            // Whole and executable before it takes the place of uv, so a stopped copy is never
            // taken for the program.
            write_atomic(target, |program| {
                std::io::copy(&mut found, program)?;
                program.set_permissions(fs::Permissions::from_mode(0o755))
            })?;
            return Ok(());
        }
    }
    Err(Interrupted::Failed(format!(
        "{entry} is not in the archive of uv"
    )))
}

/// Other systems do not train in the app (see `is_supported`), but the app still builds there.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn unpack_uv(_archive: &Path, _entry: &str, _target: &Path) -> Result<(), Interrupted> {
    Err(Interrupted::Failed(
        "this system does not train voices".to_owned(),
    ))
}

/// Opens the log of a training to append to. A long one is set aside first, so a training that
/// is resumed again and again does not grow it without end.
fn open_log(folder: &Path) -> std::io::Result<fs::File> {
    let log = folder.join("training.log");
    if fs::metadata(&log).is_ok_and(|kept| kept.len() > LOG_LIMIT_BYTES) {
        let _ = fs::rename(&log, folder.join("training.log.old"));
    }
    fs::OpenOptions::new().create(true).append(true).open(log)
}

/// The bytes of a file, or of everything in a folder. Links are not followed: a venv links to
/// its Python.
fn disk_bytes(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries
            .flatten()
            .map(|entry| disk_bytes(&entry.path()))
            .sum()
    })
}

/// The bytes of everything in `folder` except the entries named in `keep`.
fn disk_bytes_but(folder: &Path, keep: &[&str]) -> u64 {
    fs::read_dir(folder).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|entry| !keep.iter().any(|kept| entry.file_name() == *kept))
            .map(|entry| disk_bytes(&entry.path()))
            .sum()
    })
}

/// What `remove_tools` would free in `home`, in megabytes.
fn tools_on_disk(home: &Path) -> u32 {
    let rvc = home.join("rvc");
    let bytes = TOOLS
        .iter()
        .map(|tool| disk_bytes(&home.join(tool)))
        .sum::<u64>()
        + disk_bytes_but(&rvc, &[EXPERIMENTS, "assets"])
        + disk_bytes_but(&rvc.join("assets"), &["weights"])
        + disk_bytes(&rvc.join(EXPERIMENTS).join("mute"));
    u32::try_from(bytes.div_ceil(BYTES_PER_MEGABYTE)).unwrap_or(u32::MAX)
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
        let mut command = tokio::process::Command::new("nvidia-smi");
        command
            .args([
                "--query-gpu=name,memory.total,driver_version",
                "--format=csv,noheader,nounits",
            ])
            .stdin(std::process::Stdio::null())
            // A driver in a bad state may never answer, and the program must not outlive the wait.
            .kill_on_drop(true);
        #[cfg(target_os = "windows")]
        command.creation_flags(0x0800_0000);
        let output = tokio::time::timeout(CARD_TIMEOUT, command.output())
            .await
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
        if let Some(installed) = self.installed_lock().as_ref() {
            return installed.megabytes;
        }
        if self.leftover_megabytes.load(Ordering::Relaxed) == UNMEASURED {
            self.leftover_megabytes
                .store(tools_on_disk(&self.home), Ordering::Relaxed);
        }
        self.leftover_megabytes.load(Ordering::Relaxed)
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
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStrExt;

            // Windows counts the units of UTF-16, not the bytes of the name.
            let length = self.home.as_os_str().encode_wide().count();
            if length > MAX_HOME_CHARACTERS {
                return Err(failed(format!(
                    "the folder {} is {length} characters long, and Windows needs it under \
                     {MAX_HOME_CHARACTERS}",
                    self.home.display()
                )));
            }
        }
        fs::create_dir_all(&self.home).map_err(failed)?;
        let installed = self.install_tools(device.is_card(), progress).await;
        if installed.is_ok() {
            self.leftover_megabytes.store(0, Ordering::Relaxed);
        } else {
            // What downloaded so far stays for the next try, and can be removed meanwhile.
            self.measure_leftover().await;
        }
        match installed {
            Ok(()) => Ok(Outcome::Done(())),
            Err(Interrupted::Stopped) => Ok(Outcome::Stopped),
            Err(Interrupted::Failed(reason)) => Err(failed(reason)),
        }
    }

    fn stop(&self) {
        // Set before looking in the slot: see `process::run`.
        self.stopping.store(true, Ordering::SeqCst);
        process::end(&self.running);
    }

    fn remove_tools(&self) {
        *self.installed_lock() = None;
        for tool in TOOLS {
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
        // Whatever could not be deleted, such as a file another program holds open.
        self.leftover_megabytes
            .store(tools_on_disk(&self.home), Ordering::Relaxed);
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
            let written = (|| {
                let mut file = std::io::BufWriter::new(fs::File::create(&partial)?);
                wav::write(&mut file, &speech, SPEECH_RATE)?;
                file.into_inner()
                    .map_err(|error| error.into_error())?
                    .sync_all()?;
                fs::rename(&partial, dataset.join(format!("{take}.wav")))
            })();
            // A take that could not be written, such as on a full disk, leaves nothing behind.
            if written.is_err() {
                let _ = fs::remove_file(&partial);
            }
            written
        })
        .await
        .map_err(|error| Failure::because(TextKey::SaveFailed, error))?
        .map_err(|error| Failure::because(TextKey::SaveFailed, error))
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
        let mut log = BufWriter::new(open_log(&folder).map_err(failed)?);

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
            // One argument, so a title that starts with a dash is not taken for an option.
            format!("--title={}", run.title),
        ];
        match self.run_kit(&export, &mut log, progress).await? {
            Outcome::Stopped => Ok(Outcome::Stopped),
            Outcome::Done(exported) => Ok(Outcome::Done(exported_folder(&folder, exported))),
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
        let mut last = VecDeque::new();
        for line in ["one", "", "two", "three", "four"] {
            remember(&mut last, line);
        }

        assert_eq!(join(&last), "two three four");
    }

    #[test]
    fn an_exported_folder_outside_the_training_is_not_believed() {
        let training = Path::new("/home/voices/voice-1");

        assert_eq!(exported_folder(training, None), training.join("app"));
        assert_eq!(
            exported_folder(training, Some(PathBuf::from("/home/voices/voice-1/app"))),
            training.join("app")
        );
        assert_eq!(
            exported_folder(training, Some(PathBuf::from("/elsewhere/app"))),
            training.join("app")
        );
    }

    #[test]
    fn a_log_that_grew_too_long_is_set_aside_and_a_short_one_is_appended_to() {
        let folder = tempfile::tempdir().unwrap();
        let log = folder.path().join("training.log");
        fs::write(&log, "short").unwrap();
        writeln!(open_log(folder.path()).unwrap(), "more").unwrap();
        assert_eq!(fs::read_to_string(&log).unwrap(), "shortmore\n");

        fs::write(&log, vec![b'x'; LOG_LIMIT_BYTES as usize + 1]).unwrap();
        writeln!(open_log(folder.path()).unwrap(), "fresh").unwrap();

        assert_eq!(fs::read_to_string(&log).unwrap(), "fresh\n");
        assert!(folder.path().join("training.log.old").exists());
    }

    #[test]
    fn programs_get_the_environment_without_the_settings_of_python_pip_uv_or_the_loader() {
        for needed in [
            "PATH",
            "Path",
            "SystemRoot",
            "TEMP",
            "HTTPS_PROXY",
            "CUDA_VISIBLE_DEVICES",
            "LC_ALL",
            "ProgramFiles",
            "XDG_CACHE_HOME",
        ] {
            assert!(passes_to_programs(needed), "{needed}");
        }
        for unwanted in [
            "PYTHONPATH",
            "PYTHONHOME",
            "VIRTUAL_ENV",
            "UV_PYTHON_INSTALL_MIRROR",
            "LD_PRELOAD",
            "PYTHONSTARTUP",
            "PIP_INDEX_URL",
            "CONDA_PREFIX",
        ] {
            assert!(!passes_to_programs(unwanted), "{unwanted}");
        }
    }

    fn tools_json(home: &Path, uv: &str, fingerprint: &str) {
        let installed = Installed {
            on_card: false,
            fingerprint: fingerprint.to_owned(),
            uv: uv.to_owned(),
            megabytes: 1,
        };
        fs::write(
            home.join("tools.json"),
            serde_json::to_string(&installed).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn a_uv_and_a_python_from_other_pins_are_not_current() {
        let home = tempfile::tempdir().unwrap();
        let trainer_at = |uv: &str, fingerprint: &str| {
            tools_json(home.path(), uv, fingerprint);
            KitTrainer::new(home.path())
        };
        let make = |trainer: &KitTrainer| {
            for program in [trainer.uv(), trainer.python()] {
                fs::create_dir_all(program.parent().unwrap()).unwrap();
                fs::write(program, "").unwrap();
            }
        };

        let current = trainer_at(&pins::uv_id(), &pins::fingerprint(false));
        make(&current);
        assert!(current.uv_is_current());
        assert!(current.python_is_current(false));

        let old = trainer_at("0.1.0-old", "old");
        assert!(!old.uv_is_current());
        assert!(!old.python_is_current(false));
        // The size to download counts the uv that is not the pinned one.
        assert!(old.missing_bytes(false) >= pins::uv().file.size + pins::PYTHON_BYTES);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn uv_is_unpacked_whole_and_executable_or_not_at_all() {
        use std::os::unix::fs::PermissionsExt;

        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("uv.tar.gz");
        let mut tarball = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let content = b"#!/bin/sh\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tarball
            .append_data(&mut header, "uv-dir/uv", &content[..])
            .unwrap();
        fs::write(&archive, tarball.into_inner().unwrap().finish().unwrap()).unwrap();
        let target = folder.path().join("tools/uv");

        unpack_uv(&archive, "uv-dir/uv", &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), content);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(!folder.path().join("tools/uv.part").exists());

        let other = folder.path().join("tools/other");
        assert!(unpack_uv(&archive, "uv-dir/missing", &other).is_err());
        assert!(!other.exists());
    }
}
