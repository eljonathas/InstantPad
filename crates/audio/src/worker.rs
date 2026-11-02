//! A thread that sleeps until there is something for it to do.

use std::sync::Mutex;
use std::thread::{self, Thread};

pub(crate) struct Worker {
    thread: Mutex<Option<Thread>>,
}

impl Worker {
    pub(crate) fn new() -> Self {
        Self {
            thread: Mutex::new(None),
        }
    }

    /// Wakes the thread. The first time, `body` starts it, and `body` is expected to sleep with
    /// `thread::park` when it has nothing to do and to return when what it serves is gone.
    pub(crate) fn wake(&self, name: &str, body: impl FnOnce() + Send + 'static) {
        self.thread
            .lock()
            .unwrap()
            .get_or_insert_with(|| {
                thread::Builder::new()
                    .name(name.to_owned())
                    .spawn(body)
                    .expect("the operating system starts a thread")
                    .thread()
                    .clone()
            })
            .unpark();
    }
}

impl Drop for Worker {
    /// A sleeping thread would never see that what it serves is gone.
    fn drop(&mut self) {
        if let Some(thread) = self.thread.get_mut().unwrap() {
            thread.unpark();
        }
    }
}
