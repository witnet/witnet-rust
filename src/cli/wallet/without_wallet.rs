use structopt::StructOpt;

use witnet_config::config::Config;

pub fn exec_cmd(_command: Command, _config: Config) -> Result<(), failure::Error> {
    println!("This executable has been compiled without the ability of running a Witnet wallet.");
    Ok(())
}

#[derive(Debug, StructOpt)]
pub struct Command {}
