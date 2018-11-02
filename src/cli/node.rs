//! cli server

#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

use clap::{App, Arg, SubCommand};
use witnet_core::actors::config_manager::CONFIG_DEFAULT_FILENAME;

pub fn get_arg(address: &str) -> App<'_, '_> {
    SubCommand::with_name("node")
        .about("Run the Witnet server")
        .arg(
            Arg::with_name("address")
                .short("d")
                .long("address")
                .help("TCP address to which the server should bind")
                .takes_value(true)
                .default_value(address),
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Filename with config info")
                .takes_value(true)
                .default_value(CONFIG_DEFAULT_FILENAME),
        )
}
