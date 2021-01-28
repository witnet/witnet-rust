//! # Signal handling utility functions

/// It will call `cb` function for Ctrl-c events (or SIGTERM signals in Unix).
#[cfg(unix)]
pub fn ctrl_c<T: Fn() + 'static>(cb: T) {
    use futures01::{Future, Stream};
    use futures_util::compat::Compat01As03;

    // This is received when doing kill $(pidof witnet), removing this handler will make the wallet
    // shutdown instantly and corrupt the database.
    let sigterm = tokio_signal::unix::Signal::new(tokio_signal::unix::SIGTERM)
        .map(|s| s.map(|_| log::trace!("Received SIGTERM signal")))
        .flatten_stream();

    // This is received when pressing CTRL-C, and it works on both Unix and Windows
    let ctrl_c = tokio_signal::ctrl_c()
        .flatten_stream()
        .inspect(|_| log::trace!("Received CTRL-C"));

    // Handle both CTRL-C and SIGTERM using the same callback
    let handle_shutdown = ctrl_c
        .select(sigterm)
        .for_each(move |_| {
            cb();
            Ok(())
        })
        .map_err(|_| ());

    let f = futures::FutureExt::map(Compat01As03::new(handle_shutdown), |_res| ());

    actix::spawn(f);
}

/// It will call `cb` function for Ctrl-c events (or SIGTERM signals in Unix).
#[cfg(windows)]
pub fn ctrl_c<T: Fn() + 'static>(cb: T) {
    let f = async move {
        // This is received when pressing CTRL-C, and it works on both Unix and Windows
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for CTRL-C event");
        log::trace!("Received CTRL-C");

        cb();
    };

    actix::spawn(f);
}
