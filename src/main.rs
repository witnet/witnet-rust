#[macro_use]
extern crate log;
extern crate env_logger;

#[macro_use]
extern crate clap;
extern crate ctrlc;

extern crate serde_json;

extern crate witnet_config as config;
extern crate witnet_core as core;
extern crate witnet_crypto as crypto;
extern crate witnet_data_structures as data_structures;
extern crate witnet_p2p as p2p;
extern crate witnet_storage as storage;

use std::process::exit;

mod cli;
mod server;

fn main() {
    env_logger::init();

    let configuration: config::Config = config::read_config().unwrap();
    let default_address = &format!("{}:{}", configuration.server.host, configuration.server.port);

    let matches = app_from_crate!()
        .subcommand(cli::server::get_arg(default_address))
        .get_matches();

    match matches.subcommand() {
        ("server", Some(arg_matches)) => {
            let address = arg_matches
                .value_of("address")
                .unwrap_or(default_address);
            // peer address to be used with incoming features
            let _peer_address = arg_matches
                .value_of("peer")
                .unwrap_or("");

            server::run(address, || {
                ctrlc::set_handler(move || {
                    server::close();
                }).expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
            }).expect("Server error");
        }
        _ => {
            println!("Unrecognized command. Run with '--help' to learn more.");
            exit(1);
        }
    }
}
