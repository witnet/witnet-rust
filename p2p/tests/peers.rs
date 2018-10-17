use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use witnet_p2p::peers::*;

#[test]
fn p2p_peers_add() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    peers.add(address).unwrap();

    // Get a random address (there is only 1)
    let result = peers.get_random();

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), Some(address));
}

#[test]
fn p2p_peers_remove() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    peers.add(address).unwrap();

    // Remove address
    peers.remove(address).unwrap();

    // Get a random address
    let result = peers.get_random();

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), None);
}

#[test]
fn p2p_peers_get_random() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add addresses
    let address1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let address2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8080);
    peers.add(address1).unwrap();
    peers.add(address2).unwrap();

    // Get random address for a "big" number
    let mut diff: i16 = 0;
    for _ in 0..100000 {
        // Get a random address (there is only 1)
        match peers.get_random().unwrap() {
            Some(addr) if addr == address1 => diff = diff + 1,
            Some(addr) if addr == address2 => diff = diff - 1,
            _ => assert!(
                false,
                "Get random function should retrieve a random address"
            ),
        }
    }

    // Check that both addresses are the same
    // Acceptance criteria for randomness is 1%
    assert!(
        diff < 1000 && diff > -1000,
        "Get random seems not to be following a uniform distribution"
    );
}
