//! A thread that owns one device stream. A driver in a bad state can block forever, so every
//! device has its own thread, and a stuck one never holds up the others.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use instantpad_application::message::{Failure, TextKey};
use tokio::sync::oneshot;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(8);

type Job<S> = Box<dyn FnOnce(&mut Option<S>) + Send>;

/// `S` is the stream. It is not `Send` on every platform, so it is created, kept and dropped
/// on the thread of the role.
pub(crate) struct Role<S> {
    jobs: mpsc::Sender<Job<S>>,
}

impl<S: 'static> Role<S> {
    pub(crate) fn spawn(name: &str) -> Self {
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
        Self { jobs }
    }

    /// Runs `work` on the thread of the role and waits for it a few seconds.
    pub(crate) async fn run(
        &self,
        work: impl FnOnce(&mut Option<S>) -> Result<(), Failure> + Send + 'static,
    ) -> Result<(), Failure> {
        let not_responding = || Failure::text(TextKey::AudioDeviceNotResponding);

        let (reply, answer) = oneshot::channel();
        let job = Box::new(move |stream: &mut Option<S>| {
            // The caller gave up on a job that waited behind a stuck one. Running it now would
            // open a stream that nobody reads.
            if reply.is_closed() {
                return;
            }
            let result = work(stream);
            // A caller that gave up while the work ran does not know about the stream it left,
            // and would never close it.
            if reply.send(result).is_err() {
                *stream = None;
            }
        });
        self.jobs.send(job).map_err(|_| not_responding())?;

        tokio::time::timeout(RESPONSE_TIMEOUT, answer)
            .await
            .map_err(|_| not_responding())?
            .map_err(|_| not_responding())?
    }
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
    async fn a_job_the_caller_gave_up_on_is_not_run_later() {
        let role = Role::<()>::spawn("stuck");
        let (release, wait) = mpsc::channel::<()>();
        let (ran, was_run) = mpsc::channel::<()>();
        let _ = role
            .run(move |_| {
                let _ = wait.recv();
                Ok(())
            })
            .await;
        let _ = role
            .run(move |_| {
                let _ = ran.send(());
                Ok(())
            })
            .await;
        tokio::time::resume();

        drop(release);
        role.run(|_| Ok(())).await.unwrap();

        assert!(was_run.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn a_stream_opened_after_the_caller_gave_up_is_closed() {
        let role = Role::<std::sync::Arc<()>>::spawn("late");
        let stream = std::sync::Arc::new(());
        let kept = stream.clone();
        let (release, wait) = mpsc::channel::<()>();
        let result = role
            .run(move |state| {
                let _ = wait.recv();
                *state = Some(stream);
                Ok(())
            })
            .await;
        tokio::time::resume();
        assert!(result.is_err());

        drop(release);
        role.run(|_| Ok(())).await.unwrap();

        assert_eq!(std::sync::Arc::strong_count(&kept), 1);
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
