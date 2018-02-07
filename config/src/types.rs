//This file is part of Rust-Witnet.
//
//Rust-Witnet is free software: you can redistribute it and/or modify
//it under the terms of the GNU General Public License as published by
//the Free Software Foundation, either version 3 of the License, or
//(at your option) any later version.
//
//Rust-Witnet is distributed in the hope that it will be useful,
//but WITHOUT ANY WARRANTY; without even the implied warranty of
//MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//GNU General Public License for more details.
//
//You should have received a copy of the GNU General Public License
//along with Rust-Witnet. If not, see <http://www.gnu.org/licenses/>.
//
//This file is based on config/src/types.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Public types for config modules

use std::path::PathBuf;
use std::io;
use std::fmt;

use util::LoggingConfig;

/// Error type wrapping config errors.
#[derive(Debug)]
pub enum ConfigError {
    /// Error with parsing of config file
    ParseError(String, String),

    /// Error with fileIO while reading config file
    FileIOError(String, String),

    /// No file found
    FileNotFoundError(String),

    /// Error serializing config values
    SerializationError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ConfigError::ParseError(ref file_name, ref message) => write!(
                f,
                "Error parsing configuration file at {} - {}",
                file_name, message
            ),
            ConfigError::FileIOError(ref file_name, ref message) => {
                write!(f, "{} {}", message, file_name)
            }
            ConfigError::FileNotFoundError(ref file_name) => {
                write!(f, "Configuration file not found: {}", file_name)
            }
            ConfigError::SerializationError(ref message) => {
                write!(f, "Error serializing configuration: {}", message)
            }
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> ConfigError {
        ConfigError::FileIOError(
            String::from(""),
            String::from(format!("Error loading config file: {}", error)),
        )
    }
}
/// Going to hold all of the various configuration types separately for now,
/// then put them together as a single ServerConfig object afterwards. This is
/// to flatten out the configuration file into logical sections, as they tend
/// to be quite nested in the code Most structs optional, as they may or may
/// not be needed depending on what's being run
#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Keep track of the file we've read
    pub config_file_path: Option<PathBuf>,
    /// keep track of whether we're using
    /// a config file or just the defaults
    /// for each member
    pub using_config_file: bool,
    /// Global member config
    pub members: Option<ConfigMembers>,
}

/// Keeping an 'inner' structure here, as the top level GlobalConfigContainer
/// options might want to keep internal state that we don't necessarily want
/// serialised or deserialised
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigMembers {
    /// Logging config
    pub logging: Option<LoggingConfig>,
}
