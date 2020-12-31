use witnet_data_structures::chain::Environment;
use witnet_node::config_mngr;

#[test]
fn test_get_config() {
    actix::System::run(|| {
        config_mngr::start_default();

        let fut = async {
            let config = config_mngr::get().await.unwrap();
            assert_eq!(Environment::Mainnet, config.environment);
            actix::System::current().stop();
        };

        actix::Arbiter::spawn(fut);
    })
    .unwrap();
}

#[test]
fn test_load_config_from_file() {
    actix::System::run(|| {
        config_mngr::start_default();

        let fut = async {
            config_mngr::load_from_file("tests/fixtures/config.toml".into())
                .await
                .unwrap();
            let config = config_mngr::get().await.unwrap();
            assert_eq!(64, config.connections.outbound_limit);
            actix::System::current().stop();
        };

        actix::Arbiter::spawn(fut);
    })
    .unwrap();
}
