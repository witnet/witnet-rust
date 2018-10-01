//! data structures

// Removed due to flatbuffers generated code
// #![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// data greeting
pub fn greetings() -> String {
    println!("Hello from data structures!");
    String::from("Hello from data structures!")
}

/// Generated Message module from Flatbuffers compiler
#[allow(missing_docs)]
pub mod protocol_generated;
