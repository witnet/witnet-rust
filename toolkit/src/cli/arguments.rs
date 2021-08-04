//! Structures used as arguments for CLI subcommands.

use structopt::StructOpt;

/// Arguments for the `--decode-data-request` method.
#[derive(Debug, StructOpt)]
pub(crate) struct DecodeDataRequest {
    #[structopt(long, help = "Hexadecimal serialization of the data request output.")]
    pub hex: Option<String>,
}

/// Arguments for the `--try-data-request` method.
#[derive(Debug, StructOpt)]
pub(crate) struct TryDataRequest {
    #[structopt(long, help = "Hexadecimal serialization of the data request output.")]
    pub hex: Option<String>,
    #[structopt(
        long,
        help = "Whether to return the full execution trace, including partial results after each operator."
    )]
    pub full_trace: Option<bool>,
}
