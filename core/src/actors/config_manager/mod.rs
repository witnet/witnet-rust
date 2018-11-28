use actix::{
    fut::FutureResult, Actor, ActorFuture, AsyncContext, ContextFutureSpawner, MailboxError,
    Supervised, System, SystemService, WrapFuture,
};

use log::error;
use std::path::PathBuf;
use std::sync::Arc;
use witnet_config::config::Config;

// Internal Actor implementation for ConfigManager
mod actor;

/// Handlers to manage ConfigManager messages
mod handlers;

/// Messages for ConfigManager
pub mod messages;

/// Default configuration filename
pub const CONFIG_DEFAULT_FILENAME: &str = "witnet.toml";

/// Config manager actor: manages the application configuration
///
/// This actor is in charge of reading the configuration for the
/// application from a given source and using a given format, and
/// supports messages for giving access to the configuration it holds.
#[derive(Debug)]
pub struct ConfigManager {
    /// Loaded configuration
    config: Arc<Config>,

    /// Configuration file from which to read the configuration when
    /// the actor starts
    config_file: PathBuf,
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self {
            config: Arc::new(Config::default()),
            config_file: PathBuf::from(CONFIG_DEFAULT_FILENAME),
        }
    }
}

impl ConfigManager {
    /// Create a new ConfigManager instance that will try to read the
    /// given configuration file name.
    pub fn new(config_file: Option<PathBuf>) -> Self {
        Self {
            config: Arc::new(Config::default()),
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
        .send(messages::GetConfig)
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
    response: Result<messages::ConfigResult, MailboxError>,
) -> FutureResult<Arc<Config>, (), T> {
    // Process the Result<ConfigResult, MailboxError>
    match response {
        Err(e) => {
            error!("Unsuccessful communication with config manager: {}", e);
            actix::fut::err(())
        }
        Ok(res) => {
            // Process the ConfigResult
            match res {
                Err(e) => {
                    error!("Error while getting config: {}", e);
                    actix::fut::err(())
                }
                Ok(res) => actix::fut::ok(res),
            }
        }
    }
}
