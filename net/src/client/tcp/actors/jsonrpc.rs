//! Defines a JsonRPC over TCP actor.
//!
//! See the `JsonRpcClient` struct for more information.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    DuplexTransport, ErrorKind as TransportErrorKind, Transport as _,
};
use futures::Future;
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
        let (_handle, socket) = TcpSocket::new(url).map_err(|_| Error::InvalidUrl)?;
        let client = Self {
            _handle,
            socket,
            active_subscriptions: subscriptions,
            pending_subscriptions: Default::default(),
            url: String::from(url),
        };
        log::info!("TCP socket is now connected to {}", url);

        Ok(Actor::start(client))
    }

    /// Replace the TCP connection with a fresh new connection.
    pub fn reconnect(&mut self, ctx: &mut <Self as Actor>::Context) {
        log::info!("Reconnecting TCP client to {}", self.url);
        let (_handle, socket) = TcpSocket::new(self.url.as_str())
            .map_err(|e| log::error!("Reconnection error: {}", e))
            .expect("TCP socket reconnection should not panic, as the only possible error is malformed URL");
        self._handle = _handle;
        self.socket = socket;

        // Recover active subscriptions
        let mut active_subscriptions = self
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
        active_subscriptions.clear();
        self.pending_subscriptions.clear();
    }

    /// Send Json-RPC request.
    pub fn send_request(
        &self,
        method: String,
        params: Value,
    ) -> impl Future<Item = Value, Error = Error> {
        log::trace!(
            "<< Sending request, method: {:?}, params: {:?}",
            &method,
            &params
        );
        self.socket
            .execute(&method, params)
            .inspect(|resp| log::trace!(">> Received response: {:?}", resp))
            .map_err(|err| {
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

/// JSONRPC notification.
pub struct Notification {
    /// This doubles as subscription ID or subscription topic, depending on where it is used.
    pub id: String,
    /// A JSON value.
    pub value: Value,
}

impl Message for Notification {
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
    type Result = ResponseActFuture<Self, Value, Error>;

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
        let fut = self
            .send_request(method, params)
            .into_actor(self)
            .timeout(timeout, Error::RequestTimedOut(timeout.as_millis()))
            .map_err(move |err, act, ctx| {
                log::error!("JSONRPC Request error: {:?}", err);
                if is_connection_error(&err) {
                    act.reconnect(ctx);
                }
                err
            });

        Box::new(fut)
    }
}

/// A message representing a subscription to notifications.
///
/// This ties together:
/// - The JSONRPC request that needs to be sent to the server for initiating the subscription.
/// - A `Recipient` for JSONRPC notifications.
#[derive(Clone)]
pub struct Subscribe(pub Request, pub Recipient<Notification>);

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
            .map_err(|err| log::error!("Couldn't subscribe: {}", err))
            .into_actor(self)
            .map(move |resp, act, ctx| {
                match resp {
                    Ok(Value::String(id)) => {
                        if let Ok(mut subscriptions) = act.active_subscriptions.lock() {
                            (*subscriptions).insert(id.clone(), subscribe.clone());
                        };

                        let stream = act
                            .socket
                            .subscribe(&id.clone().into())
                            .map(move |value| {
                                log::debug!("<< Forwarding notification from node to subscribers",);
                                log::trace!("<< {:?}", value);
                                Notification {
                                    id: id.clone(),
                                    value,
                                }
                            })
                            .map_err(|err| Error::RequestFailed {
                                message: err.to_string(),
                                error_kind: err.0,
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
                };
            })
            .spawn(ctx);
    }
}

impl StreamHandler<Notification, Error> for JsonRpcClient {
    fn handle(
        &mut self,
        Notification {
            id: subscription_id,
            value,
        }: Notification,
        _ctx: &mut Self::Context,
    ) {
        if let Ok(subscriptions) = (*self.active_subscriptions).lock() {
            if let Some(Subscribe(ref request, ref recipient)) = subscriptions.get(&subscription_id)
            {
                let method = subscription_topic_from_request(request);
                if let Err(err) = recipient.do_send(Notification { id: method, value }) {
                    log::error!("Client couldn't notify subscriber: {}", err);
                }
            }
        }
    }
}

fn is_connection_error(err: &Error) -> bool {
    match err {
        Error::RequestFailed { error_kind, .. } => match error_kind {
            TransportErrorKind::Transport(_) => true,
            TransportErrorKind::Unreachable => true,
            _ => false,
        },
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
