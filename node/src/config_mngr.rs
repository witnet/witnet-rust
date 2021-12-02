use crate::utils::stop_system_if_panicking;
use actix::{Actor, SystemService};
use std::default::Default;
use std::path::PathBuf;
use std::sync::Arc;
use witnet_config::{config::Config, loaders::toml};
use witnet_futures_utils::TryFutureExt2;

/// Start the configuration manager with an initial configuration
pub fn start(config: Arc<Config>) {
    let addr = ConfigManager::create(|_ctx| ConfigManager { config });
    actix::SystemRegistry::set(addr);
}

/// Start the configuration manager with the default configuration
pub fn start_default() {
    let addr = ConfigManager::start_default();
    actix::SystemRegistry::set(addr);
}

/// Get a reference to the current configuration stored in the manager
pub async fn get() -> Result<Arc<Config>, failure::Error> {
    let addr = ConfigManager::from_registry();
    addr.send(Get).flatten_err().await
}

/// Substitute configuration in the manager with the one loaded from the
/// given filename.
pub async fn load_from_file(filename: PathBuf) -> Result<(), failure::Error> {
    let addr = ConfigManager::from_registry();
    addr.send(Load(Source::File(filename))).flatten_err().await
}

/// Config manager: Actor that manages the application configuration
///
/// This actor is in charge of reading the configuration for the
/// application from a given source and using a given format, and
/// supports messages for giving access to the configuration it holds.
#[derive(Debug)]
struct ConfigManager {
    config: Arc<Config>,
}

impl Drop for ConfigManager {
    fn drop(&mut self) {
        log::trace!("Dropping ConfigManager");
        stop_system_if_panicking("ConfigManager");
    }
}

/// Message to obtain a reference to the configuration managed by the
/// `ConfigManager` actor.
struct Get;

/// Message to set the value of the configuration managed by the
/// `ConfigManager` actor.
struct Set(Config);

/// Message to load additional configuration from a source.
struct Load(Source);

/// Different kinds of configuration sources
#[derive(Debug)]
enum Source {
    /// The configuration is loaded from the given path
    File(PathBuf),
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self {
            config: Arc::new(Default::default()),
        }
    }
}

impl actix::Actor for ConfigManager {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("Config Manager actor has been started!");
    }
}

impl actix::Supervised for ConfigManager {}

impl actix::SystemService for ConfigManager {}

impl actix::Message for Get {
    type Result = Result<Arc<Config>, failure::Error>;
}

impl actix::Message for Set {
    type Result = Result<(), failure::Error>;
}

impl actix::Message for Load {
    type Result = Result<(), failure::Error>;
}

impl actix::Handler<Get> for ConfigManager {
    type Result = <Get as actix::Message>::Result;

    fn handle(&mut self, _msg: Get, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.config.clone())
    }
}

impl actix::Handler<Set> for ConfigManager {
    type Result = <Set as actix::Message>::Result;

    fn handle(&mut self, Set(config): Set, _ctx: &mut Self::Context) -> Self::Result {
        self.config = Arc::new(config);

        Ok(())
    }
}

impl actix::Handler<Load> for ConfigManager {
    type Result = <Load as actix::Message>::Result;

    fn handle(&mut self, Load(source): Load, _ctx: &mut Self::Context) -> Self::Result {
        self.load_config(&source)
            .map(|r| {
                log::info!("Loaded new configuration from source: {:?}", source);
                r
            })
            .map_err(|e| {
                log::error!(
                    "Failed to load new configuration from source: {:?}, error: {}",
                    source,
                    e
                );
                e
            })
    }
}

impl ConfigManager {
    fn load_config(&mut self, source: &Source) -> Result<(), failure::Error> {
        let new_config = match source {
            Source::File(filename) => Config::from_partial(&toml::from_file(filename)?),
        };

        self.config = Arc::new(new_config);

        Ok(())
    }
}
