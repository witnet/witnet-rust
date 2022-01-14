//! A convenient and lightweight CLI providing Witnet related utilities.
//!
//! In comparison to the former `witnet-rust` CLI, this one is "standalone", i.e. all the commands
//! provided here are processed locally, without any need to connect to a Witnet node.

mod arguments;
pub(crate) mod commands;
pub(crate) mod data_requests;

use crate::errors::Error;

/// This function acts as the main router and error handler for CLI commands.
pub(crate) fn process_command(command: commands::Command) -> i32 {
    match command.subcommand {
        // `--decode-data-request`
        commands::SubCommand::DecodeDataRequest(args) => data_requests::decode_from_args(args)
            .and_then(|decoded| serde_json::to_string(&decoded).map_err(Error::JsonSerialize)),
        // `--try-data-request`
        commands::SubCommand::TryDataRequest(args) => data_requests::try_from_args(args)
            .and_then(|report| serde_json::to_string(&report).map_err(Error::JsonSerialize)),
    }
    // The output of successful commands is printed to `stdout`, and a `0` exit code is returned
    .map(|result| {
        println!("{}", result);

        0
    })
    // The output of failed commands is printed to `stderr`, and a `1` exit code is returned
    .unwrap_or_else(|error| {
        eprintln!("{}", error);

        1
    })
}
