use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use witnet_p2p::sessions::*;

/// Check if the sessions default initializes with empty state
#[test]
fn p2p_sessions_default() {
    // Create sessions struct
    let sessions = Sessions::<String>::default();

    // Check that sessions server is none
    assert!(sessions.server_address.is_none());

    // Check that sessions collections are empty
    assert_eq!(sessions.inbound_sessions.collection.len(), 0);
    assert_eq!(sessions.outbound_sessions.collection.len(), 0);
}

/// Check setting the server address
#[test]
fn p2p_sessions_set_server() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Check server address is none
    assert!(sessions.server_address.is_none());

    // Set server address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    sessions.set_server_address(address);

    // Check server address is now set
    assert!(sessions.server_address.is_some());
    assert_eq!(sessions.server_address.unwrap(), address);
}

/// Check setting the sessions limits
#[test]
fn p2p_sessions_set_limits() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Check sessions limits are set to none
    assert!(sessions.inbound_sessions.limit.is_none());
    assert!(sessions.outbound_sessions.limit.is_none());

    // Set sessions limits
    let limit_inbound = 2;
    let limit_outbound = 3;
    sessions.set_limits(limit_inbound, limit_outbound);

    // Check sessions limits have been set
    assert!(sessions.inbound_sessions.limit.is_some());
    assert_eq!(sessions.inbound_sessions.limit.unwrap(), limit_inbound);
    assert!(sessions.outbound_sessions.limit.is_some());
    assert_eq!(sessions.outbound_sessions.limit.unwrap(), limit_outbound);
}

/// Check if addresses are eligible as outbound addresses
#[test]
fn p2p_sessions_is_outbound_address_eligible() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Set server address
    let server_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8000);
    sessions.set_server_address(server_address);

    // Register an outbound session and check if result is Ok(())
    let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_address,
            "reference1".to_string()
        )
        .is_ok());

    // Register an inbound session and check if result is Ok(())
    let inbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Inbound,
            inbound_address,
            "reference1".to_string()
        )
        .is_ok());

    // Check invalid addresses
    assert!(!sessions.is_outbound_address_eligible(server_address));
    assert!(!sessions.is_outbound_address_eligible(outbound_address));

    // Check inbound address as valid address
    assert!(sessions.is_outbound_address_eligible(inbound_address));

    // Check valid addresses
    let valid_address_1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8000);
    let valid_address_2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8003);
    let valid_address_3 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8003);

    assert!(sessions.is_outbound_address_eligible(valid_address_1));
    assert!(sessions.is_outbound_address_eligible(valid_address_2));
    assert!(sessions.is_outbound_address_eligible(valid_address_3));
}

/// Check the registration of sessions
#[test]
fn p2p_sessions_register() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Register an outbound session and check if result is Ok(())
    let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_address,
            "reference1".to_string()
        )
        .is_ok());

    // Check if outbound session was register successfully
    assert!(sessions
        .outbound_sessions
        .collection
        .contains_key(&outbound_address));

    // Register an inbound session and check if result is Ok(())
    let inbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Inbound,
            inbound_address,
            "reference2".to_string()
        )
        .is_ok());

    // Check if inbound session was register successfully
    assert!(sessions
        .inbound_sessions
        .collection
        .contains_key(&inbound_address));
}

/// Check the unregistration of sessions
#[test]
fn p2p_sessions_unregister() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Register an sessions and check if result is Ok(())
    let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_address,
            "reference1".to_string()
        )
        .is_ok());
    let inbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Inbound,
            inbound_address,
            "reference2".to_string()
        )
        .is_ok());

    // Unregister sessions
    assert!(sessions
        .unregister_session(SessionType::Outbound, outbound_address)
        .is_ok());
    assert!(sessions
        .unregister_session(SessionType::Inbound, inbound_address)
        .is_ok());

    // Check that both sessions are removed from collections
    assert_eq!(sessions.outbound_sessions.collection.len(), 0);
    assert_eq!(sessions.inbound_sessions.collection.len(), 0);
}

/// Check the update of sessions
#[test]
fn p2p_sessions_update() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Register an sessions and check if result is Ok(())
    let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_address,
            "reference1".to_string()
        )
        .is_ok());
    let inbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Inbound,
            inbound_address,
            "reference2".to_string()
        )
        .is_ok());

    // Check status of registered sessions was set to Unconsolidated
    assert!(sessions
        .outbound_sessions
        .collection
        .get(&outbound_address)
        .is_some());
    assert!(sessions
        .inbound_sessions
        .collection
        .get(&inbound_address)
        .is_some());
    assert_eq!(
        sessions
            .outbound_sessions
            .collection
            .get(&outbound_address)
            .unwrap()
            .status,
        SessionStatus::Unconsolidated
    );
    assert_eq!(
        sessions
            .inbound_sessions
            .collection
            .get(&inbound_address)
            .unwrap()
            .status,
        SessionStatus::Unconsolidated
    );

    // Update sessions status to Consolidated
    assert!(sessions
        .update_session(
            SessionType::Outbound,
            outbound_address,
            SessionStatus::Consolidated
        )
        .is_ok());
    assert!(sessions
        .update_session(
            SessionType::Inbound,
            inbound_address,
            SessionStatus::Consolidated
        )
        .is_ok());

    // Check if sessions were updated
    assert_eq!(
        sessions
            .outbound_sessions
            .collection
            .get(&outbound_address)
            .unwrap()
            .status,
        SessionStatus::Consolidated
    );
    assert_eq!(
        sessions
            .inbound_sessions
            .collection
            .get(&inbound_address)
            .unwrap()
            .status,
        SessionStatus::Consolidated
    );
}
