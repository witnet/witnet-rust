//! Defines a JsonRPC over TCP actor.
//!
//! See the `JsonRpcClient` struct for more information.
use std::{collections::HashMap, time::Duration};

use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    DuplexTransport as _, ErrorKind as TransportErrorKind, Transport as _,
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
    url: String,
    retry_connect: bool,
    subscriptions: HashMap<String, Subscribe>,
}

impl JsonRpcClient {
    /// Start Json-RPC async client actor.
    pub fn start(url: &str) -> Result<Addr<JsonRpcClient>, Error> {
        log::info!("Connecting client to {}", url);
        let (_handle, socket) = TcpSocket::new(url).map_err(|_| Error::InvalidUrl)?;
        let client = Self {
            _handle,
            socket,
            url: url.to_owned(),
            retry_connect: false,
            subscriptions: Default::default(),
        };

        Ok(client.start())
    }

    /// Renew the connection of the client.
    pub fn reconnect(&mut self) {
        log::info!("Reconnecting client to {}", self.url);
        // The .expect is because the creation of the socket might only fail if the url is invalid,
        // but since this a reconnection, meaning we were able to correctly parse the url before,
        // then at this point the url should be the same, hence still valid.
        let (_handle, socket) = TcpSocket::new(self.url.as_ref()).expect("Unexpected error");
        self._handle = _handle;
        self.socket = socket;
        self.retry_connect = false;
    }

    /// Renew all existing subscriptions.
    pub fn resubscribe(&mut self, ctx: &mut <Self as Actor>::Context) {
        log::debug!("Recovering {} subscriptions", self.subscriptions.len());
        self.subscriptions
            .clone()
            .into_iter()
            .for_each(|(_, subscribe)| {
                let method = subscription_method_from_request(&subscribe.0);
                log::debug!("Resubscribing to `{}` notifications", method);
                <Self as Handler<Subscribe>>::handle(self, subscribe, ctx);
            })
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

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::error!("JsonRpcClient actor stopped!")
    }
}

impl Supervised for JsonRpcClient {}

/// JSONRPC notification.
///
/// Contains:
/// 1. Subscription ID
/// 2. A JSON value
pub struct Notification(pub String, pub Value);

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
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.retry_connect {
            self.reconnect();
            self.resubscribe(ctx);
        }

        let fut = self
            .send_request(method, params)
            .into_actor(self)
            .timeout(timeout, Error::RequestTimedOut(timeout.as_millis()))
            .map_err(move |err, _act, ctx| {
                log::error!("JSONRPC Request error: {:?}", err);
                if is_connection_error(&err) {
                    ctx.notify(RetryConnect);
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
        log::debug!("Handling Subscribe message. Request is {:?}", &request);
        ctx.address()
            .send(request.clone())
            .map_err(|err| log::error!("Couldn't subscribe: {}", err))
            .into_actor(self)
            .map(move |resp, act, ctx| {
                match resp {
                    Ok(Value::String(id)) => {
                        act.subscriptions.insert(id.clone(), subscribe.clone());
                        let stream = act
                            .socket
                            .subscribe(&id.clone().into())
                            .map(move |value| {
                                log::debug!("<< Forwarding notification from node to subscribers",);
                                log::trace!("<< {:?}", value);
                                Notification(id.clone(), value)
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
                        log::error!("Couldn't subscribe: {}", err);
                    }
                };
            })
            .spawn(ctx);
    }
}

struct RetryConnect;

impl Message for RetryConnect {
    type Result = ();
}

impl Handler<RetryConnect> for JsonRpcClient {
    type Result = <RetryConnect as Message>::Result;

    fn handle(&mut self, _msg: RetryConnect, _ctx: &mut Self::Context) -> Self::Result {
        log::info!(
            "Client connection has failed, it will retry to re-connect in the next request."
        );
        self.retry_connect = true;
    }
}

impl StreamHandler<Notification, Error> for JsonRpcClient {
    fn handle(
        &mut self,
        Notification(subscription_id, value): Notification,
        _ctx: &mut Self::Context,
    ) {
        if let Some(Subscribe(ref request, ref recipient)) =
            self.subscriptions.get(&subscription_id)
        {
            let method = subscription_method_from_request(request);
            if let Err(err) = recipient.do_send(Notification(method, value)) {
                log::error!("Client couldn't notify subscriber: {}", err);
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

/// Extract a subscription method from a JSONRPC request
fn subscription_method_from_request(request: &Request) -> String {
    request
        .params
        .get(0)
        .cloned()
        .map(serde_json::from_value)
        .expect("Subscriptions should always have a method")
        .expect("Subscription methods should always be String")
}
