use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use structopt::StructOpt;

use witnet_config::config::Config;
use witnet_data_structures::{chain::Epoch, fee::Fee};
use witnet_node as node;

use super::json_rpc_client as rpc;

pub fn exec_cmd(
    command: Command,
    config_path: Option<PathBuf>,
    mut config: Config,
) -> Result<(), failure::Error> {
    match command {
        Command::Claim {
            node,
            identifier,
            write,
            write_to,
        } => {
            let write_to_path = match (write, write_to) {
                // Don't write
                (false, None) => None,
                // Write to the same folder where the config file is located
                // Fail if using default configuration (not sourced from a file in the filesystem)
                (true, None) => config_path
                    .expect("Cannot guess a file system path for writing the signed claiming data. Please specify a valid path right after the `--write` flag.")
                    .parent()
                    .map(Path::to_path_buf),
                // Write to custom path
                (_, Some(path)) => Some(path),
            };
            rpc::claim(
                node.unwrap_or(config.jsonrpc.server_address),
                identifier,
                write_to_path.as_deref(),
            )
        }
        Command::GetBlock { node, hash } => {
            rpc::get_block(node.unwrap_or(config.jsonrpc.server_address), hash)
        }
        Command::GetTransaction { node, hash } => {
            rpc::get_transaction(node.unwrap_or(config.jsonrpc.server_address), hash)
        }
        Command::BlockChain { node, epoch, limit } => {
            rpc::get_blockchain(node.unwrap_or(config.jsonrpc.server_address), epoch, limit)
        }
        Command::GetBalance {
            node,
            address,
            simple,
        } => {
            let address = address.map(|x| x.parse()).transpose()?;
            rpc::get_balance(
                node.unwrap_or(config.jsonrpc.server_address),
                address,
                simple,
            )
        }
        Command::GetSupplyInfo { node } => {
            rpc::get_supply_info(node.unwrap_or(config.jsonrpc.server_address))
        }
        Command::GetAddress { node } => rpc::get_pkh(node.unwrap_or(config.jsonrpc.server_address)),
        Command::GetUtxoInfo { node, long, pkh } => {
            let pkh = pkh.map(|x| x.parse()).transpose()?;
            rpc::get_utxo_info(node.unwrap_or(config.jsonrpc.server_address), long, pkh)
        }
        Command::GetReputation { node, address, all } => {
            let address = address.map(|x| x.parse()).transpose()?;
            rpc::get_reputation(node.unwrap_or(config.jsonrpc.server_address), address, all)
        }
        Command::GetMiners {
            node,
            start,
            end,
            csv,
        } => rpc::get_miners(
            node.unwrap_or(config.jsonrpc.server_address),
            start,
            end,
            csv,
        ),
        Command::Output { node, pointer } => {
            rpc::get_output(node.unwrap_or(config.jsonrpc.server_address), pointer)
        }
        Command::Send {
            node,
            address,
            value,
            fee,
            time_lock,
            dry_run,
        } => rpc::send_vtt(
            node.unwrap_or(config.jsonrpc.server_address),
            Some(address.parse()?),
            value,
            None,
            fee.map(Fee::absolute_from_nanowits),
            time_lock.unwrap_or(0),
            None,
            dry_run,
        ),
        Command::Split {
            node,
            address,
            value,
            size,
            fee,
            time_lock,
            dry_run,
        } => {
            let address = address.map(|x| x.parse()).transpose()?;
            let size = if size == 0 { None } else { Some(size) };
            rpc::send_vtt(
                node.unwrap_or(config.jsonrpc.server_address),
                address,
                value,
                size,
                fee.map(Fee::absolute_from_nanowits),
                time_lock.unwrap_or(0),
                Some(true),
                dry_run,
            )
        }
        Command::Join {
            node,
            address,
            value,
            size,
            fee,
            time_lock,
            dry_run,
        } => {
            let address = address.map(|x| x.parse()).transpose()?;
            let size = if size == Some(0) { None } else { size };
            rpc::send_vtt(
                node.unwrap_or(config.jsonrpc.server_address),
                address,
                value,
                size,
                fee.map(Fee::absolute_from_nanowits),
                time_lock.unwrap_or(0),
                Some(false),
                dry_run,
            )
        }
        Command::SendRequest {
            node,
            hex,
            fee,
            dry_run,
        } => rpc::send_dr(
            node.unwrap_or(config.jsonrpc.server_address),
            hex,
            fee.map(Fee::absolute_from_nanowits),
            dry_run,
        ),
        Command::Raw { node } => rpc::raw(node.unwrap_or(config.jsonrpc.server_address)),
        Command::ShowConfig => {
            let serialized = toml::to_string(&config.to_partial()).unwrap();
            println!("\n# Config");
            println!("{}", serialized);
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

            // Collect required node operations from parameters
            let mut ops = node::actors::node::NodeOps::new();
            if let Some(path) = params.snapshot_import {
                let path = if params.force {
                    node::utils::Force::Forced(path)
                } else {
                    node::utils::Force::Some(path)
                };
                ops.add(node::actors::node::NodeOp::SnapshotImport(path));
            }

            node::actors::node::run(Arc::new(config), ops, || {
                let system = node::actors::node::System::current();
                ctrlc::set_handler(move || {
                    node::actors::node::close(&system);
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
                // Write to the same folder where the config file is located
                // Fail if using default configuration (not sourced from a file in the filesystem)
                (true, None) => config_path
                    .expect("Cannot guess a file system path for writing the master key file. Please specify a valid path right after the `--write` flag.")
                    .parent()
                    .map(Path::to_path_buf),
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
            create_local_tally,
        } => rpc::data_request_report(
            node.unwrap_or(config.jsonrpc.server_address),
            dr_tx_hash,
            json,
            print_data_request,
            create_local_tally,
        ),
        Command::SearchRequests {
            node,
            start,
            end,
            hex_dr_bytes,
            same_as_dr_tx,
        } => rpc::search_requests(
            node.unwrap_or(config.jsonrpc.server_address),
            start,
            end,
            hex_dr_bytes,
            same_as_dr_tx,
        ),
        Command::GetPeers { node } => rpc::get_peers(node.unwrap_or(config.jsonrpc.server_address)),
        Command::GetKnownPeers { node } => {
            rpc::get_known_peers(node.unwrap_or(config.jsonrpc.server_address))
        }
        Command::GetNodeStats { node } => {
            rpc::get_node_stats(node.unwrap_or(config.jsonrpc.server_address))
        }
        Command::AddPeers { node, peers } => {
            rpc::add_peers(node.unwrap_or(config.jsonrpc.server_address), peers)
        }
        Command::ClearPeers { node } => {
            rpc::clear_peers(node.unwrap_or(config.jsonrpc.server_address))
        }
        Command::InitializePeers { node } => {
            rpc::initialize_peers(node.unwrap_or(config.jsonrpc.server_address))
        }
        Command::Rewind { node, epoch } => {
            rpc::rewind(node.unwrap_or(config.jsonrpc.server_address), epoch)
        }
        Command::SignalingInfo { node } => {
            rpc::signaling_info(node.unwrap_or(config.jsonrpc.server_address))
        }
        Command::Priority { node, json } => {
            rpc::priority(node.unwrap_or(config.jsonrpc.server_address), json)
        }
    }
}

#[derive(Debug, StructOpt)]
pub enum Command {
    #[structopt(name = "server", about = "Run a Witnet node server", alias = "run")]
    Run(ConfigParams),
    #[structopt(
        name = "raw",
        about = "Send raw JSON-RPC requests, read from stdin one line at a time"
    )]
    Raw {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "blockchain",
        alias = "getBlockChain",
        about = "List block hashes"
    )]
    BlockChain {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// First epoch for which to return block hashes
        /// If negative, return block hashes from the last n epochs
        #[structopt(long = "epoch", allow_hyphen_values = true, default_value = "0")]
        epoch: i64,
        /// Number of block hashes to return.
        /// If negative, return the last n block hashes from this epoch range.
        /// If zero, unlimited
        #[structopt(long = "limit", allow_hyphen_values = true, default_value = "-50")]
        limit: i64,
    },
    #[structopt(
        name = "claim",
        about = "Claim a Witnet identity by signing the identifier with the node master key"
    )]
    Claim {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Identifier to be claimed by the node (e.g. Witnet ID)
        #[structopt(short = "i", long = "identifier")]
        identifier: String,
        /// Write the signed claimed data to "storage_path/claim-<id>-<public_key>.txt"
        #[structopt(long = "write")]
        write: bool,
        /// Change the path to write storage_path/claim-<id>-<public_key>.txt". Implies --write
        #[structopt(long = "write-to")]
        write_to: Option<PathBuf>,
    },
    #[structopt(
        name = "minerList",
        alias = "getMiners",
        about = "List block hashes with their miners and the total number of mined block by each address"
    )]
    GetMiners {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// First epoch for which to return block hashes
        /// If negative, return block hashes from the last n epochs
        #[structopt(
            long = "start",
            alias = "s",
            allow_hyphen_values = true,
            default_value = "0"
        )]
        start: i64,
        /// If negative, return the last n block hashes from this epoch range.
        /// If zero, unlimited
        #[structopt(
            long = "end",
            alias = "e",
            allow_hyphen_values = true,
            default_value = "4294967294"
        )]
        end: i64,
        /// Use csv format
        #[structopt(long = "csv")]
        csv: bool,
    },
    #[structopt(
        name = "block",
        alias = "getBlock",
        about = "Find a block by its hash "
    )]
    GetBlock {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "hash", help = "SHA-256 block hash in hex format")]
        hash: String,
    },
    #[structopt(
        name = "transaction",
        alias = "getTransaction",
        about = "Find a transaction by its hash "
    )]
    GetTransaction {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "hash", help = "SHA-256 transaction hash in hex format")]
        hash: String,
    },
    #[structopt(
        name = "balance",
        alias = "getBalance",
        about = "Get the balance of the own or supplied account",
        long_about = "Get either the simple balance or the confirmed and pending balance of the own or supplied account:\n\
                      \tBalance: the total balance of this node without distinguishing between fully confirmed and pending balance.\n\
                      \tConfirmed balance: balance that has been confirmed in a superblock by a majority of the network.\n\
                      \tPending balance: balance that is waiting to be confirmed, a negative amount corresponds to sent transactions."
    )]
    GetBalance {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Address for which to get balance. If omitted, defaults to the node address
        #[structopt(long = "address", alias = "pkh")]
        address: Option<String>,
        /// Fetch and print only the simple balance
        #[structopt(long = "simple")]
        simple: bool,
    },
    #[structopt(
        name = "supply",
        alias = "getSupply",
        alias = "getSupplyInfo",
        about = "Get the total supply of witnet tokens"
    )]
    GetSupplyInfo {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "address",
        alias = "getAddress",
        alias = "pkh",
        alias = "getPkh",
        about = "Get the public address of the node"
    )]
    GetAddress {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "utxos",
        alias = "coins",
        alias = "utxoInfo",
        alias = "getUtxoInfo",
        about = "Get the unspent transaction outputs of the node"
    )]
    GetUtxoInfo {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Show all the information about utxos
        #[structopt(long = "long")]
        long: bool,
        /// Public key hash for which to get UTXO information. If omitted, defaults to the node pkh
        #[structopt(long = "address", alias = "pkh")]
        pkh: Option<String>,
    },
    #[structopt(
        name = "reputation",
        alias = "getReputation",
        about = "Get the reputation of the given account"
    )]
    GetReputation {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Address for which to get reputation. If omitted, defaults to the node address
        #[structopt(long = "address", alias = "pkh")]
        address: Option<String>,
        /// Print all the reputation?
        #[structopt(long = "all", conflicts_with = "address")]
        all: bool,
    },
    #[structopt(name = "output", about = "Find an output of a transaction")]
    Output {
        /// Socket address of the Witnet node to query
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
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Address of the destination
        #[structopt(long = "address", alias = "pkh")]
        address: String,
        /// Value
        #[structopt(long = "value")]
        value: u64,
        /// Fee
        #[structopt(long = "fee")]
        fee: Option<u64>,
        /// Time lock
        #[structopt(long = "time-lock")]
        time_lock: Option<u64>,
        /// Print the request that would be sent to the node and exit without doing anything
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(
        name = "splitTransaction",
        about = "Create a value transfer transaction that splits UTXOs"
    )]
    Split {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash of the destination. If omitted, defaults to the node pkh
        #[structopt(long = "address", alias = "pkh")]
        address: Option<String>,
        /// Value
        #[structopt(long = "value")]
        value: u64,
        /// Utxo's size
        #[structopt(long = "size")]
        size: u64,
        /// Fee
        #[structopt(long = "fee")]
        fee: Option<u64>,
        /// Time lock
        #[structopt(long = "time-lock")]
        time_lock: Option<u64>,
        /// Print the request that would be sent to the node and exit without doing anything
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(
        name = "joinTransaction",
        about = "Create a value transfer transaction that joins UTXOs"
    )]
    Join {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// Public key hash of the destination. If omitted, defaults to the node pkh
        #[structopt(long = "address", alias = "pkh")]
        address: Option<String>,
        /// Value
        #[structopt(long = "value")]
        value: u64,
        /// Utxo's size
        #[structopt(long = "size")]
        size: Option<u64>,
        /// Fee
        #[structopt(long = "fee")]
        fee: Option<u64>,
        /// Time lock
        #[structopt(long = "time-lock")]
        time_lock: Option<u64>,
        /// Print the request that would be sent to the node and exit without doing anything
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(
        name = "sendRequest",
        alias = "send-request",
        about = "Send a serialized data request"
    )]
    SendRequest {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(long = "hex")]
        hex: String,
        #[structopt(long = "fee")]
        fee: Option<u64>,
        /// Run the data request locally to ensure correctness of RADON scripts
        /// It will returns a RadonTypes with the Tally result
        #[structopt(long = "dry-run")]
        dry_run: bool,
    },
    #[structopt(
        name = "config",
        alias = "show-config",
        alias = "showConfig",
        about = "Dump the loaded config in Toml format to stdout"
    )]
    ShowConfig,
    #[structopt(name = "masterKeyExport", about = "Export the node master key")]
    MasterKeyExport {
        /// Socket address of the Witnet node to query
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
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(name = "dr-tx-hash", help = "Data request transaction hash")]
        dr_tx_hash: String,
        #[structopt(long = "json", help = "Show output in JSON format")]
        json: bool,
        #[structopt(long = "show-dr", help = "Print data request")]
        print_data_request: bool,
        #[structopt(long = "run-tally", help = "Re-run tally stage locally")]
        create_local_tally: bool,
    },
    #[structopt(
        name = "searchRequests",
        about = "Search data requests with a specific bytecode"
    )]
    SearchRequests {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// First epoch for which to search for requests
        /// If negative, search the last n epochs
        #[structopt(
            long = "start",
            alias = "s",
            allow_hyphen_values = true,
            default_value = "0"
        )]
        start: i64,
        /// If negative, search the last n blocks from this epoch range.
        /// If zero, unlimited
        #[structopt(
            long = "end",
            alias = "e",
            allow_hyphen_values = true,
            default_value = "4294967294"
        )]
        end: i64,
        #[structopt(
            long = "hex-dr-bytes",
            value_name = "bytecode",
            help = "Data request bytecode in hexadecimal format"
        )]
        hex_dr_bytes: Option<String>,
        #[structopt(
            long = "same-as-dr-tx",
            value_name = "data request transaction hash",
            help = "Search all the data requests that have the exact same bytecode as this one"
        )]
        same_as_dr_tx: Option<String>,
    },
    #[structopt(
        name = "peers",
        alias = "getPeers",
        about = "Get the list of peers connected to the node"
    )]
    GetPeers {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "knownPeers",
        alias = "getKnownPeers",
        about = "Get the list of peers known by the node"
    )]
    GetKnownPeers {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "nodeStats",
        alias = "getNodeStats",
        about = "Get the node stats"
    )]
    GetNodeStats {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "addPeers",
        about = "Add new peer addresses for the node to try to connect to"
    )]
    AddPeers {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// List of peer addresses for the node to try to connect to.
        ///
        /// Expected format: list of "address:port" separated by spaces:
        ///
        /// addPeers 52.166.178.145:21337 52.166.178.145:22337
        ///
        /// If no addresses are provided, read the addresses from stdin.
        #[structopt(name = "peers")]
        peers: Vec<SocketAddr>,
    },
    #[structopt(name = "clearPeers", about = "Clear all peers from the buckets")]
    ClearPeers {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "initializePeers",
        about = "Clear all peers from the buckets and initialize to those in config"
    )]
    InitializePeers {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(name = "rewind", about = "Rewind blockchain to this epoch")]
    Rewind {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        /// The epoch of the top block of the chain after the rewind has completed.
        #[structopt(short = "e", long = "epoch")]
        epoch: Epoch,
    },
    #[structopt(
        name = "signalingInfo",
        about = "Get Information related to TAPI signaling"
    )]
    SignalingInfo {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
    },
    #[structopt(
        name = "priority",
        aliases = &["getPriority", "estimatePriority", "fee", "getFee", "estimateFee", "fees", "getFees", "estimateFees"],
        about = "Estimate priority values and their time-to-block for multiple priority tiers."
    )]
    Priority {
        /// Socket address of the Witnet node to query
        #[structopt(short = "n", long = "node")]
        node: Option<SocketAddr>,
        #[structopt(long = "json", help = "Show output in JSON format")]
        json: bool,
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
    /// Max number of connections to other peers this node (as a client) maintains
    #[structopt(long = "out-limit")]
    outbound_limit: Option<u16>,
    /// Period of the bootstrap peers task (in seconds)
    #[structopt(long = "peers-period")]
    bootstrap_peers_period_seconds: Option<u64>,
    #[structopt(long = "db", help = NODE_DB_HELP)]
    db: Option<PathBuf>,
    /// Path to file that contains the master key to import
    #[structopt(long = "master-key-import")]
    master_key_import: Option<PathBuf>,
    /// Path to a file that contains a chain state snapshot
    #[structopt(long = "snapshot-import")]
    snapshot_import: Option<PathBuf>,
    /// Indicate whether other operations must be executed regardless of pre-checks.
    #[structopt(long, short)]
    force: bool,
}

static NODE_DB_HELP: &str = r#"Path to the node database. If not specified will use '.witnet-rust-mainnet' for mainnet, or '.witnet-rust-testnet-N' for testnet number N."#;
