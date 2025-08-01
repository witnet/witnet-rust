//! Witnet <> Ethereum bridge
#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

use async_jsonrpc_client::{Transport, transports::tcp::TcpSocket};
use futures_util::compat::Compat01As03;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use web3::{
    Web3,
    contract::Contract,
    transports::Http,
    types::{H160, TransactionReceipt},
};

/// Actors
pub mod actors;
/// Configuration
pub mod config;

/// Creates a Witnet Request Board contract from Config information
pub fn create_wrb_contract(
    eth_jsonrpc_url: &str,
    eth_witnet_oracle: H160,
) -> (Web3<Http>, Contract<Http>) {
    let web3_http = web3::transports::Http::new(eth_jsonrpc_url)
        .map_err(|e| format!("Failed to connect to Ethereum client.\nError: {e:?}"))
        .unwrap();
    let web3 = web3::Web3::new(web3_http);
    // Why read files at runtime when you can read files at compile time
    let wrb_contract_abi_json: &[u8] = include_bytes!("../../wrb_abi.json");
    let mut wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
        .map_err(|e| format!("Unable to load WRB contract from ABI: {e:?}"))
        .unwrap();

    // Fix issue #2046, manually select the desired function when multiple candidates have the same name
    // https://github.com/witnet/witnet-rust/issues/2046
    hack_fix_functions_with_multiple_definitions(&mut wrb_contract_abi);

    let wrb_contract = Contract::new(web3.eth(), eth_witnet_oracle, wrb_contract_abi);

    (web3, wrb_contract)
}

// The web3 library does not properly support overloaded functions yet, so here we ensure that there
// is no ambiguity and every function has only one possible definition
fn hack_fix_functions_with_multiple_definitions(wrb_contract_abi: &mut web3::ethabi::Contract) {
    let functions = wrb_contract_abi
        .functions
        .get_mut("reportResult")
        .expect("no reportResult function in ABI");
    // There are two candidate "reportResult" functions, we want to keep the one with 4 inputs
    assert_eq!(functions.len(), 2);
    functions.retain(|f| f.inputs.len() == 4);
    assert_eq!(functions.len(), 1);

    // Ensure that all functions only have one possible definition
    for (function_name, definitions) in &wrb_contract_abi.functions {
        assert_eq!(
            definitions.len(),
            1,
            "function {function_name:?} is duplicated in ABI"
        );
    }
}

/// Check if the witnet node is running
pub async fn check_witnet_node_running(witnet_addr: &str) -> Result<(), String> {
    let (_handle, witnet_client) = TcpSocket::new(witnet_addr).unwrap();
    let witnet_client = Arc::new(witnet_client);
    let res = witnet_client.execute("syncStatus", json!(null));
    let res = Compat01As03::new(res);
    // 5 second timeout
    let res = tokio::time::timeout(Duration::from_secs(5), res).await;

    match res {
        Ok(Ok(x)) => {
            log::debug!("Witnet node is running at {witnet_addr}: {x:?}");

            Ok(())
        }
        Ok(Err(e)) => {
            log::warn!("Witnet node is running at {witnet_addr} but not synced: {e:?}");

            Ok(())
        }
        Err(_elapsed) => {
            // elapsed.to_string() returns "deadline has elapsed" which is hard to understand
            let e = "timeout";
            log::error!("Failed to connect to witnet node at {witnet_addr} error: {e}");

            Err(e.to_string())
        }
    }
}

/// Check if the ethereum node is running
pub async fn check_ethereum_node_running(eth_jsonrpc_url: &str) -> Result<(), String> {
    let web3_http = web3::transports::Http::new(eth_jsonrpc_url)
        .map_err(|e| format!("Failed to connect to Ethereum client.\nError: {e:?}"))
        .unwrap();
    let web3 = web3::Web3::new(web3_http);

    // Use a sample web3 call to check http connection
    let res = web3.eth().syncing().await;
    match res {
        Ok(syncing) => {
            log::debug!("Ethereum node is running at {eth_jsonrpc_url}");
            match syncing {
                web3::types::SyncState::NotSyncing => {}
                web3::types::SyncState::Syncing(sync_info) => {
                    log::warn!("Ethereum provider is syncing: {sync_info:?}");
                }
            }

            Ok(())
        }
        Err(e) => {
            match e {
                web3::Error::Decoder(error_msg)
                    if error_msg.contains("expected object or `false`, got `true`") =>
                {
                    // Ignore this error because it can be caused by a non-standard ethereum provider
                    // https://github.com/witnet/witnet-rust/issues/2141
                    log::debug!("Ethereum node is running at {eth_jsonrpc_url}");
                    log::warn!("Ethereum provider returned `true` on eth_syncing method");

                    Ok(())
                }
                _ => {
                    log::error!("Failed to connect to ethereum node: {e}");

                    Err(e.to_string())
                }
            }
        }
    }
}

/// Handle Ethereum transaction receipt
// This function is async because in the future it may be possible
// to retrieve the failure reason (for example: transaction reverted, invalid
// opcode).
pub async fn handle_receipt(receipt: &TransactionReceipt) -> Result<(), ()> {
    match receipt.status {
        Some(x) if x == 1.into() => {
            // Success
            Ok(())
        }
        Some(x) if x == 0.into() => {
            // Fail
            Err(())
        }
        x => {
            log::error!("Unknown return code, should be 0 or 1, is: {x:?}");
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hack_fix_functions_with_multiple_definitions() {
        // The hack_fix_functions_with_multiple_definitions function already does some checks
        // internally, so here we call it to ensure the ABI is correct.
        let wrb_contract_abi_json: &[u8] = include_bytes!("../../wrb_abi.json");
        let mut wrb_contract_abi = web3::ethabi::Contract::load(wrb_contract_abi_json)
            .map_err(|e| format!("Unable to load WRB contract from ABI: {e:?}"))
            .unwrap();
        hack_fix_functions_with_multiple_definitions(&mut wrb_contract_abi);
    }
}
