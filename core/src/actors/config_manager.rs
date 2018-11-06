use actix::{
    fut::FutureResult, Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler,
    MailboxError, Message, Supervised, System, SystemService, WrapFuture,
};
use log::debug;
use std::io;
use std::path::PathBuf;
use witnet_config::loaders::toml;
use witnet_config::Config;

/// Default configuration filename
pub const CONFIG_DEFAULT_FILENAME: &str = "witnet.toml";

/// Config manager actor: manages the application configuration
///
/// This actor is in charge of reading the configuration for the
/// application from a given source and using a given format, and
/// supports messages for giving access to the configuration it holds.
#[derive(Debug)]
pub struct ConfigManager {
    config: Config,
    config_file: PathBuf,
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self {
            config: Config::default(),
            config_file: PathBuf::from(CONFIG_DEFAULT_FILENAME),
        }
    }
}

impl Actor for ConfigManager {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("[Config Manager] Started!");
        self.config = toml::from_file(self.config_file.as_path()).unwrap();
    }
}

impl ConfigManager {
    /// Create a new ConfigManager instance that will try to read the
    /// given configuration file name.
    pub fn new(config_file: Option<PathBuf>) -> Self {
        Self {
            config: Config::default(),
            config_file: match config_file {
                Some(path) => path,
                None => PathBuf::from(CONFIG_DEFAULT_FILENAME),
            },
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

/// Method to send a GetConfig message to the ConfigManager
pub fn send_get_config_request<T, U: 'static>(act: &mut T, ctx: &mut T::Context, process_config: U)
where
    T: Actor,
    T::Context: AsyncContext<T>,
    U: FnOnce(&mut T, &mut T::Context, &Config),
{
    // Get config manager address
    let config_manager_addr = System::current().registry().get::<ConfigManager>();

    // Start chain of actions to send a message to the config manager
    config_manager_addr
        // Send GetConfig message to config manager actor
        // This returns a Request Future, representing an asynchronous message sending process
        .send(GetConfig)
        // Convert a normal future into an ActorFuture
        .into_actor(act)
        // Process the response from the config manager
        // This returns a FutureResult containing the socket address if present
        .then(|res, _act, _ctx| {
            // Process the response from config manager
            process_get_config_response(res)
        })
        // Process the received config
        // This returns a FutureResult containing a success
        .and_then(|config, act, ctx| {
            // Call function to process configuration
            process_config(act, ctx, &config);

            actix::fut::ok(())
        })
        .wait(ctx);
}

/// Method to process ConfigManager GetConfig response
pub fn process_get_config_response<T>(
    response: Result<ConfigResult, MailboxError>,
) -> FutureResult<Config, (), T> {
    // Process the Result<ConfigResult, MailboxError>
    match response {
        Err(e) => {
            debug!("Unsuccessful communication with config manager: {}", e);
            actix::fut::err(())
        }
        Ok(res) => {
            // Process the ConfigResult
            match res {
                Err(e) => {
                    debug!("Error while getting config: {}", e);
                    actix::fut::err(())
                }
                Ok(res) => actix::fut::ok(res),
            }
        }
    }
}
