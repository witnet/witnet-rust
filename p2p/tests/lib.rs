extern crate witnet_p2p as p2p;

use crate::p2p::greetings;

#[test]
fn p2p_greeeting() {
    assert_eq!(greetings(), String::from("Hello from p2p!"));
}
