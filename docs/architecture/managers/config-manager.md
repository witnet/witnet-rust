# Config Manager

The __config manager__ is the actor in charge of managing the configuration required by the system. Its main responsibilities are the following:

- Load configuration from a file (if specified when creating the actor) and merge it with the default configuration
- Store configuration parameters on its state
- Provide a deep-copy of the configuration to other actors

## State

The state of the `Config Manager` is defined as library code ['Config'][config], which contains the actual definition of the Config struct, along with the different loaders (`witnet_config::loaders`).

```rust
#[derive(Debug, Default)]
pub struct ConfigManager {
    /// Loaded configuration
    config: Config,
    /// Configuration file from which to read the configuration when
    /// the actor starts, if `None` the default configuration is used
    filename: Option<String>,
}
```

## Actor creation and registration

The creation of the config manager actor and its registration into the system registry are performed directly by the `main` process as follows:

```rust
const CONFIG_DEFAULT_FILENAME: &str = "witnet.toml";

let config_manager_addr = ConfigManager::new(CONFIG_DEFAULT_FILENAME).start();
System::current().registry().set(config_manager_addr);
```

In case of no configuration file, a `default` instantiation may be used. All configuration parameters will be set to their default values.

```rust
let config_manager_addr = ConfigManager::default().start();
System::current().registry().set(config_manager_addr);
```

## API

### Incoming messages: Others -> Config Manager

These are the messages supported by the config manager actor:

| Message   | Input type | Output type    | Description                              |
| --------- | ---------- | -------------- | -----------------------------------      |
| GetConfig | `()`       | `ConfigResult` | Request a deep-copy of the configuration |

Where `ConfigResult` is just:

``` rust
/// Result of the GetConfig message handling
pub type ConfigResult = Result<Config, io::Error>;
```

The way other actors will communicate with the config manager is:

1. Get the address of the config manager from the registry:

    ```rust
    // Get config manager address
    let config_manager_addr = System::current().registry().get::<ConfigManager>();
    ```

2. Use any of the sending methods provided by the address (`do_send()`, `try_send()`, `send()`) to send a message to the actor:

    ```rust
    config_manager_addr
        .send(GetConfig)
        .into_actor(self)
        .then(|res, _act, _ctx| {
            // Process the response from config manager
            process_get_config_response(res)
        })
        .and_then(|config, _act, ctx| {
            // Do something with the config
            actix::fut::ok(())
        })
        .wait(ctx);
    ```

### Outgoing messages: Config manager -> Others

The config manager is a simple wrapper over the config library and it does not need to start a communication with other actors in order to perform its functions.

## Further information

The full source code of the `Config` can be found at [`config_manager.rs`][config_manager].

[config_manager]: https://github.com/witnet/witnet-rust/blob/master/core/src/actors/config_manager
[config]: https://github.com/witnet/witnet-rust/blob/master/config/
