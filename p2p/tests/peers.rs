use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use witnet_p2p::peers::*;

#[test]
fn p2p_peers_add_to_new() {
    // Create peers struct
    let mut peers = Peers::default();
    let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 20)), 8080);
    peers.set_server(server);

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let src_address = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
        8080,
    ));

    assert_eq!(
        peers.add_to_new(vec![address], src_address).unwrap(),
        vec![]
    );
    // If we add the same address again, the method returns it
    assert_eq!(
        peers.add_to_new(vec![address], src_address).unwrap(),
        vec![address]
    );

    // Get a random address (there is only 1)
    let result = peers.get_random_peers(1);

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), vec![address]);

    // There is only 1 address
    assert_eq!(peers.get_all_from_new().unwrap(), vec![address]);
}

#[test]
fn p2p_peers_add_to_tried() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

    assert_eq!(peers.add_to_tried(address).unwrap(), None);
    // If we add the same address again, the method returns it
    assert_eq!(peers.add_to_tried(address).unwrap(), Some(address));

    // Get a random address (there is only 1)
    let result = peers.get_random_peers(1);

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), vec![address]);

    // There is only 1 address
    assert_eq!(peers.get_all_from_tried().unwrap(), vec![address]);
}

#[test]
fn p2p_peers_random() {
    // Create peers struct
    let mut peers = Peers::default();
    let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 20)), 8080);
    peers.set_server(server);

    // Add addresses
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let src_address = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
        8080,
    ));
    let address2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3)), 8080);

    peers.add_to_new(vec![address], src_address).unwrap();
    peers.add_to_tried(address2).unwrap();

    // Get 2 random address (there is only 2)
    let result = peers.get_random_peers(2);

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), vec![address, address2]);

    assert_eq!(peers.get_all_from_new().unwrap(), vec![address]);
    assert_eq!(peers.get_all_from_tried().unwrap(), vec![address2]);
}

#[test]
fn p2p_peers_random_less_than_in_tried() {
    // Create peers struct
    let mut peers = Peers::default();
    let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 20)), 8080);
    peers.set_server(server);

    // Add addresses
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let address2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3)), 8080);

    peers.add_to_tried(address).unwrap();
    peers.add_to_tried(address2).unwrap();

    // Get 1 random address when there are 2 in tried
    let result = peers.get_random_peers(1).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn p2p_peers_remove_from_tried() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    peers.add_to_tried(address).unwrap();

    // Remove address
    assert_eq!(peers.remove_from_tried(&[address], false), vec![address]);

    // Get a random address
    let result = peers.get_random_peers(1);

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), vec![]);

    // Remove the same address twice doesn't panic
    assert_eq!(peers.remove_from_tried(&[address, address], false), vec![]);
}

#[test]
fn p2p_peers_remove_from_new_with_index() {
    // Create peers struct
    let mut peers = Peers::default();
    let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 20)), 8080);
    peers.set_server(server);

    // Add address
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    let src_address = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(168, 0, 0, 12)),
        8080,
    ));
    peers.add_to_new(vec![address], src_address).unwrap();

    let index = peers.new_bucket_index(&address, &src_address.unwrap());

    // Remove address
    assert_eq!(peers.remove_from_new_with_index(&[index]), vec![address]);

    // Get a random address
    let result = peers.get_random_peers(1);

    // Check that both addresses are the same
    assert_eq!(result.unwrap(), vec![]);

    // Remove the same address twice doesn't panic
    assert_eq!(peers.remove_from_new_with_index(&[index]), vec![]);
}

#[test]
fn p2p_peers_get_all_from_new() {
    // Create peers struct
    let mut peers = Peers::default();
    let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 2, 20)), 8080);
    peers.set_server(server);

    // Add 100 addresses
    let many_peers: Vec<_> = (0..100)
        .map(|i| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, i)), 8080))
        .collect();
    let src_address = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(168, 0, 0, 12)),
        8080,
    ));
    peers.add_to_new(many_peers, src_address).unwrap();

    assert!(!peers.get_all_from_new().unwrap().is_empty());
    assert!(peers.get_all_from_tried().unwrap().is_empty());
}

#[test]
fn p2p_peers_get_all_from_tried() {
    // Create peers struct
    let mut peers = Peers::default();

    // Add 100 addresses
    for i in 0..100 {
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, i)), 8080);
        peers.add_to_tried(address).unwrap();
    }

    assert!(peers.get_all_from_new().unwrap().is_empty());
    assert!(!peers.get_all_from_tried().unwrap().is_empty());
}

#[test]
fn p2p_add_2_peers_in_collision() {
    // Create peers struct
    let mut peers = Peers::default();

    let peer1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21305);
    let peer2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21306);
    peers.add_to_tried(peer1).unwrap();
    peers.add_to_tried(peer2).unwrap();

    assert_eq!(peers.get_all_from_tried().unwrap().len(), 1);
}
