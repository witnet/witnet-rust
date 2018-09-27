use witnet_data_structures as data_structures;

use crate::data_structures::greetings;

#[test]
fn data_structures_greeeting() {
    assert_eq!(greetings(), String::from("Hello from data structures!"));
}
