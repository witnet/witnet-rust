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
//This file is based on config/src/config.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Configuration file management

use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::fs::File;

use toml;
use util::LoggingConfig;
use types::{ConfigError, ConfigMembers, GlobalConfig};

/// The default file name to use when trying to derive
/// the config file location

const CONFIG_FILE_NAME: &'static str = "wnd.toml";
const WND_HOME: &'static str = ".wnd";

/// Returns the defaults, as strewn throughout the code

impl Default for ConfigMembers {
    fn default() -> ConfigMembers {
        ConfigMembers {
            logging: Some(LoggingConfig::default()),
        }
    }
}

impl Default for GlobalConfig {
    fn default() -> GlobalConfig {
        GlobalConfig {
            config_file_path: None,
            using_config_file: false,
            members: Some(ConfigMembers::default()),
        }
    }
}

impl GlobalConfig {
    /// Try to load configuration file from typical sources (cwd, home, etc.)

    fn derive_location(&mut self) -> Result<(), ConfigError> {
        // First, check working directory
        let mut config_path = env::current_dir().unwrap();
        config_path.push(CONFIG_FILE_NAME);
        if config_path.exists() {
            self.config_file_path = Some(config_path);
            return Ok(());
        }

        // Next, look in directory of executable
        let mut config_path = env::current_exe().unwrap();
        config_path.pop();
        config_path.push(CONFIG_FILE_NAME);
        if config_path.exists() {
            self.config_file_path = Some(config_path);
            return Ok(());
        }

        // Then look in ~/.wnd
        let config_path = env::home_dir();
        if let Some(mut p) = config_path {
            p.push(WND_HOME);
            p.push(CONFIG_FILE_NAME);
            if p.exists() {
                self.config_file_path = Some(p);
                return Ok(());
            }
        }

        // Else, give up
        Err(ConfigError::FileNotFoundError(String::from("")))
    }

    /// Takes the path to a config file, or if NONE, tries to determine a
    /// config file based on rules in derive_config_location

    pub fn new(file_path: Option<&str>) -> Result<GlobalConfig, ConfigError> {
        let mut global_config = GlobalConfig::default();
        if let Some(fp) = file_path {
            global_config.config_file_path = Some(PathBuf::from(&fp));
        } else {
            let _result = global_config.derive_location();
        }

        // No attempt at a config file, just return defaults
        if let None = global_config.config_file_path {
            return Ok(global_config);
        }

        // Config gile path is given but not valid
        if !global_config.config_file_path.as_mut().unwrap().exists() {
            return Err(ConfigError::FileNotFoundError(String::from(
                global_config
                    .config_file_path
                    .as_mut()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .clone(),
            )));
        }

        // Try to parse the config file if it exists. Explode if it does exist but
        // something is wrong with it
        global_config.read()
    }

    /// Reads config

    pub fn read(mut self) -> Result<GlobalConfig, ConfigError> {
        let mut file = File::open(self.config_file_path.as_mut().unwrap())?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let decoded: Result<ConfigMembers, toml::de::Error> = toml::from_str(&contents);
        match decoded {
            Ok(gc) => {
                self.using_config_file = true;
                self.members = Some(gc);
                return Ok(self);
            }
            Err(e) => {
                return Err(ConfigError::ParseError(
                    String::from(
                        self.config_file_path
                            .as_mut()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .clone(),
                    ),
                    String::from(format!{"{}", e}),
                ));
            }
        }
    }

    /// Serialize config

    pub fn serialize(&mut self) -> Result<String, ConfigError> {
        let encoded: Result<String, toml::ser::Error> =
            toml::to_string(self.members.as_mut().unwrap());
        match encoded {
            Ok(enc) => return Ok(enc),
            Err(e) => {
                return Err(ConfigError::SerializationError(String::from(format!(
                    "{}",
                    e
                ))));
            }
        }
    }
}
