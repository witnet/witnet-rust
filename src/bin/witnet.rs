//This file is part of Rust-Witnet.
//
//Rust-Witnet is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
//Rust-Witnet is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
//You should have received a copy of the GNU General Public License
// along with Rust-Witnet. If not, see <http://www.gnu.org/licenses/>.
//
//This file is based on src/bin/grin.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Main for building the binary of a Witnet peer-to-peer node

extern crate clap;
#[macro_use]
extern crate slog;

extern crate witnet_config as config;
extern crate witnet_core as core;
extern crate witnet_util as util;
extern crate witnet_wit as wit;

mod client;

use std::thread;
use std::time::Duration;

use clap::{App, Arg, ArgMatches, SubCommand};

use config::GlobalConfig;
use core::global;
use util::{init_logger, LoggingConfig, LOGGER};

fn start_from_config_file(mut global_config: GlobalConfig) {
    info!(
        LOGGER,
        "Starting the Witnet server from configuration file at {}",
        global_config.config_file_path.unwrap().to_str().unwrap()
    );

    global::set_mining_mode(
        global_config
            .members
            .as_mut()
            .unwrap()
            .server
            .clone()
            .chain_type,
    );

    wit::Server::start(global_config.members.as_mut().unwrap().server.clone()).unwrap();
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn main() {
    // First, load a global config object, then modify that object with any
    // switches found so that the switches override the global config file

    // This will return a global config object, which will either contain defaults
    // for all of the config structures or a configuration read from a config
    // file

    let mut global_config =
        GlobalConfig::new(None).unwrap_or_else(|e| panic!("Error parsing config file: {}", e));

    if global_config.using_config_file {
        // Initialize the logger
        init_logger(global_config.members.as_mut().unwrap().logging.clone());
        info!(
            LOGGER,
            "Using configuration file at: {}",
            global_config
                .config_file_path
                .clone()
                .unwrap()
                .to_str()
                .unwrap()
        );
    } else {
        init_logger(Some(LoggingConfig::default()));
    }

    let args = App::new("Witnet")
        .version("0.1")
        .author("The Witnet Community")
        .about("Rust implementation of the Witnet Protocol.")
        .subcommand(
            SubCommand::with_name("server")
                .about("Control the Witnet server")
                .arg(
                    Arg::with_name("port")
                        .short("p")
                        .long("port")
                        .help("Port to start the P2P server on")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("seed")
                        .short("s")
                        .long("seed")
                        .help("Override seed node(s) to connect to")
                        .takes_value(true),
                )
                .subcommand(
                    SubCommand::with_name("start").about("Start the Witnet server as a daemon"),
                )
                .subcommand(SubCommand::with_name("stop").about("Stop the Witnet server daemon"))
                .subcommand(
                    SubCommand::with_name("run").about("Run the Witnet server in this console"),
                ),
        )
        .get_matches();

    match args.subcommand() {
        // Server commands and options
        ("server", Some(server_args)) => {
            server_command(server_args, global_config);
        }

        // If nothing is specified, try to just use the config file instead
        // this could possibly become the way to configure most things
        // with most command line options being phased out
        _ => {
            if global_config.using_config_file {
                start_from_config_file(global_config);
            } else {
                // Won't attempt to start with defaults, just reject instead
                println!("Unknown command, and no configuration file was found.");
                println!("Use 'witnet help' for a list of all commands.")
            }
        }
    }

    fn server_command(_server_args: &ArgMatches, _global_config: GlobalConfig) {
        info!(LOGGER, "Starting the Witnet server...")
    }
}
