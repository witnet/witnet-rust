//! data structures

// To enable `#[allow(clippy::all)]`
#![feature(tool_lints)]

/// data greeting
pub fn greetings() -> String {
    println!("Hello from data structures!");
    String::from("Hello from data structures!")
}

/// Generated Message module from Flatbuffers compiler
pub mod flatbuffers;
