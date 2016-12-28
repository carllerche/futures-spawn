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

extern crate futures;

use futures::{Future, IntoFuture};
use futures::future::{self, Lazy};

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
    fn spawn(&self, f: T);

    /// Spawns a closure on this `Spawn`
    ///
    /// This function is a convenience wrapper around the `spawn` function above
    /// for running a closure wrapped in `future::lazy`. It will spawn the
    /// function `f` provided onto the thread pool, and continue to run the
    /// future returned by `f` on the thread pool as well.
    fn spawn_fn<F, R>(&self, f: F)
        where F: FnOnce() -> R,
              R: IntoFuture<Item = (), Error = ()>,
              Self: Spawn<Lazy<F, R>>,
    {
        self.spawn(future::lazy(f));
    }
}

#[cfg(feature = "use_std")]
pub use with_std::NewThread;

#[cfg(feature = "use_std")]
mod with_std {
    use {Spawn};
    use futures::Future;

    /// Spawn all futures on a new thread
    ///
    /// This is the most basic `Spawn` implementation. Each call to `spawn` results
    /// in a new thread dedicated to processing the given future to completion.
    pub struct NewThread;

    impl<T: Future<Item = (), Error = ()> + Send + 'static> Spawn<T> for NewThread {
        fn spawn(&self, future: T) {
            use std::thread;

            thread::spawn(move || {
                let _ = future.wait();
            });
        }
    }
}

#[cfg(feature = "tokio")]
mod tokio {
    extern crate tokio_core;

    use {Spawn};
    use futures::Future;
    use self::tokio_core::reactor::{Core, Handle, Remote};

    impl<T: Future<Item = (), Error = ()> + 'static> Spawn<T> for Handle {
        fn spawn(&self, future: T) {
            self.spawn(future);
        }
    }

    impl<T: Future<Item = (), Error = ()> + 'static> Spawn<T> for Core {
        fn spawn(&self, future: T) {
            self.handle().spawn(future);
        }
    }

    impl<T: Future<Item = (), Error = ()> + Send + 'static> Spawn<T> for Remote {
        fn spawn(&self, future: T) {
            self.spawn(move |_| future);
        }
    }
}
