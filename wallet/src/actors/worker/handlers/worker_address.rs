use crate::actors::worker;
use actix::{Actor, Addr, Handler, Message};

pub struct WorkerAddress(pub Addr<worker::Worker>);

impl Message for WorkerAddress {
    type Result = ();
}

impl Handler<WorkerAddress> for worker::Worker {
    type Result = ();

    fn handle(&mut self, msg: WorkerAddress, _ctx: &mut <Self as Actor>::Context) -> Self::Result {
        self.own_address = Some(msg.0);
    }
}
