use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use lazy_static::lazy_static;
use structopt::StructOpt;
use terminal_size as term;

use super::json_rpc_client as rpc;
use witnet_config as config;
use witnet_node as node;
use witnet_wallet as wallet;

pub fn from_args() -> Cli {
    Cli::from_args()
}

pub fn exec(command: Cli) -> Result<(), failure::Error> {
    match command {
        Cli {
            config,
            debug,
            trace,
            cmd,
            ..
        } => {
            let config = get_config(config.or_else(config::dirs::find_config))?;
            init_logger(debug, trace);

            exec_cmd(cmd, config)
        }
    }
}

fn exec_node_cmd(
    command: NodeCommand,
    mut config: config::config::Config,
) -> Result<(), failure::Error> {
    match command {
        NodeCommand::Block { node, hash } => {
            rpc::get_block(node.unwrap_or_else(|| config.connections.server_addr), hash)
        }
        NodeCommand::BlockChain { node, epoch, limit } => rpc::get_blockchain(
            node.unwrap_or_else(|| config.connections.server_addr),
            epoch,
            limit,
        ),
        NodeCommand::Output { node, pointer } => rpc::get_output(
            node.unwrap_or_else(|| config.connections.server_addr),
            pointer,
        ),
        NodeCommand::Raw { node } => {
            rpc::raw(node.unwrap_or_else(|| config.connections.server_addr))
        }
        NodeCommand::ShowConfig => {
            // TODO: Implementation requires to make Config serializable
            Ok(())
        }
        NodeCommand::Run(params) => {
            if let Some(addr) = params.addr {
                config.connections.server_addr = addr;
            }

            if let Some(limit) = params.outbound_limit {
                config.connections.outbound_limit = limit;
            }

            if let Some(period) = params.bootstrap_peers_period_seconds {
                config.connections.bootstrap_peers_period = Duration::from_secs(period);
            }

            if let Some(db) = params.db {
                config.storage.db_path = db;
            }

            config.connections.known_peers.extend(params.known_peers);

            node::actors::node::run(config, || {
                // FIXME(#72): decide what to do when interrupt signals are received
                ctrlc::set_handler(move || {
                    node::actors::node::close();
                })
                .expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
            })
        }
    }
}

fn exec_wallet_cmd(
    command: WalletCommand,
    mut config: config::config::Config,
) -> Result<(), failure::Error> {
    match command {
        WalletCommand::Run(params) => {
            if let Some(node) = params.node {
                config.wallet.node_url = Some(node);
            }
            if let Some(db) = params.db {
                config.wallet.db_path = db;
            }
            let _result = wallet::run(config);
            Ok(())
        }
        WalletCommand::ShowConfig => {
            println!(
                "[wallet]\n{}",
                config::loaders::toml::to_string(&config.wallet)
                    .expect("Config serialization failed.")
            );
            Ok(())
        }
    }
}

fn exec_cmd(command: Command, config: config::config::Config) -> Result<(), failure::Error> {
    match command {
        Command::Node(cmd) => exec_node_cmd(cmd, config),
        Command::Wallet(cmd) => exec_wallet_cmd(cmd, config),
    }
}

fn init_logger(debug: bool, trace: bool) {
    let log_level = if trace {
        log::LevelFilter::Trace
    } else if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("witnet"))
        .default_format_timestamp(false)
        .default_format_module_path(false)
        .filter_level(log::LevelFilter::Info)
        .filter_module("witnet", log_level)
        .init();
}

fn get_config(path: Option<PathBuf>) -> Result<config::config::Config, failure::Error> {
    match path {
        Some(p) => {
            println!("Loading config from: {}", p.display());
            let config = config::loaders::toml::from_file(p)
                .map(|p| config::config::Config::from_partial(&p))?;
            Ok(config)
        }
        None => {
            println!("HEADS UP! No configuration specified/found. Using default one!");
            Ok(config::config::Config::default())
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(raw(max_term_width = "*TERM_WIDTH"))]
pub struct Cli {
    #[structopt(short = "c", long = "config", raw(help = "CONFIG_HELP"))]
    config: Option<PathBuf>,
    /// Turn on DEBUG logging.
    #[structopt(long = "debug")]
    debug: bool,
    /// Turn on TRACE logging.
    #[structopt(long = "trace")]
    trace: bool,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    #[structopt(name = "node", about = "Witnet full node.")]
    Node(NodeCommand),
    #[structopt(name = "wallet", about = "Witnet wallet.")]
    Wallet(WalletCommand),
}

#[derive(Debug, StructOpt)]
enum NodeCommand {
    #[structopt(name = "run", about = "Run the Witnet server.")]
    Run(NodeConfigParams),
    #[structopt(
        name = "raw",
        about = "Send raw JSON-RPC requests, read from stdin one line at a time"
    )]
    Raw {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(name = "blockchain", about = "Find blockchain hashes ")]
    BlockChain {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// First epoch from which to show block hashes.
        #[structopt(long = "epoch", default_value = "0")]
        epoch: u32,
        /// Max number of epochs for which to show block hashes.
        #[structopt(long = "limit", default_value = "100")]
        limit: u32,
    },
    #[structopt(name = "block", about = "Find a block by its hash ")]
    Block {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "hash", help = "SHA-256 block hash in hex format")]
        hash: String,
    },
    #[structopt(name = "output", about = "Find an output of a transaction ")]
    Output {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(
            name = "pointer",
            help = "Output pointer of the transaction, that is: <transaction id>:<output index>"
        )]
        pointer: String,
    },
    #[structopt(
        name = "show-config",
        about = "Dump the loaded config in Toml format to stdout."
    )]
    ShowConfig,
}

#[derive(Debug, StructOpt)]
enum WalletCommand {
    #[structopt(name = "run", about = "Run the wallet websockets server.")]
    Run(WalletConfigParams),
    #[structopt(
        name = "show-config",
        about = "Dump the loaded config in Toml format to stdout."
    )]
    ShowConfig,
}

#[derive(Debug, StructOpt)]
struct NodeConfigParams {
    /// Socket address for the node server
    #[structopt(short = "l", long = "listen")]
    addr: Option<SocketAddr>,
    /// Initially known peers for the node.
    #[structopt(long = "peer")]
    known_peers: Vec<SocketAddr>,
    /// Max number of connections to other peers this node (as a client) maintains.
    #[structopt(long = "out-limit")]
    outbound_limit: Option<u16>,
    /// Period of the bootstrap peers task (in seconds).
    #[structopt(long = "peers-period")]
    bootstrap_peers_period_seconds: Option<u64>,
    #[structopt(long = "db", raw(help = "NODE_DB_HELP"))]
    db: Option<std::path::PathBuf>,
}

#[derive(Debug, StructOpt)]
struct WalletConfigParams {
    /// Socket address for the wallet server
    #[structopt(short = "l", long = "listen", default_value = "127.0.0.1:11212")]
    addr: SocketAddr,
    /// Socket address of the Witnet node to query.
    #[structopt(short = "n", long = "node")]
    node: Option<String>,
    #[structopt(long = "db", raw(help = "WALLET_DB_HELP"))]
    db: Option<std::path::PathBuf>,
}

lazy_static! {
    static ref TERM_WIDTH: usize = {
        let size = term::terminal_size();
        if let Some((term::Width(w), _)) = size {
            w as usize
        } else {
            120
        }
    };
}

static CONFIG_HELP: &str =
    r#"Load configuration from this file. If not specified will try to find a configuration
in these paths:
- current path
- standard configuration path:
  - $XDG_CONFIG_HOME/witnet/witnet.toml in Gnu/Linux
  - $HOME/Library/Preferences/witnet/witnet.toml in MacOS
  - C:\Users\<YOUR USER>\AppData\Roaming\witnet\witnet.toml
- /etc/witnet/witnet.toml if in a *nix platform
If no configuration is found. The default configuration is used, see `config` subcommand if
you want to know more about the default config."#;

static WALLET_DB_HELP: &str = r#"Path to the wallet database. If not specified will use:
- $XDG_DATA_HOME/witnet/wallet.db in Gnu/Linux
- $HOME/Libary/Application\ Support/witnet/wallet.db in MacOS
- {FOLDERID_LocalAppData}/witnet/wallet.db in Windows
If one of the above directories cannot be determined,
the current one will be used."#;

static NODE_DB_HELP: &str = r#"Path to the node database. If not specified will use '.witnet-rust-mainnet' for mainnet, or '.witnet-rust-testnet-N' for testnet number N."#;
