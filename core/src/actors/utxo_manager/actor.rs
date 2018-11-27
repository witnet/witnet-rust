use actix::{Actor, Context, Supervised, SystemService};

use super::UtxoManager;

/// Implement Actor trait for [UtxoManager](actors::utxo_manager::UtxoManager)
impl Actor for UtxoManager {
    type Context = Context<Self>;
}

/// Required trait for being able to retrieve UtxoManager address from registry
impl Supervised for UtxoManager {}

/// Required trait for being able to retrieve UtxoManager address from registry
impl SystemService for UtxoManager {}
