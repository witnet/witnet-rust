#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]

use std::process;

mod cli;

fn main() {
    let args = cli::from_args();
    if let Err(e) = cli::exec(args) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
