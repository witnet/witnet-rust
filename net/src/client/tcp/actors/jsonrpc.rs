//! Defines a JsonRPC over TCP actor.
//!
//! See the `JsonRpcClient` struct for more information.
use std::{
    cmp,
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    DuplexTransport, ErrorKind as TransportErrorKind, Transport as _,
};
use futures::StreamExt;
use futures_util::compat::Compat01As03;
use rand::seq::SliceRandom;
use serde::Serialize;
use serde_json::value;

pub use serde_json::Value;

use super::Error;

const DEFAULT_BACKOFF_TIME_MILLIS: u64 = 250;
const MAX_BACKOFF_TIME_MILLIS: u64 = 15_000;

/// Represents a JSONRPC client connection, and wraps some related metadata.
struct Connection {
    /// Current backoff time (seconds between reconnection attempts).
    backoff: Duration,
    /// The TCP Socket for the connection.
    socket: TcpSocket,
    /// Used to calculate the time since the last reconnection, and prevent multiple reconnections
    /// in a short time interval.
    timestamp: Instant,
    /// URL for the connection, in String format.
    url: String,
}

/// Json-RPC Client actor.
///
/// Use this actor to send json-rpc requests over a websockets connection.
pub struct JsonRpcClient {
    _handle: EventLoopHandle,
    active_subscriptions: Arc<Mutex<HashMap<String, Subscribe>>>,
    pending_subscriptions: HashMap<String, Subscribe>,
    urls: Vec<String>,
    connection: Connection,
}

impl JsonRpcClient {
    /// Start JSON-RPC async client actor providing only the URL of the server.
    pub fn start(url: &str) -> Result<Addr<JsonRpcClient>, Error> {
        let subscriptions = Arc::new(Default::default());

        Self::start_with_subscriptions(vec![String::from(url)], subscriptions)
    }

    /// Start JSON-RPC async client actor providing the URL of the server and some subscriptions.
    pub fn start_with_subscriptions(
        urls: Vec<String>,
        subscriptions: Arc<Mutex<HashMap<String, Subscribe>>>,
    ) -> Result<Addr<JsonRpcClient>, Error> {
        log::info!("Configuring JSONRPC client with URLs: {:?}", &urls);
        let timestamp = Instant::now();
        let url = urls
            .choose(&mut rand::thread_rng())
            .ok_or(Error::NoUrl)?
            .clone();
        let (_handle, socket) = TcpSocket::new(&url).map_err(|_| Error::InvalidUrl)?;

        log::info!("TCP socket is now connected to {}", url);

        let client = Self {
            _handle,
            active_subscriptions: subscriptions,
            pending_subscriptions: Default::default(),
            urls,
            connection: Connection {
                backoff: Duration::from_millis(DEFAULT_BACKOFF_TIME_MILLIS),
                socket,
                timestamp,
                url,
            },
        };

        Ok(Actor::start(client))
    }

    /// Replace the TCP connection with a fresh new connection.
    pub fn reconnect(&mut self, ctx: &mut <Self as Actor>::Context) {
        let timestamp = Instant::now();
        // Apply exponential back-off on retries
        let reconnection_cooldown = self.connection.backoff;
        if timestamp.duration_since(self.connection.timestamp) < reconnection_cooldown {
            log::debug!(
                "Ignoring reconnect request: last reconnection attempt was less than {} seconds ago", reconnection_cooldown.as_secs_f32()
            );
            return;
        }

        // If there is only 1 URL, use that one.
        // If there are many, pick a new one randomly that is not the same as the previous one
        let url = pick_random(&self.urls, Some(self.current_url().to_string()))
            .expect("At this point there should be at least one URL set for connecting the client");

        // Connect to the new URL
        log::info!("Reconnecting TCP client to {}", url);
        let (_handle, socket) = TcpSocket::new(&url)
            .map_err(|e| log::error!("Reconnection error: {}", e))
            .expect("TCP socket reconnection should not panic, as the only possible error is malformed URL");

        // Update connection info
        self._handle = _handle;
        self.connection.socket = socket;
        self.connection.timestamp = timestamp;
        self.connection.url = url;

        // Recover active subscriptions
        let active_subscriptions = self
            .active_subscriptions
            .lock()
            .map(|x| x.clone())
            .expect("Active subscriptions Mutex should never be poisoned");
        log::debug!(
            "Trying to recover {} active subscriptions",
            active_subscriptions.len()
        );
        active_subscriptions.iter().for_each(|(_, subscribe)| {
            log::debug!("Resubscribing {:?}", subscribe.0);
            ctx.notify(subscribe.clone());
        });

        // Process pending subscriptions
        log::debug!(
            "Trying to process {} pending subscriptions",
            self.pending_subscriptions.len()
        );
        self.pending_subscriptions
            .iter()
            .for_each(|(topic, subscribe)| {
                log::debug!(
                    "Processing pending subscription for topic {}: {:?}",
                    topic,
                    subscribe.0
                );
                ctx.notify(subscribe.clone());
            });

        // Clear up all subscriptions (will be pushed again if they keep failing)
        if let Ok(mut x) = self.active_subscriptions.lock() {
            x.clear()
        }
        self.pending_subscriptions.clear();
    }

    /// Retrieve the URL of the current client connection.
    pub fn current_url(&self) -> &str {
        &self.connection.url
    }

    /// Send Json-RPC request.
    pub async fn send_request(
        socket: TcpSocket,
        method: String,
        params: Value,
    ) -> Result<Value, Error> {
        log::trace!(
            "<< Sending request, method: {:?}, params: {:?}",
            &method,
            &params
        );
        let f = socket.execute(&method, params);

        let res = Compat01As03::new(f).await;

        if let Ok(resp) = &res {
            log::trace!(">> Received response: {:?}", resp);
        }

        res.map_err(|err| {
            log::trace!(">> Received error: {}", err);
            Error::RequestFailed {
                message: err.to_string(),
                error_kind: err.0,
            }
        })
    }

    fn increase_backoff_time(&mut self) {
        let time = core::cmp::min(
            self.connection.backoff * 125 / 100,
            Duration::from_millis(MAX_BACKOFF_TIME_MILLIS),
        );
        self.set_backoff_time(time);
    }

    fn reset_backoff_time(&mut self) {
        self.set_backoff_time(Duration::from_millis(DEFAULT_BACKOFF_TIME_MILLIS));
    }

    fn set_backoff_time(&mut self, time: Duration) {
        log::trace!(
            "Connection backoff time is now set to {} seconds",
            time.as_secs_f32()
        );
        self.connection.backoff = time;
    }
}

impl Actor for JsonRpcClient {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("JsonRpcClient actor started!");
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        log::info!("JsonRpcClient actor was trying to stop for some reason. Refusing to stop!");

        Running::Continue
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("JsonRpcClient actor stopped!");
    }
}

impl Supervised for JsonRpcClient {}

/// JSONRPC notification, paired with a subscription ID.
pub struct NotifySubscriptionId {
    /// Subscription ID.
    pub id: String,
    /// A JSON value.
    pub value: Value,
}

impl Message for NotifySubscriptionId {
    type Result = ();
}

/// JSONRPC notification, paired with a subscription topic.
pub struct NotifySubscriptionTopic {
    /// Subscription topic.
    pub topic: String,
    /// A JSON value.
    pub value: Value,
}

impl Message for NotifySubscriptionTopic {
    type Result = ();
}

/// Request sent by the client.
#[derive(Debug, Clone)]
pub struct Request {
    method: String,
    params: Value,
    timeout: Duration,
}

impl Request {
    /// Create a new request with the given method.
    pub fn method<T: Into<String>>(method: T) -> Self {
        Self {
            method: method.into(),
            params: Value::Null,
            timeout: Duration::from_secs(60),
        }
    }

    /// Set request params.
    pub fn params<T: Serialize>(mut self, params: T) -> Result<Self, Error> {
        self.params = value::to_value(params).map_err(Error::SerializeFailed)?;
        Ok(self)
    }

    /// Set request params that are already a serialized value.
    pub fn value(mut self, params: Value) -> Self {
        self.params = params;
        self
    }

    /// Set the request timeout after which it will fail if server has not responded.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = duration;
        self
    }
}

impl Message for Request {
    type Result = Result<Value, Error>;
}

impl Handler<Request> for JsonRpcClient {
    type Result = ResponseActFuture<Self, Result<Value, Error>>;

    fn handle(
        &mut self,
        Request {
            method,
            params,
            timeout,
        }: Request,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        log::trace!(
            "Handling Request: {}, {:?}, {}",
            method,
            params,
            timeout.as_millis()
        );
        let fut = JsonRpcClient::send_request(self.connection.socket.clone(), method, params)
            .into_actor(self)
            .timeout(timeout)
            .map(move |res, _act, _ctx| {
                res.unwrap_or(Err(Error::RequestTimedOut(timeout.as_millis())))
            })
            .map(|res, act, ctx| {
                res.map(|res| {
                    // Backoff time is reset to default
                    act.reset_backoff_time();
                    res
                })
                .map_err(|err| {
                    log::error!("JSONRPC Request error: {:?}", err);
                    if is_connection_error(&err) {
                        // Backoff time is increased
                        act.increase_backoff_time();
                        act.reconnect(ctx);
                    }

                    err
                })
            });

        Box::pin(fut)
    }
}

/// A message representing a subscription to notifications.
///
/// This ties together:
/// - The JSONRPC request that needs to be sent to the server for initiating the subscription.
/// - A `Recipient` for JSONRPC notifications.
#[derive(Clone)]
pub struct Subscribe(pub Request, pub Recipient<NotifySubscriptionTopic>);

impl Message for Subscribe {
    type Result = ();
}

impl Handler<Subscribe> for JsonRpcClient {
    type Result = <Subscribe as Message>::Result;

    fn handle(&mut self, subscribe: Subscribe, ctx: &mut Self::Context) -> Self::Result {
        let request = subscribe.0.clone();
        let topic = subscription_topic_from_request(&request);
        log::debug!(
            "Handling Subscribe message for topic {}. Request is {:?}",
            topic,
            request
        );

        ctx.address()
            .send(request.clone())
            .into_actor(self)
            .map(move |res, act, ctx| match res {
                Ok(resp) => match resp {
                    Ok(Value::String(id)) => {
                        if let Ok(mut subscriptions) = act.active_subscriptions.lock() {
                            (*subscriptions).insert(id.clone(), subscribe.clone());
                        };

                        let stream_01 = act.connection.socket.subscribe(&id.clone().into());

                        let stream_03 = Compat01As03::new(stream_01);
                        let stream = stream_03.map(move |res| {
                            let id = id.clone();
                            res.map(move |value| {
                                log::debug!("<< Forwarding notification from node to subscribers",);
                                log::trace!("<< {:?}", value);
                                NotifySubscriptionId { id, value }
                            })
                            .map_err(|err| Error::RequestFailed {
                                message: err.to_string(),
                                error_kind: err.0,
                            })
                        });
                        Self::add_stream(stream, ctx);
                        if let Some(method) = request.params.get(0) {
                            log::info!("Client {} subscription created", method);
                        }
                    }
                    Ok(_) => {
                        log::error!("Unsupported subscription id. Subscription cancelled.");
                    }
                    Err(err) => {
                        log::error!(
                            "Could not subscribe to topic {}. Delaying subscription. Error was: {}",
                            topic,
                            err
                        );
                        act.pending_subscriptions.insert(topic, subscribe);
                    }
                },
                Err(err) => {
                    log::error!("Couldn't subscribe: {}", err);
                }
            })
            .spawn(ctx);
    }
}

/// Get the URL of the node that the client is trying to connect to
#[derive(Clone)]
pub struct GetCurrentNodeUrl;

impl Message for GetCurrentNodeUrl {
    type Result = String;
}

impl Handler<GetCurrentNodeUrl> for JsonRpcClient {
    type Result = <GetCurrentNodeUrl as Message>::Result;

    fn handle(&mut self, _msg: GetCurrentNodeUrl, _ctx: &mut Self::Context) -> Self::Result {
        self.connection.url.clone()
    }
}

impl StreamHandler<Result<NotifySubscriptionId, Error>> for JsonRpcClient {
    fn handle(&mut self, res: Result<NotifySubscriptionId, Error>, _ctx: &mut Self::Context) {
        match res {
            Ok(NotifySubscriptionId {
                id: subscription_id,
                value,
            }) => {
                if let Ok(subscriptions) = (*self.active_subscriptions).lock() {
                    if let Some(Subscribe(ref request, ref recipient)) =
                        subscriptions.get(&subscription_id)
                    {
                        let topic = subscription_topic_from_request(request);
                        recipient.do_send(NotifySubscriptionTopic { topic, value });
                    }
                }
            }
            Err(err) => {
                // TODO: how to handle this error?
                log::error!("Subscription failed: {}", err);
            }
        }
    }
}

fn is_connection_error(err: &Error) -> bool {
    match err {
        Error::RequestFailed { error_kind, .. } => {
            matches!(
                error_kind,
                TransportErrorKind::Transport(_) | TransportErrorKind::Unreachable
            )
        }
        Error::RequestTimedOut(_) => true,
        Error::Mailbox(_) => true,
        _ => false,
    }
}

/// Extract a subscription topic from a JSONRPC request
fn subscription_topic_from_request(request: &Request) -> String {
    request
        .params
        .get(0)
        .cloned()
        .map(serde_json::from_value)
        .expect("Subscriptions should always have a topic")
        .expect("Subscription topics should always be String")
}

/// Pick a random element from a list, avoiding "twice-in-a-row" repetition when possible.
fn pick_random<T>(input: &[T], old: Option<T>) -> Option<T>
where
    T: Clone + cmp::PartialEq,
{
    match input.len() {
        0 => None,
        1 => Some(input[0].clone()),
        _ => {
            // Non-iterative approach to randomly picking one item out of a list without drawing
            // the same item twice in a row.
            //
            // 1. Choose from 1 out of N-1 instead of 1 out of N, by making it impossible to draw
            //    the item at position 0.
            // 2. If the drawn item is equal to the formerly drawn item, return the item at position
            //    0 instead.
            // 3. Otherwise, return the randomly drawn item.
            let mut pick = input[1..].choose(&mut rand::thread_rng())?;
            if matches!(old, Some(old) if &old == pick) {
                pick = &input[0]
            }

            Some(pick.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_random_from_empty_list() {
        let list = Vec::<()>::new();

        let pick = pick_random(&list, None);
        assert_eq!(pick, None);

        let pick = pick_random(&list, Some(()));
        assert_eq!(pick, None);
    }

    #[test]
    fn pick_random_from_single_item_list() {
        let list = vec![1337];

        let pick = pick_random(&list, None);
        assert_eq!(pick, Some(list[0]));

        let pick = pick_random(&list, Some(1337));
        assert_eq!(pick, Some(list[0]));

        let pick = pick_random(&list, Some(2337));
        assert_eq!(pick, Some(list[0]));
    }

    #[test]
    fn pick_random_from_multiple_item_list() {
        let list = vec![1337, 23337, 3337];

        // This is drawing 1000 items from a list of three, checking every time that the drawn
        // number is not the same as the one drawn just before.
        (0..1_000).fold(None, |prev, _| {
            let pick = pick_random(&list, prev);

            assert_ne!(pick, prev);

            pick
        });
    }
}
