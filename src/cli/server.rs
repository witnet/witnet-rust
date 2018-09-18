use clap::{App, Arg, SubCommand};
use server;

pub fn get_arg<'a>() -> App<'a, 'a> {
    SubCommand::with_name("server")
        .about("Run the Witnet server")
        .arg(
            Arg::with_name("address")
                .short("d")
                .long("address")
                .help("TCP address to which the server should bind")
                .takes_value(true)
                .default_value(server::DEFAULT_ADDRESS),
        )
        .arg(
            Arg::with_name("background")
                .short("b")
                .long("background")
                .help("Run the server in the background"),
        )
}
