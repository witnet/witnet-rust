use std::{net::SocketAddr, sync::Arc, time::Duration};

use witnet_net::client::tcp::JsonRpcClient;

use crate::actors;

use super::*;

pub struct Params {
    pub testnet: bool,
    pub worker: Addr<actors::Worker>,
    pub client: Arc<NodeClient>,
    pub server_addr: SocketAddr,
    pub session_expires_in: Duration,
    pub requests_timeout: Duration,
}

pub struct NodeClient {
    pub url: String,
    pub actor: Addr<JsonRpcClient>,
}
