extern crate actix;
extern crate futures;
extern crate witnet_config;
extern crate witnet_core;

use actix::*;
use futures::{future, Future};
use witnet_core::actors::{config_manager::ConfigManager, messages::GetConfig};

#[test]
fn test_config_manager_load_config() {
    use std::path::PathBuf;
    let sys = System::new("test");
    let addr = ConfigManager::new(Some(PathBuf::from("tests/fixtures/config.toml"))).start();
    let res = addr.send(GetConfig);

    Arbiter::spawn(res.then(|fut| {
        let config = fut.unwrap().unwrap();

        assert_eq!(config.connections.outbound_limit, 64);

        System::current().stop();
        future::result(Ok(()))
    }));

    sys.run();
}
