use std::{env, path::PathBuf};

use lazy_static::lazy_static;
use structopt::StructOpt;
use terminal_size as term;

use env_logger::TimestampPrecision;
use witnet_config as config;

mod node;
mod wallet;

pub fn from_args() -> Cli {
    Cli::from_args()
}

// This clippy allow is needed because `init_logger` is conditionally compiled and returns unit
// in one of the implementations.
#[allow(clippy::let_unit_value)]
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
    let config_path = config.or_else(config::dirs::find_config);
    let config = get_config(&config_path)?;

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

    exec_cmd(cmd, config_path, config)
}

fn exec_cmd(
    command: Command,
    config_path: Option<PathBuf>,
    config: config::config::Config,
) -> Result<(), failure::Error> {
    match command {
        Command::Node(cmd) => node::exec_cmd(cmd, config_path, config),
        Command::Wallet(cmd) => wallet::exec_cmd(cmd, config),
    }
}

fn configure_logger(opts: &LogOptions) -> env_logger::Builder {
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
        .filter_module("witnet_node", opts.level)
        .filter_module("witnet_wallet", opts.level);

    logger_builder
}

/// Implementation of `init_logger` for non-debug environments with the `telemetry` feature being
/// enabled. Note that telemetry is ultimately enabled through configuration.
#[cfg(all(not(debug_assertions), feature = "telemetry"))]
fn init_logger(opts: LogOptions) -> Option<sentry::ClientInitGuard> {
    use std::str::FromStr;
    use std::sync::Arc;

    // Configure the logger builder
    let mut logger_builder = configure_logger(&opts);

    // Initialize Sentry (automated bug reporting) if explicitly enabled in configuration
    if opts.sentry_telemetry {
        // Configure Sentry DSN
        let dsn = sentry::types::Dsn::from_str(
            "https://def0c5d0fb354ef9ad6dddb576a21624@o394464.ingest.sentry.io/5244595",
        )
        .ok();
        // Acquire the crate name and version from the environment at compile time so Sentry can
        // report which release is being used
        let release = sentry::release_name!();
        // Initialize client. The guard binding needs to live as long as `main()` so as not to drop it
        // This enables panic capturing
        let guard = sentry::init(sentry::ClientOptions {
            dsn,
            release,
            before_send: Some(Arc::new(Box::new(filter_private_data))),
            ..Default::default()
        });
        // Logger integration for capturing errors. This actually intercepts errors but forwards all
        // log lines to the underlying logging backend, `env_logger` in this case.
        let logger = logger_builder.build();
        let max_level = logger.filter();
        let logger = sentry::integrations::log::SentryLogger::with_dest(logger);
        log::set_boxed_logger(Box::new(logger)).unwrap();
        log::set_max_level(max_level);

        log::info!("Sentry telemetry enabled");

        // Return client guard so it is not freed and it lives for as long as the application does
        Some(guard)
    } else {
        // If telemetry is not enabled, initialize logger directly
        logger_builder.init();

        None
    }
}

/// Implementation of `init_logger` for debug environments or any other environment missing the
/// `telemetry` feature. This conditional compile simply removes the need for `sentry` dependency
/// derived from having to keep the sentry guard alive for the lifetime of the entire app.
#[cfg(not(all(not(debug_assertions), feature = "telemetry")))]
fn init_logger(opts: LogOptions) {
    // Configure the logger builder
    let mut logger_builder = configure_logger(&opts);
    // If telemetry is not supported, initialize logger directly
    logger_builder.init();
}

fn get_config(path: &Option<PathBuf>) -> Result<config::config::Config, failure::Error> {
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

/// Prevents sending Sentry events containing private data
#[cfg(all(not(debug_assertions), feature = "telemetry"))]
fn filter_private_data(
    event: sentry::protocol::Event<'static>,
) -> Option<sentry::protocol::Event<'static>> {
    Some(event).filter(|event| {
        event.logger != Some(String::from("witnet_node::signature_mngr"))
            || event.breadcrumbs.values.iter().any(|breadcrumb| {
                breadcrumb
                    .message
                    .as_ref()
                    .filter(|message| !message.contains("xprv"))
                    .is_some()
            })
    })
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

#[test]
#[cfg(all(not(debug_assertions), feature = "telemetry"))]
fn test_filter_private_data() {
    let chain_manager_event = sentry::protocol::Event {
        logger: Some(String::from("witnet_node::chain_manager")),
        ..Default::default()
    };
    let signature_manager_ok_event = sentry::protocol::Event {
        logger: Some(String::from("witnet_node::signature_mngr")),
        breadcrumbs: sentry::protocol::Values::from(vec![sentry::protocol::Breadcrumb {
            message: Some(String::from("This is perfectly OK")),
            ..Default::default()
        }]),
        ..Default::default()
    };
    let signature_manager_filtered_event = sentry::protocol::Event {
        logger: Some(String::from("witnet_node::signature_mngr")),
        breadcrumbs: sentry::protocol::Values::from(vec![sentry::protocol::Breadcrumb {
            message: Some(String::from("This is an xprv encoded private key")),
            ..Default::default()
        }]),
        ..Default::default()
    };

    assert_eq!(
        filter_private_data(chain_manager_event.clone()),
        Some(chain_manager_event)
    );
    assert_eq!(
        filter_private_data(signature_manager_ok_event.clone()),
        Some(signature_manager_ok_event)
    );
    assert_eq!(
        filter_private_data(signature_manager_filtered_event.clone()),
        None
    );
}
