//! cli
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]
use std::env;
use std::path::PathBuf;
use std::result::Result;

use ctrlc;
use directories;
use failure;
use structopt::{clap::AppSettings, StructOpt};

use super::json_rpc_client;
use crate::node::actors;

/// Witnet network
#[derive(Debug, StructOpt)]
#[structopt(raw(global_settings = "&[AppSettings::AllowNegativeNumbers]"))]
pub(crate) struct Cli {
    /// `witnet cmd ...`
    #[structopt(subcommand)]
    pub(crate) cmd: Command,
}

#[derive(Debug, StructOpt)]
pub(crate) enum Command {
    #[structopt(name = "node", about = "Run the Witnet server")]
    Node {
        // TCP address to which the server should build
        // #[structopt(name = "address", short = "d")]
        // address: String,

        // Address to peer connection
        // #[structopt(name = "peer", short = "p")]
        // peer: String,

        // Config file path
        #[structopt(
            name = "config",
            long = "config",
            short = "c",
            help = "Path to the configuration file"
        )]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,
        // Run the server in the background
        // #[structopt(name = "background", short = "b")]
        // background: bool,
    },
    #[structopt(name = "cli", about = "Run JSON-RPC requests")]
    Cli {
        // Config file path
        #[structopt(
            name = "config",
            long = "config",
            short = "c",
            help = "Path to the configuration file"
        )]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,

        #[structopt(subcommand)]
        cmd: CliCommand,
    },
}

#[derive(Debug, StructOpt)]
pub(crate) enum CliCommand {
    #[structopt(
        name = "raw",
        about = "Send raw JSON-RPC requests, read from stdin one line at a time"
    )]
    Raw {
        // Config file path
        #[structopt(
            name = "config",
            long = "config",
            short = "c",
            help = "Path to the configuration file"
        )]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,
    },
    #[structopt(name = "getBlockChain", about = "Get blockchain hashes")]
    GetBlockChain {
        // Config file path
        #[structopt(
            name = "config",
            long = "config",
            short = "c",
            help = "Path to the configuration file"
        )]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,
        // Positional argument 1: first epoch for which to show block hashes
        epoch: Option<i64>,
        // Positional argument 2: max number of epochs for which to show block hashes
        limit: Option<u32>,
    },
    #[structopt(name = "getBlock", about = "Get a block by its hash")]
    GetBlock {
        // Config file path
        #[structopt(
            name = "config",
            long = "config",
            short = "c",
            help = "Path to the configuration file"
        )]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,
        #[structopt(name = "hash", help = "SHA256 block hash in string format")]
        hash: String,
    },
    #[structopt(name = "getOutput", about = "Get an output of a transaction")]
    GetOutput {
        // Config file path
        #[structopt(
            name = "config",
            long = "config",
            short = "c",
            help = "Path to the configuration file"
        )]
        #[structopt(parse(from_os_str))]
        config: Option<PathBuf>,
        #[structopt(
            name = "outputPointer",
            help = "Output pointer in the format <transaction_id>:<output_index>"
        )]
        output_index: String,
    },
}

pub(crate) fn exec(command: Command) -> Result<(), failure::Error> {
    match command {
        Command::Node { config } => {
            let fallback_config = find_config_file();
            actors::node::run(config, fallback_config, || {
                // FIXME(#72): decide what to do when interrupt signals are received
                ctrlc::set_handler(move || {
                    actors::node::close();
                })
                .expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
            })?;
        }
        Command::Cli { config, cmd } => {
            json_rpc_client::run(config, cmd)?;
        }
    }
    Ok(())
}

/// Tries to find a config file in the current working directory or in
/// the platform-specific user-accessible config location
/// (e.g.: `~/.config/witnet/witnet.toml` in Linux)
fn find_config_file() -> Option<PathBuf> {
    let mut config_dirs = Vec::with_capacity(2);

    if let Ok(cwd) = env::current_dir() {
        config_dirs.push(cwd);
    }

    if let Some(dirs) = directories::ProjectDirs::from("io", "witnet", "witnet") {
        config_dirs.push(dirs.config_dir().into());
    }

    for path in config_dirs {
        let config_path = path.join("witnet.toml");
        if config_path.exists() {
            return Some(config_path);
        }
    }

    None
}
