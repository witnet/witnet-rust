use actix::{
    fut::FutureResult, Actor, Context, Handler, MailboxError, Message, Supervised, SystemService,
};
use log::debug;
use std::io;
use witnet_config::loaders::toml;
use witnet_config::Config;

/// Config manager actor: manages the application configuration
///
/// This actor is in charge of reading the configuration for the
/// application from a given source and using a given format, and
/// supports messages for giving access to the configuration it holds.
#[derive(Debug, Default)]
pub struct ConfigManager {
    config: Config,
    filename: Option<String>,
}

impl Actor for ConfigManager {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("[Config Manager] Started!");
        match &self.filename {
            Some(filename) => self.config = toml::from_file(&filename).unwrap(),
            None => (),
        }
    }
}

impl ConfigManager {
    /// Create a new ConfigManager instance that will try to read the
    /// given configuration file name.
    pub fn new(filename: &str) -> Self {
        ConfigManager {
            config: Config::default(),
            filename: Some(filename.to_owned()),
        }
    }
}

/// Required traits for being able to retrieve the actor address from
/// the registry.
impl Supervised for ConfigManager {}
impl SystemService for ConfigManager {}

/// Message to obtain the configuration managed by the `ConfigManager`
/// actor.
pub struct GetConfig;

/// Result of the GetConfig message handling
pub type ConfigResult = Result<Config, io::Error>;

impl Message for GetConfig {
    type Result = ConfigResult;
}

impl Handler<GetConfig> for ConfigManager {
    type Result = ConfigResult;

    fn handle(&mut self, _msg: GetConfig, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.config.clone())
    }
}

/// Method to process ConfigManager GetConfig response
pub fn process_get_config_response<T>(
    response: Result<ConfigResult, MailboxError>,
) -> FutureResult<Config, (), T> {
    //) -> FutureResult<Config, (), dyn Actor<Context=T>> where T: ActorContext {
    response
        // Process the Result<ConfigResult, MailboxError>
        .map_or_else(
            |e| {
                debug!("Unsuccessful communication with config manager: {}", e);
                actix::fut::err(())
            },
            |res| {
                // Process the ConfigResult
                res.map_or_else(
                    |e| {
                        debug!("Error while getting config: {}", e);
                        actix::fut::err(())
                    },
                    actix::fut::ok,
                )
            },
        )
}
