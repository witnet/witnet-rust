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
//This file is based on utils/src/lib.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

//! Logging, as well as various low-level utilities that factor Rust
//! patterns that are frequent within the codebase.

#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![warn(missing_docs)]

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

#[macro_use]
extern crate lazy_static;

extern crate serde;
#[macro_use]
extern crate serde_derive;

// Logging related
pub mod logger;
pub use logger::{init_logger, init_test_logger, LOGGER};

pub mod types;
pub use types::{LoggingConfig, LogLevel};