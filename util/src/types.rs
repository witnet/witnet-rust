//This file is part of Rust-Witnet.
//
//Rust-Witnet is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
//Rust-Witnet is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
//You should have received a copy of the GNU General Public License
// along with Rust-Witnet. If not, see <http://www.gnu.org/licenses/>.
//
//This file is based on utils/src/types.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Logging configuration types

/// Log level types, as slog's don't implement serialize
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    /// Critical
    Critical,
    /// Error
    Error,
    /// Warning
    Warning,
    /// Info
    Info,
    /// Debug
    Debug,
    /// Trace
    Trace,
}

/// Logging config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// whether to log to stdout
    pub log_to_stdout: bool,
    /// logging level for stdout
    pub stdout_log_level: LogLevel,
    /// whether to log to file
    pub log_to_file: bool,
    /// log file level
    pub file_log_level: LogLevel,
    /// Log file path
    pub log_file_path: String,
    /// Whether to append to log or replace
    pub log_file_append: bool,
}

impl Default for LoggingConfig {
    fn default() -> LoggingConfig {
        LoggingConfig {
            log_to_stdout: true,
            stdout_log_level: LogLevel::Debug,
            log_to_file: false,
            file_log_level: LogLevel::Trace,
            log_file_path: String::from("wit.log"),
            log_file_append: false,
        }
    }
}
