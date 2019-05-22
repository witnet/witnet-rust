//! # Create Mnemonics handler
//!
//! This handler is in charge of receiving a desired length for the mnemonics, ang generating a
//! mnemonic phrase with that amount of words.
//!
//! For more information on Mnemonics see the documentation of crate witnet_crypto.
use actix::prelude::*;
use serde::Deserialize;

use crate::actors::App;
use crate::error;
use witnet_crypto as crypto;

/// Message containing the desired length of the to-generate mnemonic.
#[derive(Debug, Deserialize)]
pub struct CreateMnemonics {
    length: crypto::mnemonic::Length,
}

impl Message for CreateMnemonics {
    type Result = Result<String, error::Error>;
}

impl Handler<CreateMnemonics> for App {
    type Result = Result<String, error::Error>;

    fn handle(
        &mut self,
        CreateMnemonics { length }: CreateMnemonics,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        log::debug!("Generating mnemonics with length: {:?}", length);
        let mnemonic = crypto::mnemonic::MnemonicGen::new()
            .with_len(length)
            .generate();
        let words = mnemonic.words();

        Ok(words.to_string())
    }
}
