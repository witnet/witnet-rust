//! cli
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

use crate::core;
use ctrlc;
use failure;

use std::path::PathBuf;
use std::result::Result;
use structopt::StructOpt;

/// witnet cli
#[derive(Debug, StructOpt)]
pub(crate) struct Cli {
    /// `witnet cmd ...`
    #[structopt(subcommand)]
    pub(crate) cmd: Command,
}

#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
pub(crate) enum Command {
    #[structopt(name = "node", about = "Run the Witnet server")]
    Node {
        #[structopt(
            name = "address",
            short = "d",
            help = "TCP address to which the server should build"
        )]
        address: String,

        #[structopt(name = "peer", short = "p", help = "Address to peer connection")]
        peer: String,

        #[structopt(
            name = "background",
            short = "b",
            help = "Run the server in the background"
        )]
        background: bool,
    },
}

pub(crate) fn exec(command: Command) -> Result<(), failure::Error> {
    match Command {
        Command::Node {
            address,
            peer,
            background,
        } => {
            actors::node::run(address, || {
                // FIXME(#72): decide what to do when interrupt signals are received
                ctrlc::set_handler(move || {
                    actors::node::close();
                })
                .expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
            })
            .expect("Server error");
        }
    }
}
