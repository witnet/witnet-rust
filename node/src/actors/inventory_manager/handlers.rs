use actix::prelude::*;
use actix::{ActorFuture, Context, Handler, ResponseActFuture, WrapFuture};
use log;

use super::{InventoryManager, InventoryManagerError};
use crate::actors::messages::{AddItem, GetItem};
use crate::storage_mngr;
use witnet_data_structures::chain::{Hash, Hashable, InventoryItem};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

/// Handler for AddItem message
impl Handler<AddItem> for InventoryManager {
    type Result = ResponseActFuture<Self, (), InventoryManagerError>;

    fn handle(&mut self, msg: AddItem, _ctx: &mut Context<Self>) -> Self::Result {
        let hash = match &msg.item {
            InventoryItem::Block(item) => item.hash(),
            InventoryItem::Transaction(item) => item.hash(),
        };

        let key = match hash {
            Hash::SHA256(h) => h.to_vec(),
        };
        let fut = storage_mngr::put(&key, &msg.item)
            .into_actor(self)
            .map_err(|e, _, _| {
                log::error!("Couldn't persist item in storage: {}", e);
                InventoryManagerError::MailBoxError(e)
            })
            .and_then(|_, _, _| {
                log::debug!("Successfully persisted item in storage");
                fut::ok(())
            });

        Box::new(fut)
    }
}

/// Handler for GetItem message
impl Handler<GetItem> for InventoryManager {
    type Result = ResponseActFuture<Self, InventoryItem, InventoryManagerError>;

    fn handle(&mut self, msg: GetItem, _ctx: &mut Context<Self>) -> Self::Result {
        let key = match msg.hash {
            Hash::SHA256(x) => x.to_vec(),
        };

        let fut = storage_mngr::get::<_, InventoryItem>(&key)
            .into_actor(self)
            .map_err(|e, _, _| {
                log::error!("Couldn't get item from storage: {}", e);
                InventoryManagerError::MailBoxError(e)
            })
            .and_then(|opt, _, _| match opt {
                None => fut::err(InventoryManagerError::ItemNotFound),
                Some(item) => fut::ok(item),
            });

        Box::new(fut)
    }
}
