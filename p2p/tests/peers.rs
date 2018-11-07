use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use witnet_p2p::peers::*;

#[test]
fn p2p_peers_add() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

    assert_eq!(peers.add(vec![address]).unwrap(), vec![]);
    // If we add the same address again, the method returns it
    assert_eq!(peers.add(vec![address]).unwrap(), vec![address]);

    // Get a random address (there is only 1)
    let result = peers.get_random();

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), Some(address));

    // There is only 1 address
    assert_eq!(peers.get_all().unwrap(), vec![address]);

    // Add 100 addresses more
    let many_peers = (0..100)
        .map(|i| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 1, i)), 8080))
        .collect();
    peers.add(many_peers).unwrap();

    assert_eq!(peers.get_all().unwrap().len(), 1 + 100);
}

#[test]
fn p2p_peers_remove() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    peers.add(vec![address]).unwrap();

    // Remove address
    assert_eq!(peers.remove(&[address]).unwrap(), vec![address]);

    // Get a random address
    let result = peers.get_random();

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), None);

    // Remove the same address twice doesn't panic
    assert_eq!(peers.remove(&[address, address]).unwrap(), vec![]);
}

#[test]
fn p2p_peers_get_random() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add addresses
    let address1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let address2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8080);
    peers.add(vec![address1, address2]).unwrap();

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

#[test]
fn p2p_peers_get_all() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add 100 addresses
    let mut many_peers: Vec<_> = (0..100)
        .map(|i| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, i)), 8080))
        .collect();
    peers.add(many_peers.clone()).unwrap();

    // There are 100 peers in total
    assert_eq!(peers.get_all().unwrap().len(), 100);

    let mut added_peers = peers.get_all().unwrap();

    // Check that all peers were added
    // We need to sort the vectors first
    let sort_by_ip_then_port =
        |a: &SocketAddr, b: &SocketAddr| (a.ip(), a.port()).cmp(&(b.ip(), b.port()));
    many_peers.sort_by(sort_by_ip_then_port);
    added_peers.sort_by(sort_by_ip_then_port);
    assert_eq!(many_peers, added_peers);
}
