use std::process;

#[macro_use]
extern crate clap;
use env_logger;

use witnet_config as config;
use witnet_wallet as wallet;

fn main() {
    env_logger::Builder::from_default_env()
        .default_format_timestamp(false)
        .default_format_module_path(false)
        .filter_level(log::LevelFilter::Info)
        .init();

    let app = app_definition();
    let matches = app.get_matches();
    let config_path = matches
        .value_of("config")
        .map(|path| path.into())
        .or_else(config::dirs::find_config);

    let mut conf = if let Some(path) = config_path {
        match config::loaders::toml::from_file(path) {
            Ok(partial_config) => config::config::Config::from_partial(&partial_config),
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    } else {
        println!("HEADS UP! No configuration specified/found. Using default one!");
        config::config::Config::default()
    };

    match matches.value_of("workers") {
        Some("auto") => conf.wallet.workers = None,
        Some(workers) => match workers.parse::<usize>() {
            Ok(value) => conf.wallet.workers = Some(value),
            Err(e) => {
                eprintln!("Invalid value for workers {}", e);
                process::exit(1);
            }
        },
        _ => {}
    }

    if let Some(db_path) = matches.value_of("db_path") {
        conf.wallet.db_path = db_path.into();
    }

    if let Some(addr) = matches.value_of("addr") {
        match addr.parse() {
            Ok(addr) => {
                conf.wallet.server_addr = addr;
            }
            Err(e) => {
                eprintln!("Invalid value for addr {}", e);
                process::exit(1);
            }
        }
    }

    if let Some(addr) = matches.value_of("node_addr") {
        match addr.parse() {
            Ok(addr) => {
                conf.wallet.node_addr = Some(addr);
            }
            Err(e) => {
                eprintln!("Invalid value for node-addr {}", e);
                process::exit(1);
            }
        }
    }

    match matches.subcommand() {
        ("run", _) => match wallet::run(conf) {
            Ok(_) => process::exit(0),
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        },
        ("show-config", _) => {
            println!(
                "[wallet]\n{}",
                config::loaders::toml::to_string(&conf.wallet)
                    .expect("Config serialization failed.")
            );
        }
        _ => {
            eprintln!("{}", matches.usage());
        }
    }
}

#[inline]
fn app_definition<'a, 'b>() -> clap::App<'a, 'b> {
    app_from_crate!()
        .about(r#"
This is the generic Witnet Wallet websockets server which you can run in the background
and have a client (GUI, TUI, etc) connect to it."#)
        .subcommand(
            clap::SubCommand::with_name("show-config")
                .about("Print the configuration params that will be used. Useful as a template.")
        )
        .subcommand(
            clap::SubCommand::with_name("run")
                .about("Run the Witnet Wallet server.")
        )
        .arg(
            clap::Arg::with_name("config")
                .long("config")
                .short("c")
                .global(true)
                .help(r#"Load configuration from this file. If not specified will try to find a configuration
in these paths:
- current path
- standard configuration path:
  - $XDG_CONFIG_HOME/witnet/witnet.toml in Gnu/Linux
  - $HOME/Library/Preferences/witnet/witnet.toml in MacOS
  - C:\Users\<YOUR USER>\AppData\Roaming\witnet\witnet.toml
- /etc/witnet/witnet.toml if in a *nix platform
If no configuration is found. The default configuration is used, see `config` subcommand if
you want to know more about the default config.
"#),
        )
        .arg(
            clap::Arg::with_name("workers")
                .long("workers")
                .takes_value(true)
                .global(true)
                .help(r#"How many worker threads the server will use.
Default 1.
Use value 'auto' to use as many as available cores."#),
        )
        .arg(
            clap::Arg::with_name("db_path")
                .long("db-path")
                .takes_value(true)
                .global(true)
                .help(r#"Path to the wallet database. If not specified will use:
- $XDG_DATA_HOME/witnet/wallet.db in Gnu/Linux
- $HOME/Libary/Application\ Support/witnet/wallet.db in MacOS
- {FOLDERID_LocalAppData}/witnet/wallet.db in Windows
If one of the above directories cannot be determined,
the current one will be used."#),
        )
        .arg(
            clap::Arg::with_name("addr")
                .long("addr")
                .takes_value(true)
                .global(true)
                .help(r#"Socket address of the Wallet websockets server.
Default: 127.0.0.1:11212"#),
        )
        .arg(
            clap::Arg::with_name("node_addr")
                .long("node-addr")
                .takes_value(true)
                .global(true)
                .help(r#"Socket address of the Witnet node to query.
By default the wallet does not communicate with a node."#),
        )
}
