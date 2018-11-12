#![feature(bind_by_move_pattern_guards)]

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
    assert_eq!(sessions.inbound.collection.len(), 0);
    assert_eq!(sessions.outbound_consolidated.collection.len(), 0);
    assert_eq!(sessions.outbound_unconsolidated.collection.len(), 0);
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
    assert!(sessions.inbound.limit.is_none());
    assert!(sessions.outbound_consolidated.limit.is_none());
    assert!(sessions.outbound_unconsolidated.limit.is_none());

    // Set sessions limits
    let limit_inbound = 2;
    let limit_outbound_consolidated = 3;
    sessions.set_limits(limit_inbound, limit_outbound_consolidated);

    // Check sessions limits have been set (except unconsolidated limit)
    assert!(sessions.inbound.limit.is_some());
    assert_eq!(sessions.inbound.limit.unwrap(), limit_inbound);
    assert!(sessions.outbound_consolidated.limit.is_some());
    assert_eq!(
        sessions.outbound_consolidated.limit.unwrap(),
        limit_outbound_consolidated
    );
    assert!(sessions.outbound_unconsolidated.limit.is_none());
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

/// Check if the sum of all outbound sessions (consolidated and unconsolidated) is returned
#[test]
fn p2p_sessions_get_num_outbound_sessions() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Register an outbound unconsolidated session and check if result is Ok(())
    let outbound_uncons_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_uncons_address,
            "reference1".to_string()
        )
        .is_ok());

    // Register an outbound unconsolidated session, check if result is Ok(()) and consolidate it
    // afterwards
    let outbound_cons_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_cons_address,
            "reference1".to_string()
        )
        .is_ok());
    assert!(sessions
        .consolidate_session(SessionType::Outbound, outbound_cons_address)
        .is_ok());

    // Check that the function to be tested returns the total number of outbound sessions
    assert_eq!(sessions.get_num_outbound_sessions(), 2);
}

/// Check the conditions upon which the outbound bootstrap is needed
#[test]
fn p2p_sessions_is_outbound_bootstrap_needed() {
    // Create sessions struct (outbound unlimited by default)
    let mut sessions = Sessions::<String>::default();

    // Register and consolidate sessions
    for i in 1..4 {
        let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8000 + i);
        sessions
            .register_session(
                SessionType::Outbound,
                outbound_address,
                "reference1".to_string(),
            )
            .unwrap_or(());
        sessions
            .consolidate_session(SessionType::Outbound, outbound_address)
            .unwrap_or(());
    }

    // Register sessions (unconsolidated)
    for i in 1..4 {
        let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8000 + i);
        sessions
            .register_session(
                SessionType::Outbound,
                outbound_address,
                "reference1".to_string(),
            )
            .unwrap_or(());
    }

    // Check number of sessions registered
    assert_eq!(sessions.get_num_outbound_sessions(), 6);

    // Bootstrap is always needed when there is no limit
    assert!(sessions.is_outbound_bootstrap_needed());

    // Set limits
    let limit_inbound = 1;
    let limit_outbound = 7;
    sessions.set_limits(limit_inbound, limit_outbound);

    // Bootstrap is needed when the limit is higher than the number of outbound sessions
    assert!(sessions.is_outbound_bootstrap_needed());

    // Set limits
    let limit_inbound = 1;
    let limit_outbound = 6;
    sessions.set_limits(limit_inbound, limit_outbound);

    // Bootstrap is not needed when the limit is equal to the number of outbound sessions
    assert!(!sessions.is_outbound_bootstrap_needed());

    // Set limits
    let limit_inbound = 1;
    let limit_outbound = 5;
    sessions.set_limits(limit_inbound, limit_outbound);

    // Bootstrap is not needed when the limit is smaller than the number of outbound sessions
    assert!(!sessions.is_outbound_bootstrap_needed());
}

/// Check the function to get a random outbound consolidated session
#[test]
fn p2p_sessions_get_random_anycast_session() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Check that the function returns None when there are no sessions in the collection
    assert_eq!(sessions.get_random_anycast_session(), None);

    // Register an outbound session and check if result is Ok(())
    let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_address,
            "reference1".to_string()
        )
        .is_ok());

    // Check that the function returns None when there are no consolidated sessions in the
    // collection
    assert_eq!(sessions.get_random_anycast_session(), None);

    // Consolidate outbound session
    assert!(sessions
        .consolidate_session(SessionType::Outbound, outbound_address)
        .is_ok());

    // Check that the function returns Some(T) when there is one valid session in the collection
    assert_eq!(
        sessions.get_random_anycast_session(),
        Some("reference1".to_string())
    );

    // Register and consolidate an outbound session and check if result is Ok(())
    let outbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Outbound,
            outbound_address,
            "reference2".to_string()
        )
        .is_ok());
    assert!(sessions
        .consolidate_session(SessionType::Outbound, outbound_address)
        .is_ok());

    // Get random session for a "big" number
    let mut diff: i16 = 0;
    for _ in 0..100000 {
        // Get a random anycast sessions (there are only 2)
        match &sessions.get_random_anycast_session() {
            Some(reference) if reference == "reference1" => diff = diff + 1,
            Some(reference) if reference == "reference2" => diff = diff - 1,
            _ => assert!(
                false,
                "Get random function should retrieve a random session"
            ),
        }
    }

    // Check both sessions were retrieved equally
    // Acceptance criteria for randomness is 1%
    assert!(
        diff < 1000 && diff > -1000,
        "Get random seems not to be following a uniform distribution"
    );
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

    // Check if outbound session was registered successfully into the unconsolidated sessions
    assert!(sessions
        .outbound_unconsolidated
        .collection
        .contains_key(&outbound_address));

    // Check if no sessions was registered into the consolidated sessions
    assert_eq!(sessions.outbound_consolidated.collection.len(), 0);

    // Register an inbound session and check if result is Ok(())
    let inbound_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8002);
    assert!(sessions
        .register_session(
            SessionType::Inbound,
            inbound_address,
            "reference2".to_string()
        )
        .is_ok());

    // Check if inbound session was registered successfully
    assert!(sessions.inbound.collection.contains_key(&inbound_address));
}

/// Check the unregistration of sessions
#[test]
fn p2p_sessions_unregister() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Register sessions and check if result is Ok(())
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
        .unregister_session(
            SessionType::Outbound,
            SessionStatus::Unconsolidated,
            outbound_address
        )
        .is_ok());
    assert!(sessions
        .unregister_session(
            SessionType::Inbound,
            SessionStatus::Unconsolidated,
            inbound_address
        )
        .is_ok());

    // Check that both sessions are removed from collections
    assert_eq!(sessions.outbound_unconsolidated.collection.len(), 0);
    assert_eq!(sessions.inbound.collection.len(), 0);
}

/// Check the consolidation of sessions
#[test]
fn p2p_sessions_consolidate() {
    // Create sessions struct
    let mut sessions = Sessions::<String>::default();

    // Register sessions and check if result is Ok(())
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
        .outbound_unconsolidated
        .collection
        .get(&outbound_address)
        .is_some());
    assert_eq!(sessions.outbound_consolidated.collection.len(), 0);
    assert!(sessions.inbound.collection.get(&inbound_address).is_some());

    // Consolidate session
    assert!(sessions
        .consolidate_session(SessionType::Outbound, outbound_address)
        .is_ok());
    assert!(sessions
        .consolidate_session(SessionType::Inbound, inbound_address)
        .is_ok());

    // Check if sessions were consolidated
    assert!(sessions
        .outbound_consolidated
        .collection
        .get(&outbound_address)
        .is_some());
    assert_eq!(sessions.outbound_unconsolidated.collection.len(), 0);
    assert!(sessions.inbound.collection.get(&inbound_address).is_some());
}
