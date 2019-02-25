use actix::{Context, Handler};

use super::ConfigManager;
use crate::actors::messages::{ConfigResult, GetConfig};

impl Handler<GetConfig> for ConfigManager {
    type Result = ConfigResult;

    fn handle(&mut self, _msg: GetConfig, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.config.clone())
    }
}
