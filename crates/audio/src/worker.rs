//! A thread that sleeps until there is something for it to do.

use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use instantpad_application::sync::lock;

pub(crate) struct Worker {
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Worker {
    pub(crate) fn new() -> Self {
        Self {
            thread: Mutex::new(None),
        }
    }

    /// Wakes the thread. When there is none, or the last one ended by panicking, `body` starts
    /// one, and `body` is expected to sleep with `thread::park` when it has nothing to do and to
    /// return when what it serves is gone. Otherwise `body` is not used.
    pub(crate) fn wake(&self, name: &str, body: impl FnOnce() + Send + 'static) {
        let mut slot = lock(&self.thread);
        if slot.as_ref().is_none_or(JoinHandle::is_finished) {
            *slot = Some(
                thread::Builder::new()
                    .name(name.to_owned())
                    .spawn(body)
                    .expect("the operating system starts a thread"),
            );
        }
        if let Some(handle) = slot.as_ref() {
            handle.thread().unpark();
        }
    }
}

impl Drop for Worker {
    /// A sleeping thread would never see that what it serves is gone.
    fn drop(&mut self) {
        let slot = self
            .thread
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = slot {
            handle.thread().unpark();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_thread_that_panicked_is_replaced_when_woken_again() {
        let worker = Worker::new();
        worker.wake("test", || panic!("the thread ends"));
        // Waits for the first thread to be over.
        while lock(&worker.thread)
            .as_ref()
            .is_some_and(|t| !t.is_finished())
        {
            thread::yield_now();
        }

        let (ran, was_run) = mpsc::channel();
        worker.wake("test", move || ran.send(()).unwrap());

        assert!(was_run.recv_timeout(Duration::from_secs(10)).is_ok());
    }
}
