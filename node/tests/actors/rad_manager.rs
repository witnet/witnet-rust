use witnet_config::config::Witnessing;
use witnet_node::actors::rad_manager::RadManager;

fn from_config_test_success_helper(
    allow_unproxied: bool,
    proxies: Vec<String>,
    expected_transports: Vec<Option<String>>,
) {
    let config = Witnessing {
        allow_unproxied,
        paranoid_percentage: 51,
        proxies,
    }
    .validate();
    let manager = RadManager::from_config(config);
    let actual_transports = &manager.witnessing.transports;
    assert_eq!(actual_transports, &expected_transports);
}

fn from_config_test_error_helper(
    allow_unproxied: bool,
    proxies: Vec<String>,
    expected_panic_message: &str,
) {
    let manager = std::panic::catch_unwind(|| {
        let config = Witnessing {
            allow_unproxied,
            paranoid_percentage: 51,
            proxies,
        }
        .validate();
        RadManager::from_config(config)
    });
    let panic_message = *manager.unwrap_err().downcast::<&str>().unwrap();
    assert_eq!(panic_message, expected_panic_message);
}

#[test]
fn test_unproxied_true_without_proxies() {
    let unproxied = true;
    let proxies = vec![];
    let expected_transports = vec![None];

    from_config_test_success_helper(unproxied, proxies, expected_transports);
}

#[test]
fn test_unproxied_true_with_proxies() {
    let unproxied = true;
    let proxies = vec![
        String::from("http://example.com"),
        String::from("http://domain.tld"),
    ];
    let expected_transports = vec![
        None,
        Some(String::from("http://example.com")),
        Some(String::from("http://domain.tld")),
    ];

    from_config_test_success_helper(unproxied, proxies, expected_transports);
}

#[test]
fn test_unproxied_false_with_proxies() {
    let unproxied = false;
    let proxies = vec![
        String::from("http://example.com"),
        String::from("http://domain.tld"),
    ];
    let expected_transports = vec![
        Some(String::from("http://example.com")),
        Some(String::from("http://domain.tld")),
    ];

    from_config_test_success_helper(unproxied, proxies, expected_transports);
}

#[test]
fn test_unproxied_false_without_proxies() {
    let unproxied = false;
    let proxies = vec![];
    let expected_panic_message = "Unproxied retrieval is disabled through configuration, but no proxy addresses have been configured. At least one HTTP transport needs to be enabled. Please either set the `connections.unproxied_retrieval` setting to `true` or add the address of at least one proxy in `connections.retrieval_proxies`.";

    from_config_test_error_helper(unproxied, proxies, expected_panic_message);
}
