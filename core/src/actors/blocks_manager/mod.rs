use actix::{Supervised, SystemService};

mod actor;
mod handlers;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// Block manager actor
#[derive(Default)]
pub struct BlocksManager {}

/// Required trait for being able to retrieve BlocksManager address from registry
impl Supervised for BlocksManager {}

/// Required trait for being able to retrieve BlocksManager address from registry
impl SystemService for BlocksManager {}
