//! # Signal handling utility functions

/// It will call `cb` function for Ctrl-c events (or SIGTERM signals in Unix).
pub fn ctrl_c<T: Fn() + 'static + Clone>(cb: T) {
    #[cfg(unix)]
    register_sigterm_handler(cb.clone());

    let f = async move {
        loop {
            // This is received when pressing CTRL-C, and it works on both Unix and Windows
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for CTRL-C event");
            log::trace!("Received CTRL-C");

            cb();
        }
    };

    actix::spawn(f);
}

#[cfg(unix)]
fn register_sigterm_handler<T: Fn() + 'static>(cb: T) {
    let f = async move {
        loop {
            // This is received when doing kill $(pidof witnet), removing this handler will make the wallet
            // shutdown instantly and corrupt the database.
            let mut sigterm_stream =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to register SIGTERM handler");
            sigterm_stream.recv().await;
            log::trace!("Received SIGTERM signal");

            cb();
        }
    };

    actix::spawn(f);
}
