//! # InventoryManager actor
//! InventoryManager is the actor in charge of managing the entire life cycle of all inventory items (i.e. transactions and blocks).
//! It acts as a single entry point for getting and putting inventory items from and into StorageManager. This creates one more degree of abstraction between how storage works and the core business logic of the app.mod actor;

mod actor;

/// UtxoManager actor
#[derive(Default)]
pub struct InventoryManager;
