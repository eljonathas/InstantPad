//! Locks whose data stays usable after a panic.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, and goes on with its data if a thread panicked while holding it.
///
/// What the app keeps behind its locks is plain data that every change leaves whole, so a panic
/// elsewhere is no reason to make every later call panic too.
pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::lock;

    #[test]
    fn a_panic_under_the_lock_leaves_the_data_reachable() {
        let mutex = Mutex::new(1);
        let _ = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("while holding the lock");
        });
        assert!(mutex.is_poisoned());
        assert_eq!(*lock(&mutex), 1);
    }
}
