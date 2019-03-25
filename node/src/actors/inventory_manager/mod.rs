//! # InventoryManager actor
//! InventoryManager is the actor in charge of managing the entire life cycle of all inventory items (i.e. transactions and blocks).
//! It acts as a single entry point for getting and putting inventory items from and into StorageManager. This creates one more degree of abstraction between how storage works and the node business logic of the app.mod actor;

use std::fmt;

mod actor;
mod handlers;

/// InventoryManager actor
#[derive(Default)]
pub struct InventoryManager;

/// Possible errors when interacting with InventoryManager
#[derive(Debug)]
pub enum InventoryManagerError {
    /// An item being processed was already known to this node
    ItemAlreadyExists,
    /// An item does not exist
    ItemDoesNotExist,
    /// MailBoxError
    MailBoxError,
}

impl fmt::Display for InventoryManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InventoryManagerError::{:?}", self)
    }
}
