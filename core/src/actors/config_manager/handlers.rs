use actix::{Context, Handler};

use super::{
    messages::{ConfigResult, GetConfig},
    ConfigManager,
};

impl Handler<GetConfig> for ConfigManager {
    type Result = ConfigResult;

    fn handle(&mut self, _msg: GetConfig, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.config.clone())
    }
}
