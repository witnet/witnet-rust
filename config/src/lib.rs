extern crate toml;
extern crate serde_json;

use std::fs::File;
use std::io::prelude::*;

use toml::Value as Toml;

pub fn read_config() -> Option<Toml> {
    let name: String = String::from("wit.toml");
    let mut input = String::new();
    File::open(&name).and_then(|mut f| {
        f.read_to_string(&mut input)
    }).unwrap();


    match input.parse() {
        Ok(toml) => {
            println!("{}", toml);
            Some(toml)
        }
        Err(error) => {
            panic!("failed to parse TOML: {}", error);
        }
    }
}
