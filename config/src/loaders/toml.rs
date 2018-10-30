//! Load the configuration from a file or a `String` written in [Toml format](Tomlhttps://en.wikipedia.org/wiki/TOML)

use crate::Config;
use failure::Fail;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use toml;
use witnet_util::error::{WitnetError, WitnetResult};

/// `toml::de::Error`, but loading that configuration from a file
/// might also fail with a `std::io::Error`.
#[derive(Debug, Fail)]
pub enum Error {
    /// Indicates there was an error when trying to load configuration from a file.
    IOError(io::Error),
    /// Indicates there was an error when trying to build a
    /// `witnet_config::Config` instance out of the Toml string given.
    ParseError(toml::de::Error),
}

/// Formats the error in a user-friendly manners. Suitable for telling
/// the user what error happened when loading/parsing the
/// configuration.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::IOError(e) => e.fmt(f),
            Error::ParseError(e) => e.fmt(f),
        }
    }
}

/// Just like `std::result::Result` but withe error param fixed to
/// `Error` type in this module.
pub type Result<T> = WitnetResult<T, Error>;

/// Load configuration from a file written in Toml format.
pub fn from_file(filename: &str) -> Result<Config> {
    let mut contents = String::new();
    read_file_contents(filename, &mut contents).map_err(Error::IOError)?;
    from_str(&contents)
}

#[cfg(not(test))]
fn read_file_contents(filename: &str, contents: &mut String) -> io::Result<usize> {
    let mut file = File::open(filename)?;
    file.read_to_string(contents)
}

#[cfg(test)]
fn read_file_contents(_filename: &str, _contents: &mut String) -> io::Result<usize> {
    Ok(0)
}

/// Load configuration from a string written in Toml format.
pub fn from_str(contents: &str) -> Result<Config> {
    toml::from_str(contents).map_err(|e| WitnetError::from(Error::ParseError(e)))
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_load_empty_config() {
        let config = super::from_str("").unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_load_empty_config_from_file() {
        let config = super::from_file("some file name").unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_load_non_empty_config() {
        let config = super::from_str(
            r"
[connections]
outbound_limit = 32
[storage]
db_path = 'other-path'
",
        )
        .unwrap();
        assert_eq!(config.connections.outbound_limit, 32);
    }

    #[test]
    fn test_load_incorrect_config() {
        let config = super::from_str(
            r"
[connections]
outbound_limit = 'not a number'
",
        );

        assert!(config.is_err());
    }
}
