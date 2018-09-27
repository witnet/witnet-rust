//! storage

#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![deny(missing_docs)]

/// storage greeting
pub fn greetings() -> String {
    println!("Hello from storage!");
    String::from("Hello from storage!")
}
