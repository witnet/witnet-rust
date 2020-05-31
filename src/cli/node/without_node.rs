use structopt::StructOpt;

use std::path::PathBuf;
use witnet_config::config::Config;

pub fn exec_cmd(
    _command: Command,
    _config_path: Option<PathBuf>,
    _config: Config,
) -> Result<(), failure::Error> {
    println!("This executable has been compiled without the ability of running a Witnet node.");
    Ok(())
}

#[derive(Debug, StructOpt)]
pub struct Command {}
