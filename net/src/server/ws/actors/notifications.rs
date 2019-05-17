//! Defines an actor that can be used to notify subscribers.
use actix_web::{actix::*, Binary};
use futures::{future, Future};

/// Actor that keeps a list of clients to which send notifications.
///
/// Use the [`Subscribe`](Subscribe) message to subscribe and the [`Notify`](Notify) message to send
/// a notification to all subscriptors.
pub struct Notifications {
    /// Subscribed actors for the notification message.
    subscribers: Vec<Recipient<Notify>>,
    /// Field used to check before
    version: Version,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            subscribers: Vec::new(),
            version: Version::new(),
        }
    }
}

impl Actor for Notifications {
    type Context = Context<Self>;
}

impl Supervised for Notifications {}

impl SystemService for Notifications {}

/// Message sent to the [`Notifications`](Notifications) actor to notify all subscribed actors.
#[derive(Clone)]
pub struct Notify {
    /// Payload of the notification message sent to the subscribers.
    pub payload: Binary,
}

impl Message for Notify {
    type Result = ();
}

impl Handler<Notify> for Notifications {
    type Result = <Notify as Message>::Result;

    fn handle(&mut self, msg: Notify, ctx: &mut Self::Context) -> Self::Result {
        let current_version = self.version;
        let futures: Vec<_> = self
            .subscribers
            .iter()
            .cloned()
            .map(|subscriber| {
                subscriber
                    .send(msg.clone())
                    .map(|_| Some(subscriber))
                    .or_else(|e| {
                        log::error!("Couldn't notify client: {}", e);
                        future::ok(None)
                    })
            })
            .collect();

        // Increment version so the returned future's Refresh-Update doesn't override another ones
        self.version.increment();
        future::join_all(futures)
            .into_actor(self)
            .and_then(move |subscriptions, _, ctx| {
                ctx.notify(Refresh {
                    version: current_version,
                    subscribers: subscriptions.into_iter().filter_map(|x| x).collect(),
                });
                fut::ok(())
            })
            .spawn(ctx);
    }
}

/// Subscription message.
///
/// It tells the [`Notifications`](Notifications) actor to add the received recipient to its list of
/// notification subscriptions.
pub struct Subscribe(pub Recipient<Notify>);

impl Message for Subscribe {
    type Result = ();
}

impl Handler<Subscribe> for Notifications {
    type Result = <Subscribe as Message>::Result;

    fn handle(&mut self, msg: Subscribe, _ctx: &mut Self::Context) -> Self::Result {
        self.version.increment();
        self.subscribers.push(msg.0);
    }
}

/// Refresh the list of subscriptions
///
/// It tells the [`Notifications`](Notifications) to replace its list of recipients by the received
/// one.
struct Refresh {
    version: Version,
    subscribers: Vec<Recipient<Notify>>,
}

impl Message for Refresh {
    type Result = ();
}

impl Handler<Refresh> for Notifications {
    type Result = <Refresh as Message>::Result;

    fn handle(&mut self, msg: Refresh, _ctx: &mut Self::Context) -> Self::Result {
        // Update subscribers only to the last ones we know are responding
        if self.version == msg.version {
            self.subscribers = msg.subscribers;
        }
    }
}

/// Helper type for handling version numbers.
#[derive(Eq, PartialEq, Copy, Clone)]
struct Version(u8);

impl Version {
    /// Create a new version instance.
    fn new() -> Self {
        Self(0)
    }

    /// Increment the version
    fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}
