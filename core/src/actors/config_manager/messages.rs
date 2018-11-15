use actix::Message;

use std::io;

use witnet_config::config::Config;

/// Message to obtain the configuration managed by the `ConfigManager`
/// actor.
pub struct GetConfig;

/// Result of the GetConfig message handling
pub type ConfigResult = Result<Config, io::Error>;

impl Message for GetConfig {
    type Result = ConfigResult;
}
