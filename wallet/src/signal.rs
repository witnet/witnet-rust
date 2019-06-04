//! # Signal handling utility functions
use actix::prelude::*;

/// It will call `cb` function for Ctrl-c events (or SIGTERM signals in Unix).
pub fn ctrl_c<T: Fn() + 'static>(cb: T) {
    #[cfg(unix)]
    let sigterm = tokio_signal::unix::Signal::new(tokio_signal::unix::SIGTERM)
        .map(|s| s.map(|_| ()))
        .flatten_stream();

    #[cfg(windows)]
    let sigterm = futures::future::ok(()).into_stream();

    let ctrl_c = tokio_signal::ctrl_c().flatten_stream();
    let handle_shutdown = ctrl_c
        .select(sigterm)
        .for_each(move |_| {
            cb();
            Ok(())
        })
        .map_err(|_| ());

    actix::spawn(handle_shutdown);
}
