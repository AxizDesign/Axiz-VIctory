//! Share overlapping work for an exact key without retaining completed values.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

/// The index holds weak references. Values are owned only by callers.
/// A failed initializer leaves the slot available for another caller.
pub struct InFlight<V> {
    active: Mutex<HashMap<String, Weak<Mutex<Option<V>>>>>,
}

impl<V: Clone> InFlight<V> {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_or_try_build<E>(
        &self,
        key: &str,
        build: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        let slot = {
            let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(slot) = active.get(key).and_then(Weak::upgrade) {
                slot
            } else {
                // Cull expired slots in batches. A burst of distinct keys is
                // allowed to keep running regardless of unrelated compiles.
                if active.len() >= 64 && active.len().is_multiple_of(64) {
                    active.retain(|_, slot| slot.strong_count() != 0);
                }
                let slot = Arc::new(Mutex::new(None));
                active.insert(key.to_owned(), Arc::downgrade(&slot));
                slot
            }
        };
        let mut value = slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = value.as_ref() {
            return Ok(value.clone());
        }
        let made = build()?;
        *value = Some(made.clone());
        Ok(made)
    }
}

impl<V: Clone> Default for InFlight<V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

    #[test]
    fn overlapping_requests_share_a_value_without_retaining_it() {
        let cache = InFlight::new();
        let builds = AtomicUsize::new(0);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let cache = &cache;
            let builds = &builds;
            let worker = scope.spawn(move || {
                cache
                    .get_or_try_build("pso", || {
                        builds.fetch_add(1, Relaxed);
                        started_tx.send(()).unwrap();
                        go_rx
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                        Ok::<_, ()>(Arc::new(7))
                    })
                    .unwrap()
            });
            started_rx.recv().unwrap();
            let peer = scope.spawn(|| {
                cache
                    .get_or_try_build("pso", || {
                        builds.fetch_add(1, Relaxed);
                        Ok::<_, ()>(Arc::new(8))
                    })
                    .unwrap()
            });
            let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                let n = cache.active.lock().unwrap()["pso"].strong_count();
                if n == 2 {
                    break;
                }
                assert!(std::time::Instant::now() < until, "peer did not attach");
                std::thread::yield_now();
            }
            // Another key is free to build while the first one is blocked.
            assert_eq!(
                *cache
                    .get_or_try_build("compute", || Ok::<_, ()>(Arc::new(3)))
                    .unwrap(),
                3
            );
            go_tx.send(()).unwrap();
            assert!(Arc::ptr_eq(&worker.join().unwrap(), &peer.join().unwrap()));
        });
        assert_eq!(builds.load(Relaxed), 1);
        assert_eq!(cache.active.lock().unwrap()["pso"].strong_count(), 0);
        assert_eq!(
            *cache
                .get_or_try_build("pso", || Ok::<_, ()>(Arc::new(9)))
                .unwrap(),
            9
        );
    }

    #[test]
    fn failed_build_is_retryable() {
        let cache = InFlight::new();
        assert_eq!(cache.get_or_try_build("pso", || Err::<u8, _>(17)), Err(17));
        assert_eq!(cache.get_or_try_build("pso", || Ok::<_, u8>(42)), Ok(42));
    }
}
