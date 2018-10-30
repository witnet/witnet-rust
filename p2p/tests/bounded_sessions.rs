use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use witnet_p2p::sessions::bounded_sessions::*;
use witnet_p2p::sessions::SessionStatus;

/// Check if the bounded sessions default initializes empty collection and no limit
#[test]
fn p2p_bounded_sessions_default() {
    // Create bounded sessions struct
    let sessions = BoundedSessions::<String>::default();

    // Check collections is empty
    assert_eq!(sessions.collection.len(), 0);

    // Check that no limit has been set by default
    assert_eq!(sessions.limit, None);
}

/// Check the registration of a session
#[test]
fn p2p_bounded_sessions_register() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::default();

    // Register session and check if result is Ok(())
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    assert!(sessions.register_session(address, "reference1").is_ok());

    // Check that the collection has registered session
    assert_eq!(sessions.collection.len(), 1);

    // Check status of recently registered session
    let session_info = sessions.collection.get(&address);
    assert!(session_info.is_some());
    assert_eq!(session_info.unwrap().status, SessionStatus::Unconsolidated);
}

// Check the unregistration of a session
#[test]
fn p2p_bounded_sessions_unregister() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::default();

    // Register session and check if result is Ok(())
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    assert!(sessions.register_session(address, "reference1").is_ok());
    assert_eq!(sessions.collection.len(), 1);

    // Unregister session and check if success
    assert!(sessions.unregister_session(address).is_ok());

    // Expect element to be removed
    assert_eq!(sessions.collection.len(), 0);
}

// Check the update of a session
#[test]
fn p2p_bounded_sessions_update() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::default();

    // Add session
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    assert!(sessions.register_session(address, "reference1").is_ok());
    assert_eq!(sessions.collection.len(), 1);

    // Check value of recently registered session
    let session_info = sessions.collection.get(&address);
    assert!(session_info.is_some());
    assert_eq!(session_info.unwrap().status, SessionStatus::Unconsolidated);

    // Update session status
    assert!(sessions
        .update_session(address, SessionStatus::Consolidated)
        .is_ok());

    // Check udpated status
    let session_info = sessions.collection.get(&address);
    assert!(session_info.is_some());
    assert_eq!(session_info.unwrap().status, SessionStatus::Consolidated);
}

/// Check if the sessions limit is being used
#[test]
fn p2p_bounded_sessions_register_limit() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::default();
    sessions.set_limit(0);

    // Add session
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let result = sessions.register_session(address, "reference1");

    // Expect error
    assert!(result.is_err());
}

/// Check if the sessions cannot be registered twice
#[test]
fn p2p_bounded_sessions_register_twice() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::default();

    // Register session
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    assert!(sessions.register_session(address, "reference1").is_ok());

    // Try to register same session
    assert!(sessions.register_session(address, "reference1").is_err());
}

/// Check if non-existent session cannot be unregistered
#[test]
fn p2p_bounded_sessions_unregister_unknown() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::<String>::default();

    // Unregister non-existent session
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    assert!(sessions.unregister_session(address).is_err());
}

/// Check if non-existent session cannot be updated
#[test]
fn p2p_bounded_sessions_update_unknown() {
    // Create bounded sessions struct
    let mut sessions = BoundedSessions::<String>::default();

    // Unregister non-existent session
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    assert!(sessions
        .update_session(address, SessionStatus::Consolidated)
        .is_err());
}
