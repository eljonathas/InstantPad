//! Runs the programs of the training so that stopping one stops everything it started: RVC's
//! training starts processes of its own, which outlive their parent when only it is ended.

use std::io::{self, BufRead, BufReader};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

/// The processes of one program and everything it started.
pub(crate) struct Group {
    #[cfg(target_os = "windows")]
    job: windows_job::Job,
    #[cfg(not(target_os = "windows"))]
    leader: u32,
}

impl Group {
    /// Ends every process of the group.
    pub fn end(&self) {
        #[cfg(target_os = "windows")]
        self.job.terminate();
        #[cfg(not(target_os = "windows"))]
        {
            // The leader's id is the group's, and a minus sends to the whole group. Without the
            // `--`, kill takes the minus for an option.
            let _ = Command::new("kill")
                .args(["-s", "KILL", "--", &format!("-{}", self.leader)])
                .status();
        }
    }
}

/// Where the program that runs now is kept, so another thread can end it.
pub(crate) type Slot = Mutex<Option<Arc<Group>>>;

/// Runs `command` to its end, handing over each line it writes to its output or its errors, and
/// keeps its group in `slot` meanwhile.
pub(crate) async fn run(
    mut command: Command,
    slot: &Slot,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> io::Result<ExitStatus> {
    // One pipe for both, so the lines come in the order they were written.
    let (reader, writer) = io::pipe()?;
    command
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer);
    hide(&mut command);
    let mut child = command.spawn()?;
    // The command holds the writing end too, which must close for the reading to end.
    drop(command);
    let group = Arc::new(contain(&child)?);
    *slot.lock().expect("process lock poisoned") = Some(group.clone());

    let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in BufReader::new(reader).split(b'\n') {
            let Ok(line) = line else {
                break;
            };
            let text = String::from_utf8_lossy(&line)
                .trim_end_matches('\r')
                .to_owned();
            if lines.send(text).is_err() {
                break;
            }
        }
    });
    while let Some(line) = received.recv().await {
        on_line(&line);
    }
    let status = tokio::task::spawn_blocking(move || child.wait())
        .await
        .map_err(io::Error::other)?;

    let mut current = slot.lock().expect("process lock poisoned");
    if current
        .as_ref()
        .is_some_and(|kept| Arc::ptr_eq(kept, &group))
    {
        *current = None;
    }
    status
}

/// Ends the program in `slot` and everything it started, if one runs.
pub(crate) fn end(slot: &Slot) {
    if let Some(group) = slot.lock().expect("process lock poisoned").take() {
        group.end();
    }
}

#[cfg(target_os = "windows")]
fn hide(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    // A console with no window, which the processes it starts share, so none opens one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // A group of its own, which ends as one.
    command.process_group(0);
}

#[cfg(target_os = "windows")]
fn contain(child: &std::process::Child) -> io::Result<Group> {
    let job = windows_job::Job::new()?;
    if let Err(error) = job.assign(child) {
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .status();
        return Err(error);
    }
    Ok(Group { job })
}

#[cfg(not(target_os = "windows"))]
fn contain(child: &std::process::Child) -> io::Result<Group> {
    Ok(Group { leader: child.id() })
}

/// A job object, which ends its processes when it is closed, so they also end with the app.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod windows_job {
    use std::io;
    use std::os::windows::io::AsRawHandle;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows::core::PCWSTR;

    /// The handle as a number, which is what makes it safe to share between threads: only these
    /// functions turn it back into a handle.
    pub(super) struct Job(isize);

    impl Job {
        pub fn new() -> io::Result<Self> {
            // SAFETY: no attributes and no name; the handle is closed in `drop`.
            let handle =
                unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(io::Error::other)?;
            let job = Self(handle.0 as isize);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: the structure lives through the call and its size is the one passed.
            unsafe {
                SetInformationJobObject(
                    job.handle(),
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            }
            .map_err(io::Error::other)?;
            Ok(job)
        }

        pub fn assign(&self, child: &std::process::Child) -> io::Result<()> {
            let process = HANDLE(child.as_raw_handle());
            // SAFETY: both handles are open: the child is borrowed, and the job is owned.
            unsafe { AssignProcessToJobObject(self.handle(), process) }.map_err(io::Error::other)
        }

        pub fn terminate(&self) {
            // SAFETY: the job's handle stays open until `drop`.
            let _ = unsafe { TerminateJobObject(self.handle(), 1) };
        }

        fn handle(&self) -> HANDLE {
            HANDLE(self.0 as *mut std::ffi::c_void)
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from `CreateJobObjectW` and is closed only here.
            let _ = unsafe { CloseHandle(self.handle()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn shell(script: &str) -> Command {
        if cfg!(target_os = "windows") {
            let mut command = Command::new("cmd");
            command.args(["/C", script]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            command
        }
    }

    #[tokio::test]
    async fn the_output_and_the_errors_come_as_lines() {
        let slot = Mutex::new(None);
        let mut lines = Vec::new();

        let status = run(shell("echo one&& echo two 1>&2"), &slot, &mut |line| {
            lines.push(line.trim().to_owned());
        })
        .await
        .unwrap();

        assert!(status.success());
        assert_eq!(lines, ["one", "two"]);
        assert!(slot.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn ending_the_slot_ends_the_program_and_what_it_started() {
        let slot = Arc::new(Mutex::new(None));
        let started = Instant::now();
        let script = if cfg!(target_os = "windows") {
            "ping -n 30 127.0.0.1 > NUL"
        } else {
            "sleep 30"
        };
        let ender = slot.clone();
        tokio::spawn(async move {
            while ender.lock().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            end(&ender);
        });

        let status = run(shell(script), &slot, &mut |_| {}).await.unwrap();

        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
