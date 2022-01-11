//! Worker pool for handling events from the X server and user actions
use crate::v3::{
    bindings::{KeyBindings, KeyCode, MouseBindings, MouseEvent},
    error::ErrorHandler,
    handle::WmHandle,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::{fmt, thread};
use tracing::trace;

#[derive(Debug)]
enum Message {
    Key(KeyCode),
    Mouse(MouseEvent),
    ShutDown,
}

struct Worker {
    id: usize,
    handle: thread::JoinHandle<()>,
}

impl fmt::Debug for Worker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Worker").field("id", &self.id).finish()
    }
}

impl Worker {
    fn new(
        id: usize,
        rx: Receiver<Message>,
        h: WmHandle,
        ks: KeyBindings,
        ms: MouseBindings,
        error_handler: ErrorHandler,
    ) -> Self {
        let handle = thread::spawn(move || {
            while let Ok(m) = rx.recv() {
                match m {
                    Message::Key(k) => {
                        if let Some(action) = ks.get_mut(&k) {
                            if let Err(e) = action(h.clone()) {
                                error_handler(e);
                            }
                        }
                    }

                    Message::Mouse(e) => {
                        if let Some(action) = ms.get_mut(&(e.kind, e.state.clone())) {
                            if let Err(e) = action(h.clone(), &e) {
                                error_handler(e);
                            }
                        }
                    }

                    Message::ShutDown => {
                        trace!(id, "Shutting down");
                        return;
                    }
                }
            }
        });

        Self { id, handle }
    }
}

/// A worker pool for running jobs
#[derive(Debug)]
pub struct Pool {
    workers: Vec<Worker>,
    tx: Sender<Message>,
}

impl Pool {
    /// Create a new worker pool with 'size' workers.
    ///
    /// # Panics
    ///
    /// Panics if size == 0
    pub fn new(
        size: usize,
        h: WmHandle,
        ks: KeyBindings,
        ms: MouseBindings,
        error_handler: ErrorHandler,
    ) -> Self {
        assert!(size > 0, "attempt to create empty worker pool");

        let (tx, rx) = unbounded();
        let workers = (0..size)
            .map(|id| {
                Worker::new(
                    id,
                    rx.clone(),
                    h.clone(),
                    ks.clone(),
                    ms.clone(),
                    error_handler,
                )
            })
            .collect();

        Self { workers, tx }
    }

    /// Execute a function on the first available worker
    pub fn exec<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        // TODO: should be returning an error from this method
        self.tx.send(Message::Job(Box::new(f))).unwrap()
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        trace!("Sending shutdown signal to all workers");
        for _ in &self.workers {
            self.tx.send(Message::ShutDown).unwrap(); // TODO: remove unwrap
        }

        for w in self.workers.drain(0..) {
            trace!(w.id, "shutting down worker");
            w.handle.join().unwrap(); // TODO: remove unwrap
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn work_gets_done() {
        let (tx, rx) = unbounded();
        let p = Pool::new(2);

        for n in 0..10 {
            let ch = tx.clone();
            p.exec(move || {
                ch.send(n).unwrap();
            });
        }

        let mut nums = Vec::with_capacity(10);
        for _ in 0..10 {
            nums.push(rx.recv().unwrap());
        }

        nums.sort();
        assert_eq!(nums, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}