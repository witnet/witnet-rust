use actix::prelude::*;

use witnet_net::client::tcp::jsonrpc;

use crate::actors::app;

impl Handler<jsonrpc::NotifySubscriptionTopic> for app::App {
    type Result = <jsonrpc::NotifySubscriptionTopic as Message>::Result;

    fn handle(
        &mut self,
        jsonrpc::NotifySubscriptionTopic { topic, value }: jsonrpc::NotifySubscriptionTopic,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.handle_notification(topic, value).ok();
    }
}
