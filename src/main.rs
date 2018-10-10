#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]

use std::process::exit;

use env_logger;

use clap::*;
use ctrlc;

use witnet_config as config;
use witnet_core as core;

use crate::config::{P2P_SERVER_PORT, P2P_SERVER_HOST, P2pConfig};
use crate::core::actors::node;

mod cli;

fn main() {
    // Init app logger
    env_logger::init();

    // Read configuration
    let configuration: config::Config = config::read_config().unwrap();

    // Build default address
    let default_address = &format!(
        "{}:{}",
        configuration
            .server
            .p2p.clone()
            .unwrap_or(P2pConfig {
                host: P2P_SERVER_HOST.to_string(),
                port: P2P_SERVER_PORT,
            })
            .host,
        configuration
            .server
            .p2p.clone()
            .unwrap_or(P2pConfig {
                host: P2P_SERVER_HOST.to_string(),
                port: P2P_SERVER_PORT,
            })
            .port
    );

    // TODO
    let matches = app_from_crate!()
        .subcommand(cli::node::get_arg(default_address))
        .get_matches();

    // Check run mode
    match matches.subcommand() {
        // node run mode
        ("node", Some(arg_matches)) => {

            // Get p2p host and port as command line arguments or from config file
            let address = arg_matches
                .value_of("address")
                .unwrap_or(default_address)
                .parse()
                .unwrap_or(default_address.parse().unwrap());

            // Peer address to be used with incoming features
            let _peer_address = arg_matches.value_of("peer").unwrap_or("");

            // Call function to run system actor
            node::run(address, || {
                ctrlc::set_handler(move || {
                    node::close();
                })
                .expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
            })
            .expect("Server error");
        }

        // unknown run mode
        _ => {
            println!("Unrecognized command. Run with '--help' to learn more.");
            exit(1);
        }
    }
}
