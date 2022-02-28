//! Defines a JsonRPC over TCP actor.
//!
//! See the `JsonRpcClient` struct for more information.
use std::{
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
use serde::Serialize;
use serde_json::{value, Value};

use super::Error;

/// Json-RPC Client actor.
///
/// Use this actor to send json-rpc requests over a websockets connection.
pub struct JsonRpcClient {
    _handle: EventLoopHandle,
    socket: TcpSocket,
    active_subscriptions: Arc<Mutex<HashMap<String, Subscribe>>>,
    pending_subscriptions: HashMap<String, Subscribe>,
    url: String,
    // Used to calculate the time since the last reconnection, and prevent multiple reconnections
    // in a short time interval
    last_reconnection: Instant,
}

impl JsonRpcClient {
    /// Start JSON-RPC async client actor providing only the URL of the server.
    pub fn start(url: &str) -> Result<Addr<JsonRpcClient>, Error> {
        let subscriptions = Arc::new(Default::default());

        Self::start_with_subscriptions(url, subscriptions)
    }

    /// Start JSON-RPC async client actor providing the URL of the server and some subscriptions.
    pub fn start_with_subscriptions(
        url: &str,
        subscriptions: Arc<Mutex<HashMap<String, Subscribe>>>,
    ) -> Result<Addr<JsonRpcClient>, Error> {
        log::info!("Connecting client to {}", url);
        let last_reconnection = Instant::now();
        let (_handle, socket) = TcpSocket::new(url).map_err(|_| Error::InvalidUrl)?;
        let client = Self {
            _handle,
            socket,
            active_subscriptions: subscriptions,
            pending_subscriptions: Default::default(),
            url: String::from(url),
            last_reconnection,
        };
        log::info!("TCP socket is now connected to {}", url);

        Ok(Actor::start(client))
    }

    /// Replace the TCP connection with a fresh new connection.
    pub fn reconnect(&mut self, ctx: &mut <Self as Actor>::Context) {
        let now = Instant::now();
        // Attempt to reconnect at most once every 10 seconds
        let reconnection_cooldown = Duration::from_secs(10);
        if now.duration_since(self.last_reconnection) < reconnection_cooldown {
            log::debug!(
                "Ignoring reconnect request: last reconnection attempt was a few seconds ago"
            );
            return;
        }

        self.last_reconnection = now;
        log::info!("Reconnecting TCP client to {}", self.url);
        let (_handle, socket) = TcpSocket::new(self.url.as_str())
            .map_err(|e| log::error!("Reconnection error: {}", e))
            .expect("TCP socket reconnection should not panic, as the only possible error is malformed URL");
        self._handle = _handle;
        self.socket = socket;

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
        let fut = JsonRpcClient::send_request(self.socket.clone(), method, params)
            .into_actor(self)
            .timeout(timeout)
            .map(move |res, _act, _ctx| {
                res.unwrap_or(Err(Error::RequestTimedOut(timeout.as_millis())))
            })
            .map(|res, act, ctx| {
                res.map_err(|err| {
                    log::error!("JSONRPC Request error: {:?}", err);
                    if is_connection_error(&err) {
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

                        let stream_01 = act.socket.subscribe(&id.clone().into());

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
                        if let Err(err) =
                            recipient.do_send(NotifySubscriptionTopic { topic, value })
                        {
                            log::error!("Client couldn't notify subscriber: {}", err);
                        }
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
