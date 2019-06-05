use jsonrpc_core as rpc;
use serde_json as json;

pub struct ForwardRequest {
    pub method: String,
    pub params: rpc::Params,
}

pub type ForwardResponse = json::Value;
