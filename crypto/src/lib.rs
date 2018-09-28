//! crypto

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// crypto greeting
pub fn greetings() -> String {
    println!("Hello from crypto!");
    String::from("Hello from crypto!")
}
