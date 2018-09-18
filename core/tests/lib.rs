extern crate witnet_core as core;

use core::greetings;

#[test]
fn test_core_greeeting() {
    assert_eq!(greetings(), String::from("Hello from core!"));
}
