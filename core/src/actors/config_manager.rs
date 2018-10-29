use actix::{Actor, Context, Handler, Message, Supervised, SystemService};
use log::{debug, info};
use std::result;
use witnet_config::loaders::toml;
use witnet_config::Config;

/// Config manager actor: manages the application configuration
///
/// This actor is in charge of reading the configuration for the
/// application from a given source and using a given format, and
/// supports messages for giving access to the configuration it holds.
#[derive(Default, Debug)]
pub struct ConfigManager {
    config: Config,
}

impl Actor for ConfigManager {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("[Config Manager] I was started!");
    }
}

/// Required traits for being able to retrieve the actor address from
/// the registry.
impl Supervised for ConfigManager {}
impl SystemService for ConfigManager {}

/// Enum of possible errors that the actor might return in results.
#[derive(Debug)]
pub enum Error {
    /// An error caused when loading or parsing the configuration.
    LoadConfigError(toml::Error),
}

/// Just like `std::result::Result` but withe error param fixed to
/// `Error` type in this module.
pub type Result<T> = result::Result<T, Error>;

/// Message to tell the `ConfigManager` actor that it should load the
/// configuration from the specified file.
pub struct LoadConfig {
    /// File name (including path) of the configuration file.
    pub filename: String,
}

impl Message for LoadConfig {
    type Result = Result<Config>;
}

impl Handler<LoadConfig> for ConfigManager {
    type Result = Result<Config>;

    fn handle(&mut self, msg: LoadConfig, _ctx: &mut Context<Self>) -> Self::Result {
        let filename = &msg.filename;
        info!("[Config Manager] Loading config from: {}", filename);
        match toml::from_file(filename) {
            Ok(config) => {
                self.config = config;
                Ok(self.config.clone())
            }
            Err(e) => Err(Error::LoadConfigError(e)),
        }
    }
}

/// Message to obtain the configuration managed by the `ConfigManager`
/// actor.
pub struct GetConfig;

impl Message for GetConfig {
    type Result = Result<Config>;
}

impl Handler<GetConfig> for ConfigManager {
    type Result = Result<Config>;

    fn handle(&mut self, _msg: GetConfig, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.config.clone())
    }
}
