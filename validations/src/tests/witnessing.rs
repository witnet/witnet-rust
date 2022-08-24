use crate::witnessing::{validate_transport_address, TransportAddressError};

#[test]
fn test_validate_transport_addresses() {
    let addresses = vec![
        (
            "",
            Err(TransportAddressError::ParseError(
                url::ParseError::RelativeUrlWithoutBase,
            )),
        ),
        (
            "lorem ipsum",
            Err(TransportAddressError::ParseError(
                url::ParseError::RelativeUrlWithoutBase,
            )),
        ),
        (
            "127.0.0.1",
            Err(TransportAddressError::ParseError(
                url::ParseError::RelativeUrlWithoutBase,
            )),
        ),
        (
            "http://",
            Err(TransportAddressError::ParseError(
                url::ParseError::EmptyHost,
            )),
        ),
        (
            "ftp://127.0.0.1:9050",
            Err(TransportAddressError::UnsupportedScheme(String::from(
                "ftp",
            ))),
        ),
        (
            "socks5://127.0.0.1",
            Err(TransportAddressError::MissingPort),
        ),
        (
            "socks5://127.0.0.1:9050",
            Ok(String::from("socks5://127.0.0.1:9050")),
        ),
    ];

    for (address, expected) in addresses {
        let result = validate_transport_address(address);
        assert_eq!(result, expected);
    }
}
