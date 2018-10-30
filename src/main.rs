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

use crate::core::actors::node;

mod cli;

fn main() {
    // Init app logger
    env_logger::init();

    // Read configuration
    let configuration = config::Config::default();

    // Build default address from configuration
    let default_address = configuration.connections.server_addr;
    let default_address_cli = &format!("{}", default_address);

    // Get db root
    let default_db_root = configuration.storage.db_path;

    // TODO
    let matches = app_from_crate!()
        .subcommand(cli::node::get_arg(default_address_cli))
        .get_matches();

    // Check run mode
    match matches.subcommand() {
        // node run mode
        ("node", Some(arg_matches)) => {
            // Peer address to be used with incoming features
            let _peer_address = arg_matches.value_of("peer").unwrap_or("");

            // Call function to run system actor
            node::run(default_address, &default_db_root.to_string_lossy(), || {
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
