use witnet_storage as storage;

use crate::storage::greetings;

#[test]
fn storage_greeeting() {
    assert_eq!(greetings(), String::from("Hello from storage!"));
}
