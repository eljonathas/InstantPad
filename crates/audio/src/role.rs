//! A thread that owns one device stream. A driver in a bad state can block forever, so every
//! device has its own thread, and a stuck one never holds up the others.

use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use instantpad_application::message::{Failure, TextKey};
use tokio::sync::oneshot;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(8);

type Job<S> = Box<dyn FnOnce(&mut Option<S>) + Send>;

/// The thread the jobs go to now, and which one it is, so a caller that gave up on it replaces
/// it only once.
struct Current<S> {
    jobs: mpsc::Sender<Job<S>>,
    thread: u64,
}

/// `S` is the stream. It is not `Send` on every platform, so it is created, kept and dropped
/// on the thread of the role.
pub(crate) struct Role<S> {
    name: String,
    current: Mutex<Current<S>>,
}

impl<S: 'static> Role<S> {
    pub(crate) fn spawn(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            current: Mutex::new(Current {
                jobs: start(name),
                thread: 0,
            }),
        }
    }

    /// Runs `work` on the thread of the role and waits for it a few seconds. A thread that does
    /// not answer is stuck in the driver, maybe for good, and the next job goes to a new one, so
    /// the user can still choose another device. The stuck thread drops its stream and ends
    /// whenever the driver lets it go.
    pub(crate) async fn run(
        &self,
        work: impl FnOnce(&mut Option<S>) -> Result<(), Failure> + Send + 'static,
    ) -> Result<(), Failure> {
        let not_responding = || Failure::text(TextKey::AudioDeviceNotResponding);

        let (reply, answer) = oneshot::channel();
        let job = Box::new(move |stream: &mut Option<S>| {
            // Whoever asked gave up on it, and a newer job does what is wanted now.
            if reply.is_closed() {
                return;
            }
            let _ = reply.send(work(stream));
        });
        let thread = {
            let current = self.current.lock().unwrap();
            current.jobs.send(job).map_err(|_| not_responding())?;
            current.thread
        };

        match tokio::time::timeout(RESPONSE_TIMEOUT, answer).await {
            Ok(answer) => answer.map_err(|_| not_responding())?,
            Err(_) => {
                self.replace(thread);
                Err(not_responding())
            }
        }
    }

    /// Sends the next jobs to a new thread, unless another caller already did.
    fn replace(&self, stuck: u64) {
        let mut current = self.current.lock().unwrap();
        if current.thread == stuck {
            *current = Current {
                jobs: start(&self.name),
                thread: stuck + 1,
            };
        }
    }
}

/// A thread that runs the jobs in order, and ends, dropping its stream, once nobody can send it
/// any more.
fn start<S: 'static>(name: &str) -> mpsc::Sender<Job<S>> {
    let (jobs, queue) = mpsc::channel::<Job<S>>();
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let mut stream = None;
            for job in queue {
                job(&mut stream);
            }
        })
        .expect("the operating system starts a thread");
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_role_that_does_not_answer_gives_up_without_holding_up_the_others() {
        let stuck = Role::<()>::spawn("stuck");
        let working = Role::<()>::spawn("working");
        let (release, wait) = mpsc::channel::<()>();

        let result = stuck
            .run(move |_| {
                let _ = wait.recv();
                Ok(())
            })
            .await;
        // The clock was only paused to skip the wait, and the other role answers in real time.
        tokio::time::resume();
        let other = working.run(|_| Ok(())).await;

        assert_eq!(
            result,
            Err(Failure::text(TextKey::AudioDeviceNotResponding))
        );
        assert_eq!(other, Ok(()));
        drop(release);
    }

    #[tokio::test(start_paused = true)]
    async fn a_role_stuck_in_the_driver_still_opens_the_next_device() {
        let role = Role::<u32>::spawn("stuck at first");
        let (release, wait) = mpsc::channel::<()>();
        let (ran, stale) = mpsc::channel::<()>();

        // The second job waits behind the first, which the driver holds.
        let (stuck, behind) = tokio::join!(
            role.run(move |stream| {
                let _ = wait.recv();
                *stream = Some(1);
                Ok(())
            }),
            role.run(move |_| {
                let _ = ran.send(());
                Ok(())
            }),
        );
        tokio::time::resume();
        let next = role
            .run(|stream| match stream {
                None => Ok(()),
                Some(_) => Err(Failure::text(TextKey::DeviceUnavailable)),
            })
            .await;

        assert_eq!(stuck, Err(Failure::text(TextKey::AudioDeviceNotResponding)));
        assert_eq!(
            behind,
            Err(Failure::text(TextKey::AudioDeviceNotResponding))
        );
        assert_eq!(next, Ok(()));
        drop(release);
        // The job nobody waits for any more does not run once the driver lets go.
        assert!(stale.recv().is_err());
    }

    #[tokio::test]
    async fn the_thread_keeps_what_a_job_leaves_for_the_next_one() {
        let role = Role::<u32>::spawn("keeper");
        role.run(|state| {
            *state = Some(7);
            Ok(())
        })
        .await
        .unwrap();

        let result = role
            .run(|state| match state {
                Some(7) => Ok(()),
                _ => Err(Failure::text(TextKey::DeviceUnavailable)),
            })
            .await;

        assert_eq!(result, Ok(()));
    }

    #[tokio::test]
    async fn a_failing_job_reports_its_failure() {
        let role = Role::<()>::spawn("failing");

        let result = role
            .run(|_| Err(Failure::because(TextKey::DeviceUnavailable, "gone")))
            .await;

        assert_eq!(
            result,
            Err(Failure::because(TextKey::DeviceUnavailable, "gone"))
        );
    }
}
