use witnet_node::utils::stop_system_if_panicking;

pub mod codec;
pub mod epoch_manager;

#[test]
#[should_panic = "actor dropped successfully"]
fn actors_dont_panic_while_panicking() {
    // There used to be a bug in stop_system_if_panicking that caused a double panic with message
    // "thread panicked while panicking. aborting."
    // if some actor was dropped during a panic while no actix system was currently running

    struct TestActor;

    impl Drop for TestActor {
        fn drop(&mut self) {
            stop_system_if_panicking("TestActor");
        }
    }

    let _actor = TestActor;
    panic!("actor dropped successfully");
}
