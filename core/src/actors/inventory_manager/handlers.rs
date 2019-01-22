use crate::actors::inventory_manager::{
    messages::{AddItem, GetItem},
    InventoryManager, InventoryManagerError,
};
use actix::{ActorFuture, Context, Handler, ResponseActFuture, System, WrapFuture};
use witnet_data_structures::chain::{Hash, Hashable, InventoryItem};

use crate::actors::storage_manager::{messages::Get, messages::Put, StorageManager};

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
        let add_msg = match hash {
            Hash::SHA256(h) => Put::from_value(h.to_vec(), &msg.item).unwrap(),
        };

        // Persist items into storage
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        let fut = storage_manager_addr
            .send(add_msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                let res = match res {
                    Ok(x) => x.map_err(|e| e.into()),
                    _ => Err(InventoryManagerError::MailBoxError),
                };
                actix::fut::result(res)
            });

        Box::new(fut)
    }
}

/// Handler for AddItem message
impl Handler<GetItem> for InventoryManager {
    type Result = ResponseActFuture<Self, InventoryItem, InventoryManagerError>;

    fn handle(&mut self, msg: GetItem, _ctx: &mut Context<Self>) -> Self::Result {
        let hash = match msg.hash {
            Hash::SHA256(x) => x.to_vec(),
        };
        let get_msg = Get::<InventoryItem>::new(hash);

        // Get items from storage
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        let fut = storage_manager_addr
            .send(get_msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                let res = match res {
                    Ok(x) => x.map_err(|e| e.into()).and_then(|x| match x {
                        Some(x) => Ok(x),
                        None => Err(InventoryManagerError::ItemDoesNotExist),
                    }),
                    _ => Err(InventoryManagerError::MailBoxError),
                };
                actix::fut::result(res)
            });

        Box::new(fut)
    }
}
