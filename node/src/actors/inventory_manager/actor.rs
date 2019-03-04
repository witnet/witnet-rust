use actix::{Actor, Context, Supervised, SystemService};

use super::InventoryManager;

/// Make actor from `InventoryManager`
impl Actor for InventoryManager {
    type Context = Context<Self>;
}

/// Required trait to be able to be managed by a Supervisor
impl Supervised for InventoryManager {}

/// Required trait to be able to be registered to the System as a unique Service
impl SystemService for InventoryManager {}
