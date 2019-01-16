use actix::{ActorFuture, Context, ContextFutureSpawner, Handler, System, WrapFuture};
use ansi_term::Color::Yellow;
use log::{debug, error, info, warn};

use super::messages::{AddNewBlock, GetHighestCheckpointBeacon};
use super::validations::{block_reward, merkle_tree_root};
use super::ChainManager;
use crate::actors::reputation_manager::{messages::ValidatePoE, ReputationManager};

use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::chain::{
    Block, BlockHeader, CheckpointBeacon, Hash, LeadershipProof, Output, PublicKeyHash,
    Secp256k1Signature, Signature, Transaction, TransactionsPool, ValueTransferOutput,
};
use witnet_storage::storage::Storable;

impl ChainManager {
    /// Try to mine a block
    pub fn try_mine_block(&mut self, ctx: &mut Context<Self>) {
        if self.current_epoch.is_none() {
            warn!("Cannot mine a block because current epoch is unknown");

            return;
        }

        let current_epoch = self.current_epoch.unwrap();

        debug!("Periodic epoch notification received {:?}", current_epoch);

        // Check eligibility
        // S(H(beacon))
        let mut beacon = match self.handle(GetHighestCheckpointBeacon, ctx) {
            Ok(b) => b,
            _ => return,
        };

        if beacon.checkpoint > current_epoch {
            // We got a block from the future
            error!(
                "The current highest checkpoint beacon is from the future ({:?} > {:?})",
                beacon.checkpoint, current_epoch
            );
            return;
        }
        if beacon.checkpoint == current_epoch {
            // For some reason we already got a valid block for this epoch
            // TODO: Check eligibility anyway?
        }
        // The highest checkpoint beacon should contain the current epoch
        beacon.checkpoint = current_epoch;
        let beacon_hash = Hash::from(calculate_sha256(&beacon.to_bytes().unwrap()));
        let private_key = 1;

        // TODO: send Sign message to CryptoManager
        let sign = |x, _k| match x {
            Hash::SHA256(mut x) => {
                // Add some randomness to the signature
                x[0] = self.random as u8;
                x
            }
        };
        let signed_beacon_hash = sign(beacon_hash, private_key);
        // Currently, every hash is valid
        // Fake signature which will be accepted anyway
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: signed_beacon_hash,
            s: signed_beacon_hash,
            v: 0,
        });
        let leadership_proof = LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        };

        // Send ValidatePoE message to ReputationManager
        let reputation_manager_addr = System::current().registry().get::<ReputationManager>();
        reputation_manager_addr
            .send(ValidatePoE {
                beacon,
                proof: leadership_proof.clone(),
            })
            .into_actor(self)
            .drop_err()
            .and_then(move |eligible, act, ctx| {
                if eligible {
                    info!(
                        "{} Discovered eligibility for mining a block for epoch #{}",
                        Yellow.bold().paint("[Mining]"),
                        Yellow.bold().paint(beacon.checkpoint.to_string())
                    );
                    // Send proof of eligibility to chain manager,
                    // which will construct and broadcast the block

                    // Build the block using the supplied beacon and eligibility proof
                    let block = build_block(
                        &act.transactions_pool,
                        act.max_block_weight,
                        beacon,
                        leadership_proof,
                    );

                    // Send AddNewBlock message to self
                    act.handle(AddNewBlock { block }, ctx);
                }
                actix::fut::ok(())
            })
            .wait(ctx);
    }
}

/// Build a new Block using the supplied leadership proof and by filling transactions from the
/// `transaction_pool`
fn build_block(
    transactions_pool: &TransactionsPool,
    max_block_weight: u32,
    beacon: CheckpointBeacon,
    proof: LeadershipProof,
) -> Block {
    // Get all the unspent transactions and calculate the sum of their fees
    let mut transaction_fees = 0;
    let mut block_weight = 0;
    let mut transactions = Vec::new();

    // Insert empty Transaction (future Mint Transaction)
    transactions.push(Transaction::default());

    // Push transactions from pool until `max_block_weight` is reached
    // TODO: refactor this statement into a functional `try_fold`
    for transaction in transactions_pool.iter() {
        // Currently, 1 weight unit is equivalent to 1 byte
        let transaction_weight = transaction.size();
        // FIXME (anler): Remove unwrap and handle error correctly
        let transaction_fee = transaction.fee(transactions_pool).unwrap();
        let new_block_weight = block_weight + transaction_weight;

        if new_block_weight <= max_block_weight {
            transactions.push(transaction.clone());
            transaction_fees += transaction_fee;
            block_weight += transaction_weight;

            if new_block_weight == max_block_weight {
                break;
            }
        }
    }

    // Include Mint Transaction by miner
    // TODO: Include Witnet's node PKH (keyed signature is not needed as there is no input)
    let pkh = PublicKeyHash::default();
    let epoch = beacon.checkpoint;
    let reward = block_reward(epoch) + transaction_fees;

    // Build Mint Transaction
    transactions[0]
        .outputs
        .push(Output::ValueTransfer(ValueTransferOutput {
            pkh,
            value: reward,
        }));

    // Compute `hash_merkle_root` and build block header
    let hash_merkle_root = merkle_tree_root(&transactions);
    let block_header = BlockHeader {
        version: 0,
        beacon,
        hash_merkle_root,
    };

    Block {
        block_header,
        proof,
        txns: transactions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use witnet_data_structures::chain::*;

    #[test]
    fn build_empty_block() {
        // Initialize transaction_pool with 1 transaction
        let mut transaction_pool = TransactionsPool::default();
        let transaction = Transaction {
            version: 0,
            inputs: vec![],
            outputs: vec![],
            signatures: vec![],
        };
        transaction_pool.insert(transaction.hash(), transaction.clone());

        // Set `max_block_weight` to zero (no transaction should be included)
        let max_block_weight = 0;

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = LeadershipProof {
            block_sig: None,
            influence: 0,
        };

        // Build empty block (because max weight is zero)
        let block = build_block(
            &transaction_pool,
            max_block_weight,
            block_beacon,
            block_proof,
        );

        // Check if block only contains the Mint Transaction
        assert_eq!(block.txns.len(), 1);
        assert_eq!(block.txns[0].inputs.len(), 0);
        assert_eq!(block.txns[0].outputs.len(), 1);
        assert_eq!(block.txns[0].signatures.len(), 0);

        // Check that transaction in block is not the transaction in `transactions_pool`
        assert_ne!(block.txns[0], transaction);
    }

    #[test]
    #[ignore]
    fn build_block_with_transactions() {
        // Build sample transactions
        let transaction_1 = Transaction {
            version: 0,
            inputs: vec![Input::ValueTransfer(ValueTransferInput {
                transaction_id: Hash::SHA256([1; 32]),
                output_index: 0,
            })],
            outputs: vec![Output::ValueTransfer(ValueTransferOutput {
                pkh: PublicKeyHash::default(),
                value: 1,
            })],
            signatures: vec![],
        };
        let transaction_2 = Transaction {
            version: 0,
            inputs: vec![
                Input::ValueTransfer(ValueTransferInput {
                    transaction_id: Hash::SHA256([2; 32]),
                    output_index: 0,
                }),
                Input::ValueTransfer(ValueTransferInput {
                    transaction_id: Hash::SHA256([3; 32]),
                    output_index: 0,
                }),
            ],
            outputs: vec![
                Output::ValueTransfer(ValueTransferOutput {
                    pkh: PublicKeyHash::default(),
                    value: 2,
                }),
                Output::ValueTransfer(ValueTransferOutput {
                    pkh: PublicKeyHash::default(),
                    value: 3,
                }),
            ],
            signatures: vec![],
        };
        let transaction_3 = Transaction {
            version: 0,
            inputs: vec![
                Input::ValueTransfer(ValueTransferInput {
                    transaction_id: Hash::SHA256([4; 32]),
                    output_index: 0,
                }),
                Input::ValueTransfer(ValueTransferInput {
                    transaction_id: Hash::SHA256([5; 32]),
                    output_index: 0,
                }),
            ],
            outputs: vec![
                Output::ValueTransfer(ValueTransferOutput {
                    pkh: PublicKeyHash::default(),
                    value: 4,
                }),
                Output::ValueTransfer(ValueTransferOutput {
                    pkh: PublicKeyHash::default(),
                    value: 5,
                }),
            ],
            signatures: vec![],
        };

        // Insert transactions into `transactions_pool`
        // TODO: Currently the insert function does not take into account the fees to compute the transaction's weight
        let mut transaction_pool = TransactionsPool::default();
        transaction_pool.insert(transaction_1.hash(), transaction_1.clone());
        transaction_pool.insert(transaction_2.hash(), transaction_2.clone());
        transaction_pool.insert(transaction_3.hash(), transaction_3.clone());

        // Set `max_block_weight` to fit only `transaction_1` size
        let max_block_weight = transaction_1.size();

        // Fields required to mine a block
        let block_beacon = CheckpointBeacon::default();
        let block_proof = LeadershipProof {
            block_sig: None,
            influence: 0,
        };

        // Build block with
        let block = build_block(
            &transaction_pool,
            max_block_weight,
            block_beacon,
            block_proof,
        );

        // Check if block contains only 2 transactions (Mint Transaction + 1 included transaction)
        assert_eq!(block.txns.len(), 2);

        // Check that first transaction is the Mint Transaction
        assert_eq!(block.txns[0].inputs.len(), 0);
        assert_eq!(block.txns[0].outputs.len(), 1);
        assert_eq!(block.txns[0].signatures.len(), 0);
        // Check that transaction in block is not a transaction from `transactions_pool`
        assert_ne!(block.txns[0], transaction_1);
        assert_ne!(block.txns[0], transaction_2);
        assert_ne!(block.txns[0], transaction_3);

        // Check that the included transaction is the only one that fits the `max_block_weight`
        assert_eq!(block.txns[1], transaction_1);
    }
}
