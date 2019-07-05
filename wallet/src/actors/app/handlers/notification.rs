use actix::prelude::*;

use witnet_net::client::tcp::jsonrpc as rpc_client;

use crate::actors::App;

impl Handler<rpc_client::Notification> for App {
    type Result = <rpc_client::Notification as Message>::Result;

    fn handle(
        &mut self,
        rpc_client::Notification(value): rpc_client::Notification,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(err) = self.handle_block_notification(value) {
            log::error!("{}", err);
        }
    }
}
