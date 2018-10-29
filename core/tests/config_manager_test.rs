extern crate actix;
extern crate futures;
extern crate witnet_config;
extern crate witnet_core;

use actix::*;
use futures::{future, Future};
use witnet_config::Config;
use witnet_core::actors::config_manager::*;

#[test]
fn test_config_manager_default() {
    let sys = System::new("test");
    let addr = ConfigManager::default().start();
    let res = addr.send(GetConfig);

    Arbiter::spawn(res.then(|res| {
        let config: Config = res.unwrap().unwrap();

        assert_eq!(config.connections.outbound_limit, 8);

        System::current().stop();
        future::result(Ok(()))
    }));

    sys.run();
}

#[test]
fn test_config_manager_load_config() {
    let sys = System::new("test");
    let addr = ConfigManager::default().start();
    let res = addr.send(LoadConfig {
        filename: "tests/fixtures/config.toml".to_string(),
    });

    Arbiter::spawn(res.then(|fut| {
        let config = fut.unwrap().unwrap();

        assert_eq!(config.connections.outbound_limit, 64);

        System::current().stop();
        future::result(Ok(()))
    }));

    sys.run();
}
