use actix::{Actor, Context, Handler, Message, Supervised, SystemService};
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
}

impl Actor for ConfigManager {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("[Config Manager] Started!");
        self.config = toml::from_file("witnet.toml").unwrap();
    }
}

/// Required traits for being able to retrieve the actor address from
/// the registry.
impl Supervised for ConfigManager {}
impl SystemService for ConfigManager {}

/// Message to obtain the configuration managed by the `ConfigManager`
/// actor.
pub struct GetConfig;

impl Message for GetConfig {
    type Result = Result<Config, io::Error>;
}

impl Handler<GetConfig> for ConfigManager {
    type Result = Result<Config, io::Error>;

    fn handle(&mut self, _msg: GetConfig, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.config.clone())
    }
}
