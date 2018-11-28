use actix::Message;

use std::io;

use std::sync::Arc;
use witnet_config::config::Config;

/// Message to obtain the configuration managed by the `ConfigManager`
/// actor.
pub struct GetConfig;

/// Result of the GetConfig message handling
pub type ConfigResult = Result<Arc<Config>, io::Error>;

impl Message for GetConfig {
    type Result = ConfigResult;
}
