use actix;
use futures::Future;
use witnet_data_structures::chain::Environment;
use witnet_node::config_mngr;

fn ignore<T>(_: T) {}

#[test]
fn test_get_config() {
    actix::System::run(|| {
        config_mngr::start();

        let fut = config_mngr::get()
            .and_then(|config| {
                assert_eq!(Environment::Testnet1, config.environment);

                Ok(())
            })
            .then(|r| {
                actix::System::current().stop();
                futures::future::result(r)
            });

        actix::Arbiter::spawn(fut.map_err(ignore));
    });
}

#[test]
fn test_load_config_from_file() {
    actix::System::run(|| {
        config_mngr::start();

        let fut = config_mngr::load_from_file("tests/fixtures/config.toml".into())
            .and_then(|_| config_mngr::get())
            .and_then(|config| {
                assert_eq!(64, config.connections.outbound_limit);

                Ok(())
            })
            .then(|r| {
                actix::System::current().stop();
                futures::future::result(r)
            });

        actix::Arbiter::spawn(fut.map_err(ignore));
    });
}
