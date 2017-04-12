//! An abstraction for spawning futures
//!
//! [futures-rs](http://github.com/alexcrichton/futures-rs) provides a task
//! abstraction and the ability for custom executors to manage how future
//! execution is scheduled across them.
//!
//! `futures-spawn` provides an abstraction representing the act of spawning a
//! future. This enables writing code that is not hard coded to a specific
//! executor.

#![deny(warnings, missing_docs)]

extern crate core;
extern crate futures;

use futures::Future;
use core::result;

/// Value that can spawn a future
///
/// On spawn, the executor takes ownership of the future and becomes responsible
/// to call `Future::poll()` whenever a readiness notification is raised.
pub trait Spawn<T: Future<Item = (), Error = ()>> {

    /// Spawns a future to run on this `Spawn`.
    ///
    /// This function will return immediately, and schedule the future `f` to
    /// run on `self`. The details of scheduling and execution are left to the
    /// implementations of `Spawn`.
    fn spawn_detached(&self, f: T) -> Result<(), T>;
}

/// An error returned from the `spawn_detached` function.
///
/// A `spawn` operation should only fail if the future executor has shutdown and
/// is no longer able to schedule new futures or the executor is at capacity.
#[derive(Debug)]
pub struct Error<T> {
    task: T,
    kind: ErrorKind,
}

/// The possible reasons for a spawn to fail.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ErrorKind {
    /// The executor is shutdown
    Shutdown,
    /// The executor has no more capacity
    NoCapacity,
}

/// The return type of a `Spawn::spawn_detached` method, indicating the outcome
/// of `spawn` attempt.
pub type Result<T, U> = result::Result<T, Error<U>>;

#[cfg(feature = "use_std")]
pub use with_std::{NewThread, SpawnHandle, Spawned, SpawnHelper};

impl<T> Error<T> {
    /// Create a new `Error`
    pub fn new(kind: ErrorKind, task: T) -> Error<T> {
        Error {
            task: task,
            kind: kind,
        }
    }

    /// Returns the associated reason for the error
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Consumes self and returns the original task that was spawned.
    pub fn into_task(self) -> T {
        self.task
    }
}

#[cfg(feature = "use_std")]
mod with_std {
    use Spawn;
    use futures::{Future, IntoFuture, Poll, Async};
    use futures::future::{self, CatchUnwind, Lazy};
    use futures::sync::oneshot;

    use std::{thread};
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::SeqCst;

    /// The type of future returned from the `Spawn::spawn` function, which
    /// proxies the futures running on the thread pool.
    ///
    /// This future will resolve in the same way as the underlying future, and it
    /// will propagate panics.
    #[must_use]
    pub struct SpawnHandle<T, E> {
        rx: oneshot::Receiver<thread::Result<Result<T, E>>>,
        keep_running_flag: Arc<AtomicBool>,
    }

    /// Contains a future that was spawned
    pub struct Spawned<F: Future> {
        future: CatchUnwind<AssertUnwindSafe<F>>,
        tx: Option<oneshot::Sender<thread::Result<Result<F::Item, F::Error>>>>,
        keep_running_flag: Arc<AtomicBool>,
    }

    /// Spawn all futures on a new thread
    ///
    /// This is the most basic `Spawn` implementation. Each call to `spawn` results
    /// in a new thread dedicated to processing the given future to completion.
    pub struct NewThread;

    /// Additional strategies for spawning a future.
    ///
    /// These functions have to be on a separate trait vs. on the `Spawn` trait
    /// in order to make rustc happy.
    pub trait SpawnHelper {
        /// Spawns a future to run on this `Spawn`, returning a future representing
        /// the produced value.
        ///
        ///
        /// This function will return immediately, and schedule the future `f` to
        /// run on `self`. The details of scheduling and execution are left to the
        /// implementations of `Spawn`. The returned future serves as a proxy to the
        /// computation that `F` is running.
        ///
        /// To simply run an arbitrary closure and extract the result, you can use
        /// the `future::lazy` combinator to defer work to executing on `&self`.
        ///
        /// Note that if the future `f` panics it will be caught by default and the
        /// returned future will propagate the panic. That is, panics will not reach
        /// `&self` and will be propagated to the returned future's `poll` method if
        /// queried.
        ///
        /// If the returned future is dropped then `f` will be canceled, if
        /// possible. That is, if the computation is in the middle of working, it
        /// will be interrupted when possible.
        fn spawn<F>(&self, future: F) -> super::Result<SpawnHandle<F::Item, F::Error>, F>
            where F: Future,
                  Self: Spawn<Spawned<F>>,
        {
            use futures::sync::oneshot;
            use std::panic::AssertUnwindSafe;
            use std::sync::Arc;
            use std::sync::atomic::AtomicBool;

            let (tx, rx) = oneshot::channel();
            let keep_running_flag = Arc::new(AtomicBool::new(false));

            // AssertUnwindSafe is used here becuase `Send + 'static` is basically
            // an alias for an implementation of the `UnwindSafe` trait but we can't
            // express that in the standard library right now.
            let sender = Spawned {
                future: AssertUnwindSafe(future).catch_unwind(),
                tx: Some(tx),
                keep_running_flag: keep_running_flag.clone(),
            };

            // Spawn the future
            try!(self.spawn_detached(sender).or_else(|e| {
                // TODO: Try to unbox the task
                drop(e);
                Ok(())
                /*
                let kind = e.kind();
                let task = e.into_task();

                unimplemented!();
                // super::Error::new(kind, task.future.into_inner())
                */
            }));

            Ok(SpawnHandle {
                rx: rx,
                keep_running_flag: keep_running_flag,
            })
        }

        /// Spawns a closure on this `Spawn`
        ///
        /// This function is a convenience wrapper around the `spawn` function above
        /// for running a closure wrapped in `future::lazy`. It will spawn the
        /// function `f` provided onto the thread pool, and continue to run the
        /// future returned by `f` on the thread pool as well.
        fn spawn_fn<F, R>(&self, f: F) -> super::Result<SpawnHandle<R::Item, R::Error>, F>
            where F: FnOnce() -> R,
                  R: IntoFuture,
                  Self: Spawn<Spawned<Lazy<F, R>>>,
        {
            self.spawn(future::lazy(f))
                .map_err(|e| {
                    // TODO: Try to unbox the error
                    drop(e);
                    unreachable!();
                })
        }
    }

    impl<T> SpawnHelper for T {
    }

    impl<T, E> SpawnHandle<T, E> {
        /// Drop this future without canceling the underlying future.
        ///
        /// When `SpawnHandle` is dropped, the spawned future will be dropped as
        /// well. This function can be used when user wants to drop but keep
        /// executing the underlying future.
        pub fn detach(self) {
            self.keep_running_flag.store(true, SeqCst);
        }
    }

    impl<T, E> Future for SpawnHandle<T, E> {
        type Item = T;
        type Error = E;

        fn poll(&mut self) -> Poll<T, E> {
            match self.rx.poll().expect("shouldn't be canceled") {
                Async::Ready(Ok(Ok(e))) => Ok(e.into()),
                Async::Ready(Ok(Err(e))) => Err(e),
                Async::Ready(Err(e)) => panic::resume_unwind(e),
                Async::NotReady => {
                    Ok(Async::NotReady)
                }
            }
        }
    }

    impl<F: Future> Future for Spawned<F> {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            if let Ok(Async::Ready(_)) = self.tx.as_mut().unwrap().poll_cancel() {
                if !self.keep_running_flag.load(SeqCst) {
                    // Cancelled, bail out
                    return Ok(().into())
                }
            }

            let res = match self.future.poll() {
                Ok(Async::Ready(e)) => Ok(e),
                Ok(Async::NotReady) => {
                    return Ok(Async::NotReady);
                }
                Err(e) => Err(e),
            };

            let _ = self.tx.take().unwrap().send(res);

            Ok(Async::Ready(()))
        }
    }

    impl<T: Future<Item = (), Error = ()> + Send + 'static> Spawn<T> for NewThread {
        fn spawn_detached(&self, future: T) -> super::Result<(), T> {
            use std::thread;

            thread::spawn(move || {
                let _ = future.wait();
            });

            Ok(())
        }
    }

    #[test]
    fn test_new_thread() {
        let new_thread = NewThread;
        let res = new_thread.spawn_fn(|| Ok::<u32, ()>(1));

        assert_eq!(1, res.wait().unwrap());
    }
}

#[cfg(feature = "tokio")]
mod tokio {
    extern crate tokio_core;

    use Spawn;
    use futures::Future;
    use self::tokio_core::reactor::{Core, Handle, Remote};

    impl<T: Future<Item = (), Error = ()> + 'static> Spawn<T> for Handle {
        fn spawn_detached(&self, future: T) -> super::Result<(), T> {
            Handle::spawn(self, future);
            Ok(())
        }
    }

    impl<T: Future<Item = (), Error = ()> + 'static> Spawn<T> for Core {
        fn spawn_detached(&self, future: T) -> super::Result<(), T> {
            self.handle().spawn_detached(future)
        }
    }

    impl<T: Future<Item = (), Error = ()> + Send + 'static> Spawn<T> for Remote {
        fn spawn_detached(&self, future: T) -> super::Result<(), T> {
            Remote::spawn(self, move |_| future);
            Ok(())
        }
    }
}
