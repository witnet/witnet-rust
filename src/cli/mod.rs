use std::{env, path::PathBuf};

use lazy_static::lazy_static;
use structopt::StructOpt;
use terminal_size as term;

use witnet_config as config;

mod node;
mod wallet;

pub fn from_args() -> Cli {
    Cli::from_args()
}

pub fn exec(command: Cli) -> Result<(), failure::Error> {
    match command {
        Cli {
            config,
            debug,
            trace,
            no_timestamp,
            no_module_path,
            cmd,
            ..
        } => {
            let mut log_opts = LogOptions::default();
            let config = get_config(config.or_else(config::dirs::find_config))?;

            log_opts.level = config.log.level;
            log_opts.source = LogOptionsSource::Config;
            log_opts.timestamp = !no_timestamp;
            log_opts.module_path = !no_module_path;

            if let Ok(rust_log) = env::var("RUST_LOG") {
                if rust_log.contains("witnet") {
                    log_opts.level = env_logger::Logger::from_default_env().filter();
                    log_opts.source = LogOptionsSource::Env;
                }
            }

            if trace {
                log_opts.level = log::LevelFilter::Trace;
                log_opts.source = LogOptionsSource::Flag;
            } else if debug {
                log_opts.level = log::LevelFilter::Debug;
                log_opts.source = LogOptionsSource::Flag;
            }

            init_logger(log_opts);
            witnet_data_structures::set_environment(config.environment);

            exec_cmd(cmd, config)
        }
    }
}

fn exec_cmd(command: Command, config: config::config::Config) -> Result<(), failure::Error> {
    match command {
        Command::Node(cmd) => node::exec_cmd(cmd, config),
        Command::Wallet(cmd) => wallet::exec_cmd(cmd, config),
    }
}

fn init_logger(opts: LogOptions) {
    println!(
        "Setting log level to: {}, source: {:?}",
        opts.level, opts.source
    );
    env_logger::Builder::from_env(env_logger::Env::default())
        .default_format_timestamp(opts.timestamp)
        .default_format_module_path(opts.module_path)
        .filter_level(log::LevelFilter::Info)
        .filter_module("witnet", opts.level)
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
    /// Do not show timestamps in logs.
    #[structopt(long = "no-timestamp")]
    no_timestamp: bool,
    /// Do not show module path in logs.
    #[structopt(long = "no-module-path")]
    no_module_path: bool,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    #[structopt(name = "node", about = "Witnet full node.")]
    Node(node::Command),
    #[structopt(name = "wallet", about = "Witnet wallet.")]
    Wallet(wallet::Command),
}

struct LogOptions {
    level: log::LevelFilter,
    timestamp: bool,
    module_path: bool,
    source: LogOptionsSource,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            level: log::LevelFilter::Error,
            timestamp: true,
            module_path: true,
            source: LogOptionsSource::Defaults,
        }
    }
}

#[derive(Debug)]
enum LogOptionsSource {
    Defaults,
    Config,
    Env,
    Flag,
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
