//! Runs the programs of the training so that stopping one stops everything it started: RVC's
//! training starts processes of its own, which outlive their parent when only it is ended.
//!
//! A program ends in this order: it exits, the processes it left behind are ended with its
//! group, and then what they still wrote is read, for a short while only.

use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use instantpad_application::sync::lock;
use tokio::time::Instant;

/// How long the output of a program that already ended is still read. Everything it wrote is in
/// the pipe by then, unless a process that escaped its group holds the pipe open.
const DRAIN_FOR: Duration = Duration::from_secs(2);
/// The longest line kept. Longer ones are cut, so output without line breaks stays small.
const LONGEST_LINE: usize = 64 * 1024;

/// The processes of one program and everything it started.
pub(crate) struct Group {
    #[cfg(target_os = "windows")]
    job: windows_job::Job,
    #[cfg(not(target_os = "windows"))]
    leader: u32,
    /// The program's input, which only this app holds the writing end of. When the app dies for
    /// any reason the system closes it, and the kit, which watches it, ends its own group. A
    /// Unix process has no job object to do that for it.
    #[cfg(not(target_os = "windows"))]
    _lifeline: Option<std::process::ChildStdin>,
}

impl Group {
    /// Ends every process of the group.
    pub fn end(&self) {
        #[cfg(target_os = "windows")]
        self.job.terminate();
        #[cfg(not(target_os = "windows"))]
        {
            use rustix::process::{Pid, Signal, kill_process_group};

            // The leader's id is the group's. A group with nobody left is already ended, and the
            // only other failure, a missing right, cannot happen to a group this app started.
            if let Some(group) = i32::try_from(self.leader).ok().and_then(Pid::from_raw) {
                let _ = kill_process_group(group, Signal::KILL);
            }
        }
    }
}

/// Where the program that runs now is kept, so another thread can end it.
pub(crate) type Slot = Mutex<Option<Arc<Group>>>;

/// Runs `command` to its end, handing over each line it writes to its output or its errors, and
/// keeps its group in `slot` meanwhile. A program started once `stopping` is set is ended at
/// once: [`end`] found nothing to end if it came before the program was in `slot`.
pub(crate) async fn run(
    mut command: Command,
    slot: &Slot,
    stopping: &AtomicBool,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> io::Result<ExitStatus> {
    // One pipe for both, so the lines come in the order they were written.
    let (reader, writer) = io::pipe()?;
    command.stdout(writer.try_clone()?).stderr(writer);
    prepare(&mut command);
    let mut child = command.spawn()?;
    // The command holds the writing end too, which must close for the reading to end.
    drop(command);
    let group = Arc::new(contain(&mut child)?);
    *lock(slot) = Some(group.clone());
    // Read after the program is in the slot, and `stop` sets it before it looks in the slot, so
    // one of the two always ends the program.
    if stopping.load(Ordering::SeqCst) {
        group.end();
    }

    let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
    let reading = std::thread::spawn(move || read_lines(reader, |line| lines.send(line).is_ok()));
    let mut exit = tokio::task::spawn_blocking(move || child.wait());
    let mut open = true;
    let status = loop {
        tokio::select! {
            // The program ending is what ends the run, not the pipe closing: a process it
            // started may hold the pipe long after it.
            status = &mut exit => break status.map_err(io::Error::other)?,
            line = received.recv(), if open => match line {
                Some(line) => on_line(&line),
                None => open = false,
            },
        }
    };
    // What the program left behind goes with it, which also lets the pipe close.
    group.end();
    let deadline = Instant::now() + DRAIN_FOR;
    while let Ok(Some(line)) = tokio::time::timeout_at(deadline, received.recv()).await {
        on_line(&line);
    }
    // Joined only when the pipe closed, since a process that escaped the group may hold it.
    if reading.is_finished() {
        let _ = reading.join();
    }

    let mut current = lock(slot);
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
    if let Some(group) = lock(slot).take() {
        group.end();
    }
}

/// Hands every line of `reader` to `send`, until the end or until `send` says no. A line ends at
/// `\n` or `\r`, because progress is drawn with a bare `\r`, and empty lines are left out.
fn read_lines(mut reader: impl Read, mut send: impl FnMut(String) -> bool) {
    let mut line = Vec::new();
    let mut block = [0; 8192];
    loop {
        let read = match reader.read(&mut block) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for &byte in &block[..read] {
            let ends = byte == b'\n' || byte == b'\r';
            if !ends {
                line.push(byte);
            }
            if (ends || line.len() >= LONGEST_LINE) && !line.is_empty() {
                if !send(String::from_utf8_lossy(&line).into_owned()) {
                    return;
                }
                line.clear();
            }
        }
    }
    if !line.is_empty() {
        send(String::from_utf8_lossy(&line).into_owned());
    }
}

#[cfg(target_os = "windows")]
fn prepare(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    // A console with no window, which the processes it starts share, so none opens one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn prepare(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // A group of its own, which ends as one, and an input that stays open as long as the app
    // lives (see `Group`).
    command.stdin(Stdio::piped()).process_group(0);
}

/// Puts the program in a job that ends with the app.
///
/// The program is already running when it is assigned, and a process it starts in between is
/// outside the job. Closing that gap takes starting it suspended and resuming its first thread
/// after the assignment, and `std` neither gives the handle of that thread nor takes a job when
/// it creates the process (that is unstable). Finding the thread by walking the system's thread
/// list is code that cannot be tried on a build machine without Windows, and a mistake in it
/// leaves every training suspended, so the short gap is accepted: the programs run here are uv
/// and Python, and what they start comes after they have loaded.
#[cfg(target_os = "windows")]
fn contain(child: &mut std::process::Child) -> io::Result<Group> {
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
fn contain(child: &mut std::process::Child) -> io::Result<Group> {
    Ok(Group {
        leader: child.id(),
        _lifeline: child.stdin.take(),
    })
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

        let status = run(
            shell("echo one&& echo two 1>&2"),
            &slot,
            &AtomicBool::new(false),
            &mut |line| {
                lines.push(line.trim().to_owned());
            },
        )
        .await
        .unwrap();

        assert!(status.success());
        assert_eq!(lines, ["one", "two"]);
        assert!(slot.lock().unwrap().is_none());
    }

    fn long_script() -> &'static str {
        if cfg!(target_os = "windows") {
            "ping -n 30 127.0.0.1 > NUL"
        } else {
            "sleep 30"
        }
    }

    #[tokio::test]
    async fn ending_the_slot_ends_the_program_and_what_it_started() {
        let slot = Arc::new(Mutex::new(None));
        let started = Instant::now();
        let ender = slot.clone();
        tokio::spawn(async move {
            while ender.lock().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            end(&ender);
        });

        let status = run(
            shell(long_script()),
            &slot,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .await
        .unwrap();

        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn a_program_that_starts_after_a_stop_is_ended_at_once() {
        let slot = Mutex::new(None);
        let started = Instant::now();
        // The stop came while nothing was in the slot, so ending it found nothing.
        let stopping = AtomicBool::new(true);
        end(&slot);

        let status = run(shell(long_script()), &slot, &stopping, &mut |_| {})
            .await
            .unwrap();

        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_line_ends_at_a_line_feed_or_a_carriage_return_and_empty_ones_are_left_out() {
        let mut lines = Vec::new();

        read_lines(
            &b"one\r\ntwo\n\nprogress 1%\rprogress 2%\rlast"[..],
            |line| {
                lines.push(line);
                true
            },
        );

        assert_eq!(lines, ["one", "two", "progress 1%", "progress 2%", "last"]);
    }

    #[test]
    fn a_line_without_an_end_is_cut() {
        let mut lines = Vec::new();

        read_lines(&vec![b'a'; LONGEST_LINE + 5][..], |line| {
            lines.push(line.len());
            true
        });

        assert_eq!(lines, [LONGEST_LINE, 5]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_left_behind_does_not_keep_the_run_waiting_and_is_ended() {
        let slot = Mutex::new(None);
        let started = Instant::now();
        let mut lines = Vec::new();

        // The shell ends at once, and the sleep it started keeps the output open.
        let status = run(
            shell("sleep 30 & echo done"),
            &slot,
            &AtomicBool::new(false),
            &mut |line| lines.push(line.to_owned()),
        )
        .await
        .unwrap();

        assert!(status.success());
        assert_eq!(lines, ["done"]);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_program_has_an_input_that_stays_open_until_the_app_lets_go() {
        let slot = Arc::new(Mutex::new(None));
        let ender = slot.clone();
        let running = tokio::spawn(async move {
            // Reads its input to the end, which a closed input would give at once.
            run(shell("cat"), &ender, &AtomicBool::new(false), &mut |_| {}).await
        });

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!running.is_finished());
        end(&slot);

        let status = running.await.unwrap().unwrap();
        assert!(!status.success());
    }
}
