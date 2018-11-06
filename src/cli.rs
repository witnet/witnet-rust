//! cli
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

use crate::core::actors;
use ctrlc;
use failure;

use std::path::PathBuf;
use std::result::Result;
use structopt::StructOpt;

/// Witnet network
#[derive(Debug, StructOpt)]
pub(crate) struct Cli {
    /// `witnet cmd ...`
    #[structopt(subcommand)]
    pub(crate) cmd: Command,
}

#[derive(Debug, StructOpt)]
pub(crate) enum Command {
    #[structopt(name = "node", about = "Run the Witnet server")]
    Node {
        /// TCP address to which the server should build
        #[structopt(name = "address", short = "d")]
        address: String,

        /// Address to peer connection
        #[structopt(name = "peer", short = "p")]
        peer: String,

        /// Config file path
        #[structopt(name = "config", short = "c")]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,

        /// Run the server in the background
        #[structopt(name = "background", short = "b")]
        background: bool,
    },
}

pub(crate) fn exec(command: Command) -> Result<(), failure::Error> {
    match command {
        Command::Node { config, .. } => {
            actors::node::run(config, || {
                // FIXME(#72): decide what to do when interrupt signals are received
                ctrlc::set_handler(move || {
                    actors::node::close();
                })
                .expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
            })?;
        }
    }
    Ok(())
}
