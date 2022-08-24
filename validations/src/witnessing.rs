use core::{
    convert::{From, TryInto},
    fmt,
};

use failure::Fail;
use itertools::Itertools;
use witnet_data_structures::witnessing::WitnessingConfig;

/// Checks whether a `WitnetssingConfig` value is valid.
///
/// Namely, this verifies that:
/// - Each of the addresses to use as transports are constructed correctly.
/// - The protocols of the transports are supported.
pub fn validate_witnessing_config<T, T2>(
    config: &WitnessingConfig<T>,
) -> Result<WitnessingConfig<T2>, WitnessingConfigError>
where
    T: Clone + fmt::Debug + fmt::Display,
    T2: Clone + fmt::Debug + fmt::Display + TryFrom<String>,
    <T2 as TryFrom<String>>::Error: fmt::Display,
{
    let mut valid = Vec::<Option<T2>>::new();
    let mut invalid = Vec::<(String, TransportAddressError)>::new();

    for option in config.transports.iter() {
        match option
            .clone()
            .map(|t| (t.clone(), validate_transport_address::<T, T2>(t)))
        {
            None => valid.push(None),
            Some((_, Ok(t2))) => valid.push(Some(t2)),
            Some((t, Err(e))) => invalid.push((t.to_string(), e)),
        }
    }

    if !invalid.is_empty() {
        return Err(WitnessingConfigError::Addresses(invalid));
    }

    Ok(WitnessingConfig {
        transports: valid,
        paranoid_threshold: config.paranoid_threshold,
    })
}

/// The error type for `validate_witnessing_config`
#[derive(Clone, Debug, Eq, Fail, PartialEq)]
pub enum WitnessingConfigError {
    /// The error is in the addresses.
    Addresses(Vec<(String, TransportAddressError)>),
}

impl fmt::Display for WitnessingConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let submessage = match self {
            WitnessingConfigError::Addresses(addresses) => {
                let interpolation = addresses
                    .iter()
                    .map(|(address, error)| format!("{} ({})", address, error))
                    .join("\n- ");

                format!(
                    "The following transport addresses are invalid:\n- {}",
                    interpolation
                )
            }
        };

        write!(f, "Invalid witnessing configuration. {}", submessage)
    }
}

/// All kind of errors that can happen when parsing and validating transport addresses.
#[derive(Clone, Debug, Eq, Fail, PartialEq)]
pub enum TransportAddressError {
    /// The address is missing a port number.
    #[fail(display = "the address is missing a port number")]
    MissingPort,
    /// Other errors.
    #[fail(display = "{}", _0)]
    Other(String),
    /// Error parsing a valid URL from the address.
    #[fail(display = "{}", _0)]
    ParseError(url::ParseError),
    /// Error parsing a valid URL from the address.
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
pub fn validate_transport_address<T, T2>(address: T) -> Result<T2, TransportAddressError>
where
    T: Clone + fmt::Display,
    T2: Clone + fmt::Display + TryFrom<String>,
    <T2 as TryFrom<String>>::Error: fmt::Display,
{
    // Fail if the address can't be parsed
    let parsed: url::Url = address
        .to_string()
        .as_str()
        .try_into()
        .map_err(TransportAddressError::ParseError)?;

    // Fail if the scheme is not supported
    let scheme = String::from(parsed.scheme());
    if !matches!(
        scheme.as_str(),
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) {
        Err(TransportAddressError::UnsupportedScheme(scheme))?;
    }

    // Fail if no port is provided
    if parsed.port().is_none() {
        Err(TransportAddressError::MissingPort)?;
    }

    let address_as_t2 = T2::try_from(address.to_string())
        .map_err(|e| TransportAddressError::Other(e.to_string()))?;

    Ok(address_as_t2)
}
