use super::*;

mod request_handlers;

pub use request_handlers::*;
#[cfg(test)]
mod request_handlers_tests;

pub trait Handler {
    type Result;

    fn handle(self, state: &types::State) -> Self::Result;
}
