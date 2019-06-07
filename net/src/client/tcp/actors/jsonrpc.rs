//! Defines a JsonRPC over TCP actor.
//!
//! See the `JsonRpcClient` struct for more information.
use std::fmt;

use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    DuplexTransport as _, Error as TransportError, ErrorKind as TransportErrorKind, Transport as _,
};
use futures::Future;
use serde::Serialize;
use serde_json::{error::Error as JsonError, value, Value};

/// Possible types of errors that can occurr when sending requests.
#[derive(Debug)]
pub enum Error {
    /// The error ocurred at the transport layer, e.g.: connection or event loop might be down.
    Transport(TransportError),
    /// The error ocurred when serializaing the request params to json.
    Serialization(JsonError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Transport(ref e) => write!(fmt, "{}", e),
            Error::Serialization(ref e) => write!(fmt, "{}", e),
        }
    }
}

/// Json-RPC Client actor.
///
/// Use this actor to send json-rpc requests over a websockets connection.
pub struct JsonRpcClient {
    _handle: EventLoopHandle,
    socket: TcpSocket,
    url: String,
    retry_connect: bool,
    subscriber: Option<SetSubscriber>,
    subscription_id: Option<String>,
}

impl JsonRpcClient {
    /// TODO: doc
    pub fn start(url: &str) -> Result<Addr<JsonRpcClient>, Error> {
        log::info!("Connecting client to {}", url);
        let (_handle, socket) = TcpSocket::new(url).map_err(Error::Transport)?;
        let client = Self {
            _handle,
            socket,
            url: url.to_owned(),
            retry_connect: false,
            subscriber: None,
            subscription_id: None,
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
        // TODO: re-subscribe
    }

    /// TODO: doc
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
                Error::Transport(err)
            })
    }
}

impl Actor for JsonRpcClient {
    type Context = Context<Self>;
}

/// TODO: doc
pub struct Notification(pub Value);

impl Message for Notification {
    type Result = ();
}

/// Request sent by the client.
#[derive(Debug, Clone)]
pub struct Request {
    method: String,
    params: Value,
}

impl Request {
    /// Create a new request with the given method.
    pub fn method<T: Into<String>>(method: T) -> Self {
        Self {
            method: method.into(),
            params: Value::Null,
        }
    }

    /// Set request params.
    pub fn params<T: Serialize>(mut self, params: T) -> Result<Self, Error> {
        self.params = value::to_value(params).map_err(Error::Serialization)?;
        Ok(self)
    }

    /// Set request params that are already a serialized value.
    pub fn value(mut self, params: Value) -> Self {
        self.params = params;
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
        Request { method, params }: Request,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.retry_connect {
            self.reconnect();
            ctx.notify(Subscribe);
        }

        let fut =
            self.send_request(method, params)
                .into_actor(self)
                .map_err(move |err, _act, ctx| {
                    if is_connection_error(&err) {
                        ctx.notify(RetryConnect);
                    }
                    err
                });

        Box::new(fut)
    }
}

/// TODO: doc
pub struct SetSubscriber(pub Recipient<Notification>, pub Request);

impl Message for SetSubscriber {
    type Result = ();
}

impl Handler<SetSubscriber> for JsonRpcClient {
    type Result = <SetSubscriber as Message>::Result;

    fn handle(&mut self, msg: SetSubscriber, ctx: &mut Self::Context) -> Self::Result {
        self.subscriber = Some(msg);
        ctx.notify(Subscribe);
    }
}

struct Subscribe;

impl Message for Subscribe {
    type Result = ();
}

impl Handler<Subscribe> for JsonRpcClient {
    type Result = <Subscribe as Message>::Result;

    fn handle(&mut self, _msg: Subscribe, ctx: &mut Self::Context) -> Self::Result {
        if let Some(SetSubscriber(recipient, request)) = self.subscriber.take() {
            ctx.address()
                .send(request.clone())
                .map_err(|err| log::error!("Couldn't subscribe: {}", err))
                .into_actor(self)
                .map(|resp, act, ctx| {
                    match resp {
                        Ok(Value::String(id)) => {
                            let stream = act
                                .socket
                                .subscribe(&id.clone().into())
                                .map(|value| {
                                    log::debug!(
                                        "<< Forwarding notification from node to subscriber"
                                    );
                                    Notification(value)
                                })
                                .map_err(Error::Transport);
                            Self::add_stream(stream, ctx);
                            act.subscription_id = Some(id);
                            log::info!("Client subscription created");
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

            self.subscriber = Some(SetSubscriber(recipient, request));
        }
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
    fn handle(&mut self, msg: Notification, _ctx: &mut Self::Context) {
        if let Some(SetSubscriber(ref recipient, _)) = self.subscriber {
            if let Err(err) = recipient.do_send(msg) {
                log::error!("Client couldn't notify subscriber: {}", err);
            }
        }
    }
}

fn is_connection_error(err: &Error) -> bool {
    match err {
        Error::Transport(err) => match err.kind() {
            TransportErrorKind::Transport(_) => true,
            _ => false,
        },
        _ => false,
    }
}
