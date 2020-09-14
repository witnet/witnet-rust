use actix::prelude::*;
use actix::{ActorFuture, Context, Handler, ResponseActFuture, WrapFuture};

use super::{InventoryManager, InventoryManagerError};
use crate::actors::messages::{
    AddItem, AddItems, GetItem, GetItemBlock, GetItemSuperblock, GetItemTransaction,
    StoreInventoryItem, SuperBlockNotify,
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

    fn handle(&mut self, msg: AddItem, ctx: &mut Context<Self>) -> Self::Result {
        // Simply calls AddItems with 1 item
        self.handle(
            AddItems {
                items: vec![msg.item],
            },
            ctx,
        )
    }
}

/// Handler for AddItems message
impl Handler<AddItems> for InventoryManager {
    type Result = ResponseActFuture<Self, (), InventoryManagerError>;

    fn handle(&mut self, msg: AddItems, _ctx: &mut Context<Self>) -> Self::Result {
        let mut blocks_to_add = vec![];
        let mut transactions_to_add = vec![];
        let mut superblocks_to_add = vec![];

        for item in msg.items {
            match item {
                StoreInventoryItem::Block(block) => {
                    let block_hash = block.hash();
                    let key = match block_hash {
                        Hash::SHA256(h) => h.to_vec(),
                    };
                    // Store the block and all the transactions
                    let items_to_add = block.txns.create_pointers_to_transactions(block_hash);
                    blocks_to_add.push((key, block));
                    transactions_to_add.extend(items_to_add.into_iter().map(
                        |(tx_hash, pointer_to_block)| {
                            let key = match tx_hash {
                                Hash::SHA256(h) => h.to_vec(),
                            };

                            (key, pointer_to_block)
                        },
                    ));
                }
                StoreInventoryItem::Transaction(hash, pointer_to_block) => {
                    let key = match hash {
                        Hash::SHA256(h) => h.to_vec(),
                    };

                    transactions_to_add.push((key, pointer_to_block));
                }
                StoreInventoryItem::Superblock(superblock_notify) => {
                    let superblock_index = superblock_notify.superblock.index;
                    let key = key_superblock(superblock_index);
                    superblocks_to_add.push((key, superblock_notify));
                }
            }
        }

        let block_len = blocks_to_add.len();
        let tx_len = transactions_to_add.len();
        let superblock_len = superblocks_to_add.len();

        log::trace!("Persisting {} blocks to storage", block_len);

        // Store all the blocks, and then store all the transactions
        Box::new(
            storage_mngr::put_batch(&blocks_to_add)
                .into_actor(self)
                .map_err(|e, _, _| {
                    log::error!("Error when writing blocks to storage: {}", e);

                    InventoryManagerError::MailBoxError(e)
                })
                .and_then(move |(), act, _| {
                    log::trace!("Successfully persisted {} blocks to storage", block_len);
                    log::trace!("Persisting {} transactions to storage", tx_len);

                    storage_mngr::put_batch(&transactions_to_add)
                        .into_actor(act)
                        .map_err(|e, _, _| {
                            log::error!("Error when writing transactions to storage: {}", e);

                            InventoryManagerError::MailBoxError(e)
                        })
                })
                .and_then(move |(), act, _| {
                    log::trace!("Successfully persisted {} transactions to storage", tx_len);

                    storage_mngr::put_batch(&superblocks_to_add)
                        .into_actor(act)
                        .map_err(|e, _, _| {
                            log::error!("Error when writing superblocks to storage: {}", e);

                            InventoryManagerError::MailBoxError(e)
                        })
                })
                .and_then(move |(), _, _| {
                    log::trace!(
                        "Successfully persisted {} superblocks to storage",
                        superblock_len
                    );

                    actix::fut::ok(())
                }),
        )
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
            InventoryEntry::SuperBlock(superblock_index) => Box::new(
                self.handle(GetItemSuperblock { superblock_index }, ctx)
                    .map(|superblock_notify, _, _| {
                        InventoryItem::SuperBlock(superblock_notify.superblock)
                    }),
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

fn key_superblock(superblock_index: u32) -> Vec<u8> {
    // Add 0 padding to the left of the superblock index to make sorted keys represent consecutive
    // indexes
    format!("SUPERBLOCK-{:010}", superblock_index).into()
}

/// Handler for GetItemSuperblock message
impl Handler<GetItemSuperblock> for InventoryManager {
    type Result = ResponseActFuture<Self, SuperBlockNotify, InventoryManagerError>;

    fn handle(&mut self, msg: GetItemSuperblock, _ctx: &mut Context<Self>) -> Self::Result {
        let key = key_superblock(msg.superblock_index);

        let fut = storage_mngr::get::<_, SuperBlockNotify>(&key)
            .into_actor(self)
            .then(move |res, _, _| match res {
                Ok(opt) => match opt {
                    None => fut::err(InventoryManagerError::ItemNotFound),
                    Some(superblock) => fut::ok(superblock),
                },
                Err(e) => {
                    log::error!("Couldn't get item from storage: {}", e);

                    fut::err(InventoryManagerError::MailBoxError(e))
                }
            });

        Box::new(fut)
    }
}
