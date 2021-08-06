//! Structures used as arguments for CLI subcommands.

use structopt::StructOpt;

/// Arguments for the `--decode-data-request` method.
#[derive(Debug, StructOpt)]
pub(crate) struct DecodeDataRequest {
    #[structopt(long, help = "Hexadecimal serialization of the data request output.")]
    pub hex: Option<String>,
    #[structopt(
        long,
        help = "File system path to an instance to a `.sol` file containing an instance of  `WitnetRequest`"
    )]
    pub from_solidity: Option<String>,
}

/// Arguments for the `--try-data-request` method.
#[derive(Debug, StructOpt)]
pub(crate) struct TryDataRequest {
    #[structopt(long, help = "Hexadecimal serialization of the data request output.")]
    pub hex: Option<String>,
    #[structopt(
        long,
        help = "File system path to an instance to a `.sol` file containing an instance of  `WitnetRequest`"
    )]
    pub from_solidity: Option<String>,
    #[structopt(
        long,
        help = "Whether to return the full execution trace, including partial results after each operator."
    )]
    pub full_trace: Option<bool>,
}

/// Easy derivation of `DecodeDataRequest` from `TryDataRequest`
impl From<TryDataRequest> for DecodeDataRequest {
    fn from(tdr: TryDataRequest) -> Self {
        Self {
            hex: tdr.hex,
            from_solidity: tdr.from_solidity,
        }
    }
}
