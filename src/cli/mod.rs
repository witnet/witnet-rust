use std::{env, path::PathBuf};

use lazy_static::lazy_static;
use structopt::StructOpt;
use terminal_size as term;

use env_logger::TimestampPrecision;
use std::borrow::Cow;
use std::str::FromStr;
use witnet_config as config;

mod node;
mod wallet;

pub fn from_args() -> Cli {
    Cli::from_args()
}

pub fn exec(
    Cli {
        config,
        debug,
        trace,
        no_timestamp,
        no_module_path,
        cmd,
        ..
    }: Cli,
) -> Result<(), failure::Error> {
    let mut log_opts = LogOptions::default();
    let config = get_config(config.or_else(config::dirs::find_config))?;

    log_opts.level = config.log.level;
    log_opts.sentry_telemetry = config.log.sentry_telemetry;
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

    let _guard = init_logger(log_opts);
    witnet_data_structures::set_environment(config.environment);

    exec_cmd(cmd, config)
}

fn exec_cmd(command: Command, config: config::config::Config) -> Result<(), failure::Error> {
    match command {
        Command::Node(cmd) => node::exec_cmd(cmd, config),
        Command::Wallet(cmd) => wallet::exec_cmd(cmd, config),
    }
}

fn init_logger(opts: LogOptions) -> Option<sentry::internals::ClientInitGuard> {
    println!(
        "Setting log level to: {}, source: {:?}",
        opts.level, opts.source
    );

    let mut logger_builder = env_logger::Builder::from_env(env_logger::Env::default());
    logger_builder
        .format_timestamp(if opts.timestamp {
            Some(TimestampPrecision::Seconds)
        } else {
            None
        })
        .format_module_path(opts.module_path)
        .filter_level(log::LevelFilter::Info)
        .filter_module("witnet", opts.level)
        .filter_module("witnet_node", opts.level);

    // Initialize Sentry (automated bug reporting) if explicitly enabled in configuration
    if cfg!(not(debug_assertions)) && opts.sentry_telemetry {
        // Configure Sentry DSN
        let dsn = sentry::internals::Dsn::from_str(
            "https://def0c5d0fb354ef9ad6dddb576a21624@o394464.ingest.sentry.io/5244595",
        )
        .ok();
        // Acquire the crate name and version from the environment at compile time so Sentry can
        // report which release is being used
        let release = option_env!("CARGO_PKG_NAME")
            .and_then(|name| {
                option_env!("CARGO_PKG_VERSION").map(|version| format!("{}@{}", name, version))
            })
            .map(Cow::from);
        // Initialize client. The guard binding needs to live as long as `main()` so as not to drop it
        let guard = sentry::init(sentry::ClientOptions {
            dsn,
            release,
            ..Default::default()
        });
        // Logger integration for capturing errors. This actually intercepts errors but forwards all
        // log lines to the underlying logging backend, `env_logger` in this case.
        sentry::integrations::env_logger::init(Some(logger_builder.build()), Default::default());
        // Panic capturing
        sentry::integrations::panic::register_panic_handler();

        // Return client guard so it is not freed and it lives for as long as the application does
        Some(guard)
    } else {
        // If telemetry is not enabled, initialize logger directly
        logger_builder.init();

        None
    }
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
#[structopt(max_term_width = *TERM_WIDTH)]
pub struct Cli {
    #[structopt(short = "c", long = "config", help = CONFIG_HELP)]
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
    sentry_telemetry: bool,
    source: LogOptionsSource,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            level: log::LevelFilter::Error,
            timestamp: true,
            module_path: true,
            sentry_telemetry: false,
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

static CONFIG_HELP: &str = r#"Load configuration from this file. If not specified will try to find a configuration
in these paths:
- current path
- standard configuration path:
  - $XDG_CONFIG_HOME/witnet/witnet.toml in Gnu/Linux
  - $HOME/Library/Preferences/witnet/witnet.toml in MacOS
  - C:\Users\<YOUR USER>\AppData\Roaming\witnet\witnet.toml
- /etc/witnet/witnet.toml if in a *nix platform
If no configuration is found. The default configuration is used, see `config` subcommand if
you want to know more about the default config."#;
