use std::net::SocketAddr;

use structopt::StructOpt;

use witnet_config::{config::Config, loaders};
use witnet_wallet as wallet;

pub fn exec_cmd(command: Command, mut config: Config) -> Result<(), failure::Error> {
    match command {
        Command::Run(params) => {
            if let Some(node) = params.node {
                config.wallet.node_url = Some(node);
            }
            if let Some(db) = params.db {
                config.wallet.db_path = db;
            }
            if let Some(millis) = params.timeout {
                config.wallet.requests_timeout = millis;
            }
            let _result = wallet::run(config)?;
            Ok(())
        }
        Command::ShowConfig => {
            println!(
                "[wallet]\n{}",
                loaders::toml::to_string(&config.wallet).expect("Config serialization failed.")
            );
            Ok(())
        }
    }
}

#[derive(Debug, StructOpt)]
pub enum Command {
    #[structopt(
        name = "server",
        about = "Run a wallet server exposing a websockets API.",
        alias = "run"
    )]
    Run(ConfigParams),
    #[structopt(
        name = "show-config",
        about = "Dump the loaded config in Toml format to stdout."
    )]
    ShowConfig,
}

#[derive(Debug, StructOpt)]
pub struct ConfigParams {
    /// Socket address for the wallet server
    #[structopt(short = "l", long = "listen", default_value = "127.0.0.1:11212")]
    addr: SocketAddr,
    /// Socket address of the Witnet node to query.
    #[structopt(short = "n", long = "node")]
    node: Option<String>,
    #[structopt(long = "db", raw(help = "WALLET_DB_HELP"))]
    db: Option<std::path::PathBuf>,
    /// Milliseconds after outgoing requests should time out.
    #[structopt(long = "timeout")]
    timeout: Option<u64>,
}

static WALLET_DB_HELP: &str = r#"Path to the wallet database. If not specified will use:
- $XDG_DATA_HOME/witnet/wallet.db in Gnu/Linux
- $HOME/Libary/Application\ Support/witnet/wallet.db in MacOS
- {FOLDERID_LocalAppData}/witnet/wallet.db in Windows
If one of the above directories cannot be determined,
the current one will be used."#;
