use std::{net::SocketAddr, path::PathBuf, time::Duration};

use structopt::StructOpt;

use witnet_config::config::Config;
use witnet_node as node;

use super::json_rpc_client as rpc;

pub fn exec_cmd(command: Command, mut config: Config) -> Result<(), failure::Error> {
    match command {
        Command::GetBlock { node, hash } => {
            rpc::get_block(node.unwrap_or(config.jsonrpc.server_address), hash)
        }
        Command::GetTransaction { node, hash } => {
            rpc::get_transaction(node.unwrap_or(config.jsonrpc.server_address), hash)
        }
        Command::BlockChain { node, epoch, limit } => {
            rpc::get_blockchain(node.unwrap_or(config.jsonrpc.server_address), epoch, limit)
        }
        Command::GetBalance { node, pkh } => {
            let pkh = pkh.map(|x| x.parse()).transpose()?;
            rpc::get_balance(node.unwrap_or(config.jsonrpc.server_address), pkh)
        }
        Command::GetPkh { node } => rpc::get_pkh(node.unwrap_or(config.jsonrpc.server_address)),
        Command::GetUtxoInfo { node, long, pkh } => {
            let pkh = pkh.map(|x| x.parse()).transpose()?;
            rpc::get_utxo_info(node.unwrap_or(config.jsonrpc.server_address), long, pkh)
        }
        Command::GetReputation { node, pkh, all } => {
            let pkh = pkh.map(|x| x.parse()).transpose()?;
            rpc::get_reputation(node.unwrap_or(config.jsonrpc.server_address), pkh, all)
        }
        Command::Output { node, pointer } => {
            rpc::get_output(node.unwrap_or(config.jsonrpc.server_address), pointer)
        }
        Command::Send {
            node,
            pkh,
            value,
            fee,
            time_lock,
            dry_run,
        } => rpc::send_vtt(
            node.unwrap_or(config.jsonrpc.server_address),
            Some(pkh.parse()?),
            value,
            None,
            fee,
            time_lock.unwrap_or(0),
            None,
            dry_run,
        ),
        Command::Split {
            node,
            pkh,
            value,
            size,
            fee,
            time_lock,
            dry_run,
        } => {
            let pkh = pkh.map(|x| x.parse()).transpose()?;
            let size = if size == 0 { None } else { Some(size) };
            rpc::send_vtt(
                node.unwrap_or(config.jsonrpc.server_address),
                pkh,
                value,
                size,
                fee,
                time_lock.unwrap_or(0),
                Some(true),
                dry_run,
            )
        }
        Command::Join {
            node,
            pkh,
            value,
            size,
            fee,
            time_lock,
            dry_run,
        } => {
            let pkh = pkh.map(|x| x.parse()).transpose()?;
            let size = if size == Some(0) { None } else { size };
            rpc::send_vtt(
                node.unwrap_or(config.jsonrpc.server_address),
                pkh,
                value,
                size,
                fee,
                time_lock.unwrap_or(0),
                Some(false),
                dry_run,
            )
        }
        Command::SendRequest {
            node,
            hex,
            fee,
            run,
        } => rpc::send_dr(node.unwrap_or(config.jsonrpc.server_address), hex, fee, run),
        Command::Raw { node } => rpc::raw(node.unwrap_or(config.jsonrpc.server_address)),
        Command::ShowConfig => {
            // TODO: Implementation requires to make Config serializable
            Ok(())
        }
        Command::Run(params) => {
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

            if let Some(master_key_import_path) = params.master_key_import {
                config.storage.master_key_import_path = Some(master_key_import_path);
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
        Command::MasterKeyExport {
            node,
            write,
            write_to,
        } => {
            let write_to_path = match (write, write_to) {
                // Don't write
                (false, None) => None,
                // Write to default storage path
                (true, None) => Some(config.storage.db_path),
                // Write to custom path
                (_, Some(path)) => Some(path),
            };
            rpc::master_key_export(
                node.unwrap_or(config.jsonrpc.server_address),
                write_to_path.as_deref(),
            )
        }
        Command::DataRequestReport {
            node,
            dr_tx_hash,
            json,
            print_data_request,
        } => rpc::data_request_report(
            node.unwrap_or(config.jsonrpc.server_address),
            dr_tx_hash,
            json,
            print_data_request,
        ),
        Command::GetPeers { node } => rpc::get_peers(node.unwrap_or(config.jsonrpc.server_address)),
        Command::GetKnownPeers { node } => {
            rpc::get_known_peers(node.unwrap_or(config.jsonrpc.server_address))
        }
    }
}

#[derive(Debug, StructOpt)]
pub enum Command {
    #[structopt(name = "server", about = "Run a Witnet node server.", alias = "run")]
    Run(ConfigParams),
    #[structopt(
        name = "raw",
        about = "Send raw JSON-RPC requests, read from stdin one line at a time"
    )]
    Raw {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(name = "blockchain", about = "List block hashes")]
    BlockChain {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// First epoch for which to return block hashes.
        /// If negative, return block hashes from the last n epochs.
        #[structopt(long = "epoch", allow_hyphen_values = true, default_value = "0")]
        epoch: i64,
        /// Number of block hashes to return.
        /// If negative, return the last n block hashes from this epoch range.
        /// If zero, unlimited.
        #[structopt(long = "limit", allow_hyphen_values = true, default_value = "-50")]
        limit: i64,
    },
    #[structopt(
        name = "getBlock",
        alias = "block",
        about = "Find a block by its hash "
    )]
    GetBlock {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "hash", help = "SHA-256 block hash in hex format")]
        hash: String,
    },
    #[structopt(
        name = "getTransaction",
        alias = "transaction",
        about = "Find a transaction by its hash "
    )]
    GetTransaction {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "hash", help = "SHA-256 transaction hash in hex format")]
        hash: String,
    },
    #[structopt(name = "getBalance", about = "Get total balance of the given account")]
    GetBalance {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash for which to get balance. If omitted, defaults to the node pkh.
        #[structopt(long = "pkh")]
        pkh: Option<String>,
    },
    #[structopt(name = "getPkh", about = "Get the public key hash of the node")]
    GetPkh {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "getUtxoInfo",
        about = "Get the unspent transaction outputs of the node"
    )]
    GetUtxoInfo {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Show all the information about utxos
        #[structopt(long = "long")]
        long: bool,
        /// Public key hash for which to get UTXO information. If omitted, defaults to the node pkh.
        #[structopt(long = "pkh")]
        pkh: Option<String>,
    },
    #[structopt(
        name = "getReputation",
        about = "Get the reputation of the given account"
    )]
    GetReputation {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash for which to get reputation. If omitted, defaults to the node pkh.
        #[structopt(long = "pkh")]
        pkh: Option<String>,
        /// Print all the reputation?
        #[structopt(long = "all", conflicts_with = "pkh")]
        all: bool,
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
    #[structopt(name = "send", about = "Create a value transfer transaction")]
    Send {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash of the destination
        #[structopt(long = "pkh")]
        pkh: String,
        /// Value
        #[structopt(long = "value")]
        value: u64,
        /// Fee
        #[structopt(long = "fee")]
        fee: u64,
        /// Time lock
        #[structopt(long = "time-lock")]
        time_lock: Option<u64>,
        /// Print the request that would be sent to the node and exit without doing anything
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(
        name = "splitTransaction",
        about = "Create a value transfer transaction that split utxos"
    )]
    Split {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash of the destination. If omitted, defaults to the node pkh.
        #[structopt(long = "pkh")]
        pkh: Option<String>,
        /// Value
        #[structopt(long = "value")]
        value: u64,
        /// Utxo's size
        #[structopt(long = "size")]
        size: u64,
        /// Fee
        #[structopt(long = "fee")]
        fee: u64,
        /// Time lock
        #[structopt(long = "time-lock")]
        time_lock: Option<u64>,
        /// Print the request that would be sent to the node and exit without doing anything
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(
        name = "joinTransaction",
        about = "Create a value transfer transaction that join utxos"
    )]
    Join {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash of the destination. If omitted, defaults to the node pkh.
        #[structopt(long = "pkh")]
        pkh: Option<String>,
        /// Value
        #[structopt(long = "value")]
        value: u64,
        /// Utxo's size
        #[structopt(long = "size")]
        size: Option<u64>,
        /// Fee
        #[structopt(long = "fee")]
        fee: u64,
        /// Time lock
        #[structopt(long = "time-lock")]
        time_lock: Option<u64>,
        /// Print the request that would be sent to the node and exit without doing anything
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(name = "send-request", about = "Send a serialized data request")]
    SendRequest {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(long = "hex")]
        hex: String,
        #[structopt(long = "fee", default_value = "0")]
        fee: u64,
        /// Run the data request locally before sending, to ensure correctness of RADON scripts
        #[structopt(long = "run")]
        run: bool,
    },
    #[structopt(
        name = "show-config",
        about = "Dump the loaded config in Toml format to stdout."
    )]
    ShowConfig,
    #[structopt(name = "masterKeyExport", about = "Export the node master key.")]
    MasterKeyExport {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Write the private key to "storage_path/private_key_pkh.txt"
        #[structopt(long = "write")]
        write: bool,
        /// Change the path where to write storage_path/private_key_pkh.txt". Implies --write
        #[structopt(long = "write-to")]
        write_to: Option<PathBuf>,
    },
    #[structopt(
        name = "dataRequestReport",
        about = "Show information about a data request"
    )]
    DataRequestReport {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "dr-tx-hash", help = "Data request transaction hash")]
        dr_tx_hash: String,
        #[structopt(long = "json", help = "Show output in JSON format")]
        json: bool,
        #[structopt(long = "show-dr", help = "Print data request")]
        print_data_request: bool,
    },
    #[structopt(
        name = "peers",
        alias = "getPeers",
        about = "Get the list of peers connected to the node"
    )]
    GetPeers {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "knownPeers",
        alias = "getKnownPeers",
        about = "Get the list of peers known by the node"
    )]
    GetKnownPeers {
        /// Socket address of the Witnet node to query.
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
}

#[derive(Debug, StructOpt)]
pub struct ConfigParams {
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
    #[structopt(long = "db", help = NODE_DB_HELP)]
    db: Option<PathBuf>,
    /// Path to file that contains the master key to import
    #[structopt(long = "master-key-import")]
    master_key_import: Option<PathBuf>,
}

static NODE_DB_HELP: &str = r#"Path to the node database. If not specified will use '.witnet-rust-mainnet' for mainnet, or '.witnet-rust-testnet-N' for testnet number N."#;
