use super::ConfigManager;
use actix::{Actor, Context};
use log::{debug, info};
use std::sync::Arc;
use witnet_config::{config::Config, loaders::toml};

impl Actor for ConfigManager {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("Config Manager actor has been started!");
        info!(
            "Reading configuration from file: {}",
            self.config_file.to_string_lossy()
        );
        self.config = Arc::new(Config::from_partial(
            &toml::from_file(&self.config_file).unwrap(),
        ))
    }
}
