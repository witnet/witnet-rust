use actix::{Handler, Message};

use crate::{actors::worker, types};
use witnet_data_structures::chain::StateMachine;

pub struct NodeStatusRequest {
    pub status: StateMachine,
    pub wallet: types::SessionWallet,
    pub sink: types::DynamicSink,
}

impl Message for NodeStatusRequest {
    type Result = worker::Result<()>;
}

impl Handler<NodeStatusRequest> for worker::Worker {
    type Result = <NodeStatusRequest as Message>::Result;

    fn handle(&mut self, msg: NodeStatusRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.handle_node_status(msg.status, msg.wallet, msg.sink)
    }
}
