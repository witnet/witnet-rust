use actix::prelude::*;

use witnet_crypto::mnemonic::Seed;

use crate::actors::{crypto::Error, Crypto};
use crate::wallet;

pub struct GenSeed(pub wallet::SeedSource);

impl Message for GenSeed {
    type Result = Result<Seed, Error>;
}

impl Handler<GenSeed> for Crypto {
    type Result = <GenSeed as Message>::Result;

    fn handle(&mut self, GenSeed(seed_source): GenSeed, _ctx: &mut Self::Context) -> Self::Result {
        self.gen_seed(seed_source)
    }
}
