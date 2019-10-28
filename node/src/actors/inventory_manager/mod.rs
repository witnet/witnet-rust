//! # InventoryManager actor
//! InventoryManager is the actor in charge of managing the entire life cycle of all inventory items (i.e. transactions and blocks).
//! It acts as a single entry point for getting and putting inventory items from and into StorageManager. This creates one more degree of abstraction between how storage works and the node business logic of the app.mod actor;

use failure::Fail;

mod actor;
mod handlers;

/// InventoryManager actor
#[derive(Default)]
pub struct InventoryManager;

/// Possible errors when interacting with InventoryManager
#[derive(Debug, Fail)]
pub enum InventoryManagerError {
    /// An item does not exist
    #[fail(display = "Item not found")]
    ItemNotFound,
    /// MailBoxError
    #[fail(display = "{}", _0)]
    MailBoxError(failure::Error),
}
