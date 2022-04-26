use actix::{prelude::*, ActorFutureExt, Context, Handler, ResponseActFuture, WrapFuture};

use super::{InventoryManager, InventoryManagerError};
use crate::{
    actors::messages::{
        AddItem, AddItems, GetItem, GetItemBlock, GetItemSuperblock, GetItemTransaction,
        StoreInventoryItem, SuperBlockNotify,
    },
    storage_mngr,
};
use witnet_data_structures::{
    chain::{Block, Epoch, Hash, Hashable, InventoryEntry, InventoryItem, PointerToBlock},
    transaction::Transaction,
};

fn key_superblock(superblock_index: u32) -> Vec<u8> {
    // Add 0 padding to the left of the superblock index to make sorted keys represent consecutive
    // indexes
    format!("SUPERBLOCK-{:010}", superblock_index).into()
}

impl InventoryManager {
    fn handle_add_items(
        &mut self,
        msg: AddItems,
    ) -> ResponseActFuture<Self, Result<(), InventoryManagerError>> {
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
        Box::pin(
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

    fn handle_get_item_block(
        &mut self,
        msg: GetItemBlock,
    ) -> ResponseActFuture<Self, Result<Block, InventoryManagerError>> {
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

        Box::pin(fut)
    }

    fn handle_get_item_transaction(
        &mut self,
        msg: GetItemTransaction,
    ) -> ResponseActFuture<Self, Result<(Transaction, PointerToBlock, Epoch), InventoryManagerError>>
    {
        let key = match msg.hash {
            Hash::SHA256(x) => x.to_vec(),
        };

        let fut = storage_mngr::get::<_, PointerToBlock>(&key)
            .into_actor(self)
            .then(move |res, act, _ctx| match res {
                Ok(Some(pointer_to_block)) => {
                    // Recursion
                    let fut = act
                        .handle_get_item_block(GetItemBlock {
                            hash: pointer_to_block.block_hash,
                        })
                        .then(move |res, _, _| {
                            match res {
                                Ok(block) => {
                                    // Read transaction from block
                                    let tx = block.txns.get(pointer_to_block.transaction_index);
                                    match tx {
                                        Some(tx) if tx.hash() == msg.hash => {
                                            let block_epoch = block.block_header.beacon.checkpoint;
                                            fut::ok((tx, pointer_to_block, block_epoch))
                                        }
                                        Some(_tx) => {
                                            // The transaction hash does not match
                                            fut::err(
                                                InventoryManagerError::NoTransactionInPointedBlock(
                                                    pointer_to_block,
                                                ),
                                            )
                                        }
                                        None => fut::err(
                                            InventoryManagerError::NoTransactionInPointedBlock(
                                                pointer_to_block,
                                            ),
                                        ),
                                    }
                                }
                                Err(InventoryManagerError::ItemNotFound) => fut::err(
                                    InventoryManagerError::NoPointedBlock(pointer_to_block),
                                ),
                                Err(e) => {
                                    log::error!("Couldn't get item from storage: {}", e);
                                    fut::err(e)
                                }
                            }
                        });

                    Box::pin(fut)
                }
                Ok(None) => {
                    let fut: ResponseActFuture<
                        Self,
                        Result<(Transaction, PointerToBlock, Epoch), InventoryManagerError>,
                    > = Box::pin(fut::err(InventoryManagerError::ItemNotFound));
                    fut
                }
                Err(e) => {
                    log::error!("Couldn't get item from storage: {}", e);
                    let fut: ResponseActFuture<
                        Self,
                        Result<(Transaction, PointerToBlock, Epoch), InventoryManagerError>,
                    > = Box::pin(fut::err(InventoryManagerError::MailBoxError(e)));
                    fut
                }
            });

        Box::pin(fut)
    }

    fn handle_get_item_superblock(
        &mut self,
        msg: GetItemSuperblock,
    ) -> ResponseActFuture<Self, Result<SuperBlockNotify, InventoryManagerError>> {
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

        Box::pin(fut)
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

/// Handler for AddItem message
impl Handler<AddItem> for InventoryManager {
    type Result = ResponseActFuture<Self, Result<(), InventoryManagerError>>;

    fn handle(&mut self, msg: AddItem, _ctx: &mut Context<Self>) -> Self::Result {
        // Simply calls AddItems with 1 item
        self.handle_add_items(AddItems {
            items: vec![msg.item],
        })
    }
}

/// Handler for AddItems message
impl Handler<AddItems> for InventoryManager {
    type Result = ResponseActFuture<Self, Result<(), InventoryManagerError>>;

    fn handle(&mut self, msg: AddItems, _ctx: &mut Context<Self>) -> Self::Result {
        self.handle_add_items(msg)
    }
}

/// Handler for GetItem message
impl Handler<GetItem> for InventoryManager {
    type Result = ResponseActFuture<Self, Result<InventoryItem, InventoryManagerError>>;

    fn handle(&mut self, msg: GetItem, _ctx: &mut Context<Self>) -> Self::Result {
        let fut: Self::Result = match msg.item {
            InventoryEntry::Tx(hash) => Box::pin(
                self.handle_get_item_transaction(GetItemTransaction { hash })
                    .map_ok(|(tx, _pointer_to_block, _epoch), _, _| InventoryItem::Transaction(tx)),
            ),
            InventoryEntry::Block(hash) => Box::pin(
                self.handle_get_item_block(GetItemBlock { hash })
                    .map_ok(|block, _, _| InventoryItem::Block(block)),
            ),
            InventoryEntry::SuperBlock(superblock_index) => Box::pin(
                self.handle_get_item_superblock(GetItemSuperblock { superblock_index })
                    .map_ok(|superblock_notify, _, _| {
                        InventoryItem::SuperBlock(superblock_notify.superblock)
                    }),
            ),
        };

        fut
    }
}

/// Handler for GetItem message
impl Handler<GetItemBlock> for InventoryManager {
    type Result = ResponseActFuture<Self, Result<Block, InventoryManagerError>>;

    fn handle(&mut self, msg: GetItemBlock, _ctx: &mut Context<Self>) -> Self::Result {
        self.handle_get_item_block(msg)
    }
}

/// Handler for GetItem message
impl Handler<GetItemTransaction> for InventoryManager {
    type Result = ResponseActFuture<
        Self,
        Result<(Transaction, PointerToBlock, Epoch), InventoryManagerError>,
    >;

    fn handle(&mut self, msg: GetItemTransaction, _ctx: &mut Context<Self>) -> Self::Result {
        self.handle_get_item_transaction(msg)
    }
}

/// Handler for GetItemSuperblock message
impl Handler<GetItemSuperblock> for InventoryManager {
    type Result = ResponseActFuture<Self, Result<SuperBlockNotify, InventoryManagerError>>;

    fn handle(&mut self, msg: GetItemSuperblock, _ctx: &mut Context<Self>) -> Self::Result {
        self.handle_get_item_superblock(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actors::chain_manager::mining::build_block, config_mngr, storage_mngr,
        utils::test_actix_system,
    };
    use std::sync::Arc;
    use witnet_config::config::{Config, StorageBackend};
    use witnet_data_structures::{
        chain::{
            CheckpointBeacon, EpochConstants, Input, KeyedSignature, OutputPointer, PublicKeyHash,
            TransactionsPool, ValueTransferOutput,
        },
        data_request::DataRequestPool,
        transaction::{Transaction, VTTransaction, VTTransactionBody},
        utxo_pool::UnspentOutputsPool,
        vrf::BlockEligibilityClaim,
    };

    const INITIAL_BLOCK_REWARD: u64 = 250 * 1_000_000_000;
    const HALVING_PERIOD: u32 = 3_500_000;

    static MILLION_TX_OUTPUT: &str =
        "0f0f000000000000000000000000000000000000000000000000000000000000:0";
    static MY_PKH_1: &str = "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm";

    fn build_block_with_vt_transactions(block_epoch: u32) -> Block {
        let output1_pointer: OutputPointer = MILLION_TX_OUTPUT.parse().unwrap();
        let input = vec![Input::new(output1_pointer.clone())];
        let vto1 = ValueTransferOutput {
            value: 1,
            ..Default::default()
        };
        let vto2 = ValueTransferOutput {
            value: 2,
            ..Default::default()
        };
        let vto3 = ValueTransferOutput {
            value: 3,
            ..Default::default()
        };
        let one_output = vec![vto1.clone()];
        let two_outputs = vec![vto1.clone(), vto2];
        let two_outputs2 = vec![vto1, vto3];

        let vt_body_one_output = VTTransactionBody::new(input.clone(), one_output);
        let vt_body_two_outputs1 = VTTransactionBody::new(input.clone(), two_outputs);
        let vt_body_two_outputs2 = VTTransactionBody::new(input, two_outputs2);

        // Build sample transactions
        let vt_tx1 = VTTransaction::new(vt_body_one_output, vec![]);
        let vt_tx2 = VTTransaction::new(vt_body_two_outputs1, vec![]);
        let vt_tx3 = VTTransaction::new(vt_body_two_outputs2, vec![]);

        let transaction_1 = Transaction::ValueTransfer(vt_tx1.clone());
        let transaction_2 = Transaction::ValueTransfer(vt_tx2);
        let transaction_3 = Transaction::ValueTransfer(vt_tx3);

        // Set `max_vt_weight` to fit only `transaction_1` weight
        let max_vt_weight = vt_tx1.weight();
        let max_dr_weight = 0;

        // Insert transactions into `transactions_pool`
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1, 1);
        transaction_pool.insert(transaction_2, 10);
        transaction_pool.insert(transaction_3, 10);
        assert_eq!(transaction_pool.vt_len(), 3);

        let mut unspent_outputs_pool = UnspentOutputsPool::default();
        let output1 = ValueTransferOutput {
            time_lock: 0,
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1_000_000,
        };
        unspent_outputs_pool.insert(output1_pointer.clone(), output1, 0);
        assert!(unspent_outputs_pool.contains_key(&output1_pointer));

        let dr_pool = DataRequestPool::default();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon {
            checkpoint: block_epoch,
            hash_prev_block: Hash::default(),
        };
        let block_number = 1;
        let block_proof = BlockEligibilityClaim::default();
        let collateral_minimum = 1_000_000_000;

        // Build block with

        let (block_header, txns) = build_block(
            (&mut transaction_pool, &unspent_outputs_pool, &dr_pool),
            max_vt_weight,
            max_dr_weight,
            block_beacon,
            block_proof,
            &[],
            PublicKeyHash::default(),
            EpochConstants::default(),
            block_number,
            collateral_minimum,
            None,
            None,
            0,
            INITIAL_BLOCK_REWARD,
            HALVING_PERIOD,
            0,
        );

        Block::new(block_header, KeyedSignature::default(), txns)
    }

    #[test]
    fn persist_same_transaction_twice_overwrites() {
        // Create two blocks with the same transaction, to simulate a reorganization.
        // GetItemTransaction should return the hash of the last block that was added using
        // AddItem.
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();
            let inventory_manager = InventoryManager::default().start();

            // Create first block with value transfer transactions
            let block = build_block_with_vt_transactions(1);
            let block_hash1 = block.hash();
            let tx_hash1 = block.txns.value_transfer_txns[0].hash();
            let item = StoreInventoryItem::Block(Box::new(block));

            // Persist first block
            let res = inventory_manager.send(AddItem { item }).await.unwrap();
            res.unwrap();

            // Get first transaction of that block
            let res = inventory_manager
                .send(GetItemTransaction { hash: tx_hash1 })
                .await
                .unwrap();

            // The transaction pointer should point to that block
            let (_tx, tx_pointer1, _tx_epoch1) = res.unwrap();
            assert_eq!(tx_pointer1.block_hash, block_hash1);

            // Create a different block with the same transactions
            let block = build_block_with_vt_transactions(2);
            let block_hash2 = block.hash();
            let tx_hash2 = block.txns.value_transfer_txns[0].hash();
            assert_ne!(block_hash1, block_hash2);
            assert_eq!(tx_hash1, tx_hash2);
            let item = StoreInventoryItem::Block(Box::new(block));

            // Persist second block
            let res = inventory_manager.send(AddItem { item }).await.unwrap();
            res.unwrap();

            // Get first transaction again
            let res = inventory_manager
                .send(GetItemTransaction { hash: tx_hash1 })
                .await
                .unwrap();

            // Now, the transaction pointer should point to the second block
            let (_tx, tx_pointer2, _tx_epoch2) = res.unwrap();
            assert_eq!(tx_pointer2.block_hash, block_hash2);
        });
    }
}
