use actix::prelude::*;
use actix::{ActorFuture, Context, Handler, ResponseActFuture, WrapFuture};
use log;

use super::{InventoryManager, InventoryManagerError};
use crate::actors::messages::{
    AddItem, AddItems, GetItem, GetItemBlock, GetItemTransaction, StoreInventoryItem,
};
use crate::storage_mngr;
use witnet_data_structures::chain::{
    Block, Hash, Hashable, InventoryEntry, InventoryItem, PointerToBlock,
};
use witnet_data_structures::transaction::Transaction;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

/// Handler for AddItem message
impl Handler<AddItem> for InventoryManager {
    type Result = ResponseActFuture<Self, (), InventoryManagerError>;

    fn handle(&mut self, msg: AddItem, _ctx: &mut Context<Self>) -> Self::Result {
        match msg.item {
            StoreInventoryItem::Block(block) => {
                let block_hash = block.hash();
                let key = match block_hash {
                    Hash::SHA256(h) => h.to_vec(),
                };
                let fut = storage_mngr::put(&key, &block)
                    .into_actor(self)
                    .map_err(|e, _, _| {
                        log::error!("Couldn't persist block in storage: {}", e);
                        InventoryManagerError::MailBoxError(e)
                    })
                    .and_then(move |_, _, ctx| {
                        log::trace!("Successfully persisted block in storage");
                        // Store all the transactions as well
                        let items_to_add = block.txns.create_pointers_to_transactions(block_hash);
                        let items = items_to_add
                            .into_iter()
                            .map(|(tx_hash, pointer_to_block)| {
                                StoreInventoryItem::Transaction(tx_hash, pointer_to_block)
                            })
                            .collect();
                        ctx.notify(AddItems { items });

                        fut::ok(())
                    });

                Box::new(fut)
            }
            StoreInventoryItem::Transaction(hash, pointer_to_block) => {
                let key = match hash {
                    Hash::SHA256(h) => h.to_vec(),
                };
                let fut = storage_mngr::put(&key, &pointer_to_block)
                    .into_actor(self)
                    .map_err(|e, _, _| {
                        log::error!("Couldn't persist transaction in storage: {}", e);
                        InventoryManagerError::MailBoxError(e)
                    })
                    .and_then(|_, _, _| {
                        log::trace!("Successfully persisted transaction in storage");
                        fut::ok(())
                    });

                Box::new(fut)
            }
        }
    }
}

/// Handler for AddItems message
impl Handler<AddItems> for InventoryManager {
    type Result = ();

    fn handle(&mut self, msg: AddItems, ctx: &mut Context<Self>) -> Self::Result {
        log::trace!("Persisting {} items in storage", msg.items.len());
        // FIXME(919): instead of calling AddItem in a for loop, persist the items in batch:
        // * Implement batch storage API
        // * Move the logic from AddItem here, and make AddItem call AddItems with vec![item]
        // * Return a better result than `()`
        for item in msg.items {
            ctx.notify(AddItem { item });
        }
    }
}

/// Handler for GetItem message
impl Handler<GetItem> for InventoryManager {
    type Result = ResponseActFuture<Self, InventoryItem, InventoryManagerError>;

    fn handle(&mut self, msg: GetItem, ctx: &mut Context<Self>) -> Self::Result {
        let fut: Self::Result = match msg.item {
            InventoryEntry::Tx(hash) => Box::new(
                self.handle(GetItemTransaction { hash }, ctx)
                    .map(|(tx, _pointer_to_block), _, _| InventoryItem::Transaction(tx)),
            ),
            InventoryEntry::Block(hash) => Box::new(
                self.handle(GetItemBlock { hash }, ctx)
                    .map(|block, _, _| InventoryItem::Block(block)),
            ),
        };

        fut
    }
}

/// Handler for GetItem message
impl Handler<GetItemBlock> for InventoryManager {
    type Result = ResponseActFuture<Self, Block, InventoryManagerError>;

    fn handle(&mut self, msg: GetItemBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let key = match msg.hash {
            Hash::SHA256(x) => x.to_vec(),
        };

        let fut = storage_mngr::get::<_, Block>(&key)
            .into_actor(self)
            .then(move |res, _, _| match res {
                Ok(opt) => match opt {
                    None => fut::err(InventoryManagerError::ItemNotFound),
                    Some(block) => fut::ok(block),
                },
                Err(e) => {
                    log::error!("Couldn't get item from storage: {}", e);

                    fut::err(InventoryManagerError::MailBoxError(e))
                }
            });

        Box::new(fut)
    }
}

/// Handler for GetItem message
impl Handler<GetItemTransaction> for InventoryManager {
    type Result = ResponseActFuture<Self, (Transaction, PointerToBlock), InventoryManagerError>;

    fn handle(&mut self, msg: GetItemTransaction, _ctx: &mut Context<Self>) -> Self::Result {
        let key = match msg.hash {
            Hash::SHA256(x) => x.to_vec(),
        };

        let fut = storage_mngr::get::<_, PointerToBlock>(&key)
            .into_actor(self)
            .then(|res, act, ctx| match res {
                Ok(opt) => match opt {
                    None => {
                        let fut: Self::Result =
                            Box::new(fut::err(InventoryManagerError::ItemNotFound));
                        fut
                    }
                    Some(pointer_to_block) => {
                        // Recursion
                        let fut = act
                            .handle(
                                GetItemBlock {
                                    hash: pointer_to_block.block_hash,
                                },
                                ctx,
                            )
                            .then(move |res, _, _| {
                                match res {
                                    Ok(block) => {
                                        // Read transaction from block
                                        let tx = block.txns.get(pointer_to_block.transaction_index);
                                        match tx {
                                            Some(tx) if tx.hash() == msg.hash => fut::ok((tx, pointer_to_block)),
                                            Some(_tx) => {
                                                // The transaction hash does not match
                                                fut::err(InventoryManagerError::NoTransactionInPointedBlock(pointer_to_block))
                                            }
                                            None => fut::err(
                                                InventoryManagerError::NoTransactionInPointedBlock(pointer_to_block),
                                            ),
                                        }
                                    }
                                    Err(InventoryManagerError::ItemNotFound) => {
                                        fut::err(InventoryManagerError::NoPointedBlock(pointer_to_block))
                                    }
                                    Err(e) => {
                                        log::error!("Couldn't get item from storage: {}", e);
                                        fut::err(e)
                                    }
                                }
                            });

                        Box::new(fut)
                    }
                },
                Err(e) => {
                    log::error!("Couldn't get item from storage: {}", e);
                    let fut: Self::Result =
                        Box::new(fut::err(InventoryManagerError::MailBoxError(e)));
                    fut
                }
            });

        Box::new(fut)
    }
}
