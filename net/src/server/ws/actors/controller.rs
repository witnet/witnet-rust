//! Defines an actor to control system run and shutdown.
//!
//! See the [`Controller`] struct for more information.
use std::io;
use std::time::Duration;

use actix_web::{actix::*, server, ws, App, Binary};
use failure::Fail;
use futures::{future, Future};
use jsonrpc_core as rpc;

use super::notifications::{Notifications, Notify};
use super::ws::Ws;

/// Actor to start and gracefully stop an actix system.
///
/// This actor contains a static `run` method which will run an actix system and block the current
/// thread until the system shuts down again.
///
/// To shut down more gracefully, other actors can register with the [`Subscribe`] message. When a
/// shutdown signal is sent to the process, they will receive a [`Shutdown`] message with an
/// optional timeout. They can respond with a future, after which they will be stopped. Once all
/// registered actors have stopped successfully, the entire system will stop.
#[derive(Default)]
pub struct Controller {
    /// Configured timeout for graceful shutdown
    timeout: Duration,
    /// Subscribed actors for the shutdown message.
    subscribers: Vec<Recipient<Shutdown>>,
}

/// Function passed to a JsonRPC handler factory so the handlers are able to send notifications to
/// other clients.
fn notify(payload: Binary) {
    let n = Notifications::from_registry();
    n.do_send(Notify { payload });
}

impl Controller {
    /// Starts a JsonRPC Websockets server.
    ///
    /// The factory is used to create handlers for the json-rpc messages sent to the server.
    pub fn run<F>(jsonrpc_handler_factory: F) -> io::Result<()>
    where
        F: Fn(fn(Binary)) -> rpc::IoHandler + Clone + Send + Sync + 'static,
    {
        let s = server::new(move || {
            // Ensure that the controller starts if no actor has started it yet. It will register with
            // `ProcessSignals` shut down even if no actors have subscribed. If we remove this line, the
            // controller will not be instanciated and our system will not listen for signals.
            Controller::from_registry();

            let factory = jsonrpc_handler_factory.clone();
            App::new().resource("/", move |r| {
                r.f(move |req| ws::start(req, Ws::new(factory(notify))))
            })
        })
        .bind("127.0.0.1:3030")?;

        s.run();

        Ok(())
    }
    /// Performs a graceful shutdown with the given timeout.
    ///
    /// This sends a `Shutdown` message to all subscribed actors and
    /// waits for them to finish. As soon as all actors have
    /// completed, `Controller::stop` is called.
    pub fn shutdown(&mut self, ctx: &mut Context<Self>, timeout: Option<Duration>) {
        let futures: Vec<_> = self
            .subscribers
            .iter()
            .map(|addr| {
                addr.send(Shutdown { timeout })
                    .map(|_| ())
                    .map_err(|e| log::error!("Shutdown failed: {}", e))
            })
            .collect();

        future::join_all(futures)
            .into_actor(self)
            .and_then(|_, _, ctx| {
                ctx.stop();
                fut::ok(())
            })
            .spawn(ctx)
    }
}

impl Actor for Controller {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        signal::ProcessSignals::from_registry()
            .do_send(signal::Subscribe(ctx.address().recipient()));
    }
}

impl Supervised for Controller {}

impl SystemService for Controller {}

impl Handler<signal::Signal> for Controller {
    type Result = <signal::Signal as Message>::Result;

    fn handle(&mut self, message: signal::Signal, ctx: &mut Self::Context) -> Self::Result {
        match message.0 {
            signal::SignalType::Int => {
                log::info!("SIGINT received, exiting");
                self.shutdown(ctx, None)
            }
            signal::SignalType::Quit => {
                log::info!("SIGQUIT received, exiting");
                self.shutdown(ctx, None);
            }
            signal::SignalType::Term => {
                let timeout = self.timeout;
                log::info!("SIGTERM received, stopping in {}s", timeout.as_secs());
                self.shutdown(ctx, Some(timeout));
            }
            _ => (),
        }
    }
}

/// Subscription message for [`Shutdown`](Shutdown) events
pub struct Subscribe(pub Recipient<Shutdown>);

impl Message for Subscribe {
    type Result = ();
}

impl Handler<Subscribe> for Controller {
    type Result = <Subscribe as Message>::Result;

    fn handle(&mut self, msg: Subscribe, _ctx: &mut Self::Context) -> Self::Result {
        self.subscribers.push(msg.0)
    }
}

/// Shutdown request message sent by the [`Controller`](Controller) to subscribed actors.
///
/// The specified timeout is only a hint to the implementor of this message. A handler has to ensure
/// that it doesn't take significantly longer to resolve the future. Ideally, open work is persisted
/// or finished in an orderly manner but no new requests are accepted anymore.
///
/// The implementor may indicate a timeout by responding with `Err(TimeoutError)`. At the moment,
/// this does not have any consequences for the shutdown.
pub struct Shutdown {
    /// The timeout for this shutdown. `None` indicates an immediate forced shutdown.
    pub timeout: Option<Duration>,
}

/// Result type with error set to [`TimeoutError`](TimeoutError)
pub type ShutdownResult = Result<(), TimeoutError>;

impl Message for Shutdown {
    type Result = ShutdownResult;
}

#[derive(Debug, Fail, Copy, Clone, Eq, PartialEq)]
#[fail(display = "timed out")]
pub struct TimeoutError;
