//! Load the configuration from a file or a `String` written in [Toml format](Tomlhttps://en.wikipedia.org/wiki/TOML)

#[cfg(test)]
use std::cell::Cell;
use std::io;
use std::path::Path;

use failure::Fail;

use crate::config::PartialConfig;

/// `toml::de::Error`, but loading that configuration from a file
/// might also fail with a `std::io::Error`.
#[derive(Debug, Fail)]
pub enum Error {
    /// There was an error when trying to load configuration from a file.
    #[fail(display = "Error reading config file: {}", _0)]
    IOError(io::Error),
    /// Indicates there was an error when trying to build a
    /// `witnet_config::config::PartialConfig` instance out of the Toml string given.
    #[fail(display = "Error parsing config file: {}", _0)]
    ParseError(toml::de::Error),
}

/// Load configuration from a file written in Toml format.
pub fn from_file<S: AsRef<Path>>(file: S) -> Result<PartialConfig, Error> {
    let f = file.as_ref();
    let mut contents = String::new();

    log::debug!("Loading config from `{}`", f.to_string_lossy());

    read_file_contents(f, &mut contents).map_err(Error::IOError)?;
    from_str(&contents).map_err(Error::ParseError)
}

#[cfg(not(test))]
fn read_file_contents(file: &Path, contents: &mut String) -> io::Result<usize> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(file)?;
    file.read_to_string(contents)
}

/// Load configuration from a string written in Toml format.
pub fn from_str(contents: &str) -> Result<PartialConfig, toml::de::Error> {
    toml::from_str(contents)
}

#[cfg(test)]
thread_local!(static FILE_CONTENTS: Cell<&'static str> = Cell::new(""));

#[cfg(test)]
fn read_file_contents(_filename: &Path, contents: &mut String) -> io::Result<usize> {
    FILE_CONTENTS.with(|cell| {
        let value = cell.get();
        contents.insert_str(0, value);
        Ok(value.len())
    })
}

#[cfg(test)]
mod tests {
    use crate::config::*;
    use std::path::{Path, PathBuf};
    use witnet_data_structures::chain::Environment;

    #[test]
    fn test_load_empty_config() {
        let config = super::from_str("").unwrap();

        assert_eq!(config, PartialConfig::default());
    }

    #[test]
    fn test_load_empty_config_from_file() {
        super::FILE_CONTENTS.with(|cell| cell.set(""));
        let filename = Path::new("config.toml");
        let config = super::from_file(&filename).unwrap();

        assert_eq!(config, PartialConfig::default());
    }

    #[test]
    fn test_load_config_from_file() {
        super::FILE_CONTENTS.with(|cell| {
            cell.set(
                r"
environment = 'testnet'
[connections]
inbound_limit = 999
    ",
            )
        });
        let filename = Path::new("config.toml");
        let config = super::from_file(&filename).unwrap();

        assert_eq!(config.environment, Environment::Testnet);
        assert_eq!(config.connections.inbound_limit, Some(999));
    }

    #[test]
    fn test_configure_environment() {
        let config = super::from_str("environment = 'mainnet'").unwrap();
        let result = super::from_str("environment = 'wrong'");

        assert_eq!(config.environment, Environment::Mainnet);
        assert!(result.is_err());
    }

    #[test]
    fn test_configure_connections() {
        let empty_config = super::from_str("[connections]").unwrap();
        let config = super::from_str(
            r"
[connections]
server_addr = '127.0.0.1:1234'
known_peers = ['192.168.1.12:1234']
    ",
        )
        .unwrap();

        assert_eq!(empty_config.connections, PartialConnections::default());
        assert_eq!(empty_config.connections.known_peers.len(), 0);
        assert_eq!(
            config.connections.server_addr,
            Some("127.0.0.1:1234".parse().unwrap())
        );
        assert_eq!(config.connections.known_peers.len(), 1);
    }

    #[test]
    fn test_configure_storage() {
        let empty_config = super::from_str("[storage]").unwrap();
        let config = super::from_str(
            r"
[storage]
db_path = 'dbfiles'
    ",
        )
        .unwrap();

        assert_eq!(empty_config.storage, PartialStorage::default());
        assert_eq!(config.storage.db_path, Some(PathBuf::from("dbfiles")));
    }

    #[test]
    fn test_load_durations() {
        use std::time::Duration;

        let empty_config = super::from_str("[storage]").unwrap();
        let config = super::from_str(
            r"
[connections]
bootstrap_peers_period_seconds = 11
storage_peers_period_seconds = 7
handshake_timeout_seconds = 21
    ",
        )
        .unwrap();

        // Check default values in empty config
        assert_eq!(
            empty_config.connections.bootstrap_peers_period,
            PartialConnections::default().bootstrap_peers_period
        );
        assert_eq!(
            empty_config.connections.storage_peers_period,
            PartialConnections::default().storage_peers_period
        );
        assert_eq!(
            empty_config.connections.handshake_timeout,
            PartialConnections::default().handshake_timeout
        );

        // Check values in initialized config
        assert_eq!(
            config.connections.bootstrap_peers_period,
            Some(Duration::from_secs(11))
        );
        assert_eq!(
            config.connections.storage_peers_period,
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            config.connections.handshake_timeout,
            Some(Duration::from_secs(21))
        );
    }

    #[test]
    fn test_configure_jsonrpc() {
        let empty_config = super::from_str("[jsonrpc]").unwrap();
        let config = super::from_str(
            r"
[jsonrpc]
server_address = '127.0.0.1:1234'
    ",
        )
        .unwrap();
        let config_disabled = super::from_str(
            r"
[jsonrpc]
enabled = false
    ",
        )
        .unwrap();

        assert_eq!(empty_config.jsonrpc, PartialJsonRPC::default());
        assert_eq!(
            config.jsonrpc.server_address,
            Some("127.0.0.1:1234".parse().unwrap())
        );
        assert_eq!(config_disabled.jsonrpc.enabled, Some(false),);
    }

    #[test]
    fn test_configure_mining() {
        let empty_config = super::from_str("[mining]").unwrap();
        let config_disabled = super::from_str(
            r"
[mining]
enabled = false
    ",
        )
        .unwrap();

        assert_eq!(empty_config.mining, PartialMining::default());
        assert_eq!(config_disabled.mining.enabled, Some(false),);
    }

    #[test]
    fn test_configure_tapi_missing_fields() {
        // Check that the tapi table does not need to explicitly set all the new "oppose_wip" fields
        // and they default to "false"
        let empty_config = super::from_str("[tapi]").unwrap();

        let config_oppose_0021 = super::from_str(
            r"
[tapi]
oppose_wip0021 = true
    ",
        )
        .unwrap();

        assert_eq!(empty_config.tapi, Tapi::default());
        assert!(!empty_config.tapi.oppose_wip0021);
        assert!(config_oppose_0021.tapi.oppose_wip0021);
    }
}
