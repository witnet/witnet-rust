//! # Wallet-Api request types

use jsonrpc_pubsub as pubsub;

use serde::Deserialize;

/// Used to obtain the ids of the different wallets that exist so you can pick one and open it with
/// `UnlockWallet`.
#[derive(Debug, Deserialize)]
pub struct GetWalletInfos;

// use jsonrpc_core as rpc;
// use jsonrpc_pubsub as pubsub;

// use witnet_data_structures as structs;
// use witnet_crypto as crypto;

// // TODO: Remove allow atribute when all strucst are used.
// #![allow(dead_code)]

//use jsonrpc_core::Value;
// use serde::{Deserialize, Serialize};

// /// TODO: Implement (radon crate)
// #[derive(Serialize)]
// pub struct RadonValue {}

// /// TODO: Question. Is this pkh?
// #[derive(Serialize)]
// pub struct Address {}

// /// TODO: Implement (import data_structures crate)
// #[derive(Serialize)]
// pub struct Transaction {}

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct Wallet {
//     pub(crate) version: u32,
//     pub(crate) info: WalletInfo,
//     pub(crate) seed: SeedInfo,
//     pub(crate) epochs: EpochsInfo,
//     pub(crate) purpose: DerivationPath,
//     pub(crate) accounts: Vec<Account>,
// }

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub enum SeedInfo {
//     Wip3(Seed),
// }

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct Seed(pub(crate) Vec<u8>);

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct EpochsInfo {
//     pub(crate) last: u32,
//     pub(crate) born: u32,
// }

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct DerivationPath(pub(crate) String);

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct Account {
//     key_path: KeyPath,
//     key_chains: Vec<KeyChain>,
//     balance: u64,
// }

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct KeyPath(Vec<ChildNumber>);

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub struct ChildNumber(u32);

// /// TODO: doc
// #[derive(Debug, Deserialize, Serialize)]
// pub enum KeyChain {
//     External,
//     Internal,
//     Rad,
// }

// pub type WalletInfos = Vec<WalletInfo>;

// /// Forward message. It will send a JsonRPC request with the given method string and params to the
// /// node.
// pub struct Forward(pub String, pub rpc::Params);
