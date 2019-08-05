use actix::prelude::*;

use crate::actors::app;

pub struct NotifyBalances;

impl Message for NotifyBalances {
    type Result = ();
}

impl Handler<NotifyBalances> for app::App {
    type Result = <NotifyBalances as Message>::Result;

    fn handle(&mut self, _msg: NotifyBalances, _ctx: &mut Self::Context) -> Self::Result {
        self.notify_balances()
    }
}
