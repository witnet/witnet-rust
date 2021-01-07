use witnet_data_structures::chain::Environment;
use witnet_node::{config_mngr, utils::test_actix_system};

#[test]
fn test_get_config() {
    test_actix_system(|| async {
        config_mngr::start_default();

        let config = config_mngr::get().await.unwrap();
        assert_eq!(Environment::Mainnet, config.environment);
    })
}

#[test]
fn test_load_config_from_file() {
    test_actix_system(|| async {
        config_mngr::start_default();

        config_mngr::load_from_file("tests/fixtures/config.toml".into())
            .await
            .unwrap();
        let config = config_mngr::get().await.unwrap();
        assert_eq!(64, config.connections.outbound_limit);
    })
}
