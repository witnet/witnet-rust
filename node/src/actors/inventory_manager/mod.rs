//! # InventoryManager actor
//! InventoryManager is the actor in charge of managing the entire life cycle of all inventory items (i.e. transactions and blocks).
//! It acts as a single entry point for getting and putting inventory items from and into StorageManager. This creates one more degree of abstraction between how storage works and the node business logic of the app.mod actor;

use crate::utils::stop_system_if_panicking;
use thiserror::Error;
use witnet_data_structures::chain::PointerToBlock;

mod actor;
mod handlers;

/// InventoryManager actor
#[derive(Debug, Default)]
pub struct InventoryManager;

impl Drop for InventoryManager {
    fn drop(&mut self) {
        log::trace!("Dropping InventoryManager");
        stop_system_if_panicking("InventoryManager");
    }
}

/// Possible errors when interacting with InventoryManager
#[derive(Debug, Error)]
pub enum InventoryManagerError {
    /// An item does not exist
    #[error("Item not found")]
    ItemNotFound,
    /// A transaction pointer exists, but the corresponding block does not
    #[error("A transaction pointer exists, but the corresponding block does not: {0:?}")]
    NoPointedBlock(PointerToBlock),
    /// A transaction pointer exists, but the corresponding block does not contain that transaction
    #[error(
        "A transaction pointer exists, but the corresponding block does not contain that transaction: {0:?}"
    )]
    NoTransactionInPointedBlock(PointerToBlock),
    /// MailBoxError
    #[error("{0}")]
    MailBoxError(anyhow::Error),
}
