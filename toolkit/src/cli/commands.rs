//! Structures used for defining the available CLI commands and subcommands.

use structopt::StructOpt;

use super::arguments;

#[derive(Debug, StructOpt)]
pub(crate) enum SubCommand {
    #[structopt(name = "decode-data-request", about = "Decodes a data request.")]
    DecodeDataRequest(arguments::DecodeDataRequest),
    #[structopt(
        name = "try-data-request",
        about = "Tries a data request locally so as to preview what its result could be as of now."
    )]
    TryDataRequest(arguments::TryDataRequest),
}

#[derive(Debug, StructOpt)]
pub(crate) struct Command {
    #[structopt(subcommand)]
    pub subcommand: SubCommand,
}
