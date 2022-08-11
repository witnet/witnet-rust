use std::fmt;

use core::convert::From;
use failure::Fail;
use itertools::Itertools;
use witnet_data_structures::witnessing::WitnessingConfig;

/// The error type for `validate_witnessing_config`
#[derive(Clone, Debug, Fail, PartialEq)]
pub enum WitnessingConfigError {
    /// The error is in the addresses.
    Addresses(Vec<(String, TransportAddressError)>),
}

impl fmt::Display for WitnessingConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WitnessingConfigError::Addresses(addresses) => {
                let interpolation = addresses
                    .iter()
                    .map(|(address, error)| format!("{} ({})", address, error))
                    .join("\n- ");

                write!(
                    f,
                    "The following transport addresses are invalid:\n- {}",
                    interpolation
                )
            }
        }
    }
}

/// Checks whether a `WitnetssingConfig` value is valid.
///
/// Namely, this verifies that:
/// - Each of the addresses to use as transports are constructed correctly.
/// - The protocols of the transports are supported.
pub fn validate_witnessing_config(config: &WitnessingConfig) -> Result<(), WitnessingConfigError> {
    // Collect only the bad transport addresses
    let invalid_addresses = config
        .transports
        .iter()
        .cloned()
        .filter_map(|option| {
            option.and_then(|address| {
                if let Err(err) = validate_transport_address(&address) {
                    Some((address, err))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();

    if !invalid_addresses.is_empty() {
        return Err(WitnessingConfigError::Addresses(invalid_addresses));
    }

    Ok(())
}

///
#[derive(Clone, Debug, Fail, PartialEq)]
pub enum TransportAddressError {
    /// The address is missing a port number.
    #[fail(display = "the address is missing a port number")]
    MissingPort,
    /// Error parsing a valid URL from the address.
    #[fail(display = "{}", _0)]
    ParseError(url::ParseError),
    /// The scheme (`http`, `socks5`, etc.) found in the address is not supported.
    #[fail(display = "\"{}\" is not a supported type of transport", _0)]
    UnsupportedScheme(String),
}

impl From<url::ParseError> for TransportAddressError {
    fn from(error: url::ParseError) -> Self {
        Self::ParseError(error)
    }
}

/// Tells whether a transport address is well-formed.
pub fn validate_transport_address(address: &str) -> Result<(), TransportAddressError> {
    // Fail if the address can't be parsed
    let url = url::Url::parse(address).map_err(TransportAddressError::from)?;

    // Fail if the scheme is not supported
    let scheme = String::from(url.scheme());
    if !matches!(
        scheme.as_str(),
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) {
        Err(TransportAddressError::UnsupportedScheme(scheme))?;
    }

    // Fail if no port is provided
    if url.port().is_none() {
        Err(TransportAddressError::MissingPort)?;
    }

    Ok(())
}
