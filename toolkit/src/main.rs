//! # Witnet Toolkit
//!
//! Provides convenient and simple to use methods for building any kind of Witnet related tools,
//! either as libraries, FFIs or CLIs.
//!
//! ## Usage
//!
//! This crate can fundamentally be used in two different ways: as a CLI tool or as a Rust library.
//!
//! ### As a CLI tool
//!
//! When compiling this crate, the target binary is a CLI that can be used either standalone or
//! wrapped as a FFI for performing Witnet related operations in programming languages different
//! than Rust.
//!
//! ### As a Rust library
//!
//! The `lib.rs` file contains helper functions that can be easily imported into other Rust projects
//! in order to create Witnet related software using Rust.

use structopt::StructOpt;

use cli::commands::Command;

mod cli;

/// The main entrypoint for the `witnet_toolkit` binary.
///
/// This basically handles the core functionality of the CLI, and ensures that the process exits
/// gracefully.
#[tokio::main]
async fn main() {
    let command = Command::from_args();
    let exit_code = cli::process_command(command);
    std::process::exit(exit_code);
}
