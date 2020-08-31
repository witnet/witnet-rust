use std::net::SocketAddr;
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use witnet_net::client::tcp::JsonRpcClient;

use super::*;
use crate::actors;

pub struct Params {
    pub testnet: bool,
    pub worker: Addr<actors::Worker>,
    pub client: Arc<RwLock<NodeClient>>,
    pub server_addr: SocketAddr,
    pub session_expires_in: Duration,
    pub requests_timeout: Duration,
}

#[derive(Clone)]
pub struct NodeClient {
    pub url: String,
    pub actor: Addr<JsonRpcClient>,
}
