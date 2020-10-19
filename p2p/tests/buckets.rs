use std::net::SocketAddr;

use witnet_p2p::peers::{
    calculate_index_for_new, calculate_index_for_tried, split_socket_addresses,
};

/// Tests for the business logic of inserting and removing peer addresses into the `ice` bucket.
mod ice {
    use super::ip;
    use std::time::Duration;
    use witnet_p2p::peers::Peers;

    #[test]
    fn test_can_ice_an_address() {
        let ice_period = Duration::from_secs(1000);
        let mut peers = Peers {
            ice_period,
            ..Default::default()
        };
        let address = ip("192.168.1.1:21337");
        let can_ice = peers.ice_peer_address(&address);

        assert!(can_ice);

        let is_iced = peers.ice_bucket_contains(&address);

        assert!(is_iced)
    }

    #[test]
    fn test_icing_does_not_block_the_entire_ip() {
        let mut peers = Peers::default();
        let address_21337 = ip("192.168.1.1:21337");
        let address_21338 = ip("192.168.1.1:21338");

        peers.ice_peer_address(&address_21337);
        let is_iced = peers.ice_bucket_contains(&address_21338);

        assert!(!is_iced)
    }

    #[test]
    fn test_icing_does_not_block_different_ip() {
        let mut peers = Peers::default();
        let address_1 = ip("192.168.1.1:21337");
        let address_2 = ip("192.168.1.2:21337");

        peers.ice_peer_address(&address_1);
        let is_iced = peers.ice_bucket_contains(&address_2);

        assert!(!is_iced)
    }

    #[test]
    // FIXME(#1646): Remove this ignore
    #[ignore]
    fn test_ice_melts_ater_some_time() {
        let ice_period = Duration::from_secs(1000);
        let mut peers = Peers {
            bootstrapped: true,
            ice_period,
            ..Default::default()
        };
        let address = ip("192.168.1.1:21337");
        peers.ice_peer_address_pure(&address, 0);
        let is_iced_right_after = peers.ice_bucket_contains_pure(&address, 1);
        let is_still_iced_right_before_ice_period_is_over =
            peers.ice_bucket_contains_pure(&address, 999);
        let is_still_iced_just_when_ice_period_is_over =
            peers.ice_bucket_contains_pure(&address, 1000);
        let is_iced_right_after_ice_period_is_over = peers.ice_bucket_contains_pure(&address, 1001);

        assert!(is_iced_right_after);
        assert!(is_still_iced_right_before_ice_period_is_over);
        assert!(is_still_iced_just_when_ice_period_is_over);
        assert!(!is_iced_right_after_ice_period_is_over);
    }

    #[test]
    fn test_remove_iced_address() {
        let ice_period = Duration::from_secs(1000);
        let mut peers = Peers {
            ice_period,
            ..Default::default()
        };
        let address = ip("192.168.1.1:21337");
        let can_ice = peers.ice_peer_address(&address);

        assert!(can_ice);

        let mut is_iced = peers.ice_bucket_contains(&address);

        assert!(is_iced);

        peers.remove_from_ice(&address);

        is_iced = peers.ice_bucket_contains(&address);

        assert!(!is_iced);
    }

    #[test]
    fn test_remove_iced_addresses() {
        let ice_period = Duration::from_secs(1000);
        let mut peers = Peers {
            ice_period,
            ..Default::default()
        };
        let address_1 = ip("192.168.1.1:21337");
        let address_2 = ip("192.168.2.2:21337");

        peers.ice_peer_address(&address_1);
        peers.ice_peer_address(&address_2);

        let mut is_iced = peers.ice_bucket_contains(&address_1);

        assert!(is_iced);

        is_iced = peers.ice_bucket_contains(&address_2);

        assert!(is_iced);

        peers.remove_many_from_ice(&[address_1, address_2]);

        is_iced = peers.ice_bucket_contains(&address_1);

        assert!(!is_iced);

        is_iced = peers.ice_bucket_contains(&address_2);

        assert!(!is_iced);
    }

    #[test]
    fn test_ice_address_non_existent_in_tried() {
        let ice_period = Duration::from_secs(1000);
        let mut peers = Peers {
            ice_period,
            ..Default::default()
        };
        let address = ip("192.168.1.1:21337");

        peers.clear_tried_bucket();
        peers.remove_from_tried(&[address], true);

        let is_iced = peers.ice_bucket_contains(&address);

        assert!(is_iced);
    }
    #[test]
    fn test_not_ice_address_non_existent_in_tried() {
        let ice_period = Duration::from_secs(1000);
        let mut peers = Peers {
            ice_period,
            ..Default::default()
        };
        let address = ip("192.168.1.1:21337");

        peers.clear_tried_bucket();
        peers.remove_from_tried(&[address], false);

        let is_iced = peers.ice_bucket_contains(&address);

        assert!(!is_iced);
    }
}

/// Tests for the business logic of inserting peer addresses into the `new` buckets.
mod new {
    use super::{ip, new_bucket_index};

    #[test]
    fn test_same_peer_ip_different_peer_port_same_new_bucket_index() {
        let sk = 0;
        let src_addr = ip("127.0.0.1:21337");
        let peer_addr_21337 = ip("192.168.1.1:21337");
        let peer_addr_21338 = ip("192.168.1.1:21338");

        let new_index_21337 = new_bucket_index(sk, &peer_addr_21337, &src_addr);
        let new_index_21338 = new_bucket_index(sk, &peer_addr_21338, &src_addr);

        assert_eq!(new_index_21337, new_index_21338);
    }

    #[test]
    fn test_close_peer_ip_same_new_bucket_different_index() {
        let sk = 0;
        let src_addr = ip("127.0.0.1:21337");
        let peer_addr_1_1 = ip("192.168.1.1:21337");
        let peer_addr_2_1 = ip("192.168.2.1:21337");

        let new_index_1_1 = new_bucket_index(sk, &peer_addr_1_1, &src_addr);
        let new_index_2_1 = new_bucket_index(sk, &peer_addr_2_1, &src_addr);

        assert_ne!(new_index_1_1, new_index_2_1);
        assert!((f64::from(new_index_1_1) - f64::from(new_index_2_1)).abs() < 64.0);
    }

    #[test]
    fn test_same_peer_address_same_source_ip_different_source_port_same_new_index() {
        let sk = 0;
        let src_addr_21337 = ip("127.0.0.1:21337");
        let src_addr_21338 = ip("127.0.0.1:21338");
        let peer_addr = ip("192.168.1.1:21337");

        let new_index_1_1 = new_bucket_index(sk, &peer_addr, &src_addr_21337);
        let new_index_2_1 = new_bucket_index(sk, &peer_addr, &src_addr_21338);

        assert_eq!(new_index_1_1, new_index_2_1);
    }

    #[test]
    fn test_same_peer_address_close_source_ip_same_source_port_same_new_bucket_same_index() {
        let sk = 0;
        let src_addr_0_1 = ip("127.0.0.1:21337");
        let src_addr_1_1 = ip("127.0.1.1:21337");
        let peer_addr = ip("192.168.1.1:21337");

        let new_index_0_1 = new_bucket_index(sk, &peer_addr, &src_addr_0_1);
        let new_index_1_1 = new_bucket_index(sk, &peer_addr, &src_addr_1_1);

        assert_eq!(new_index_0_1, new_index_1_1);
    }

    #[test]
    fn test_same_peer_address_different_source_ip_same_source_port_different_new_bucket() {
        let sk = 0;
        let src_addr_0_0_1 = ip("127.0.0.1:21337");
        let src_addr_1_0_1 = ip("127.1.0.1:21337");
        let peer_addr = ip("192.168.1.1:21337");

        let new_index_0_0_1 = new_bucket_index(sk, &peer_addr, &src_addr_0_0_1);
        let new_index_1_0_1 = new_bucket_index(sk, &peer_addr, &src_addr_1_0_1);

        assert_ne!(new_index_0_0_1, new_index_1_0_1);
    }

    #[test]
    fn test_different_sk_different_bucket() {
        let sk_1 = 1;
        let sk_2 = 2;
        let src_addr = ip("127.0.0.1:21337");
        let peer_addr = ip("192.168.1.1:21337");

        let new_index_sk_1 = new_bucket_index(sk_1, &peer_addr, &src_addr);
        let new_index_sk_2 = new_bucket_index(sk_2, &peer_addr, &src_addr);

        assert_ne!(new_index_sk_1, new_index_sk_2);
    }
}

/// Tests for the business logic of inserting peer addresses into the `tried` buckets.
mod tried {
    use super::{ip, tried_bucket_index};

    #[test]
    fn test_same_peer_ip_different_peer_port_same_tried_bucket_different_index() {
        let sk = 0;
        let peer_addr_21337 = ip("192.168.1.1:21337");
        let peer_addr_21338 = ip("192.168.1.1:21338");

        let new_index_21337 = tried_bucket_index(sk, &peer_addr_21337);
        let new_index_21338 = tried_bucket_index(sk, &peer_addr_21338);

        assert_ne!(new_index_21337, new_index_21338);
        assert!((f64::from(new_index_21337) - f64::from(new_index_21338)).abs() < 64.0);
    }

    #[test]
    fn test_close_peer_ip_same_tried_bucket_different_index() {
        let sk = 0;
        let peer_addr_1_1 = ip("192.168.1.1:21337");
        let peer_addr_1_2 = ip("192.168.1.2:21337");

        let new_index_1_1 = tried_bucket_index(sk, &peer_addr_1_1);
        let new_index_1_2 = tried_bucket_index(sk, &peer_addr_1_2);

        assert_ne!(new_index_1_1, new_index_1_2);
        assert!((f64::from(new_index_1_1) - f64::from(new_index_1_2)).abs() < 64.0);
    }

    #[test]
    fn test_slightly_far_peer_ip_different_tried_bucket_different_index() {
        let sk = 0;
        let peer_addr_1_1 = ip("192.168.1.1:21337");
        let peer_addr_2_1 = ip("192.168.2.1:21337");

        let new_index_1_1 = tried_bucket_index(sk, &peer_addr_1_1);
        let new_index_2_1 = tried_bucket_index(sk, &peer_addr_2_1);

        assert_ne!(new_index_1_1, new_index_2_1);
        assert!((f64::from(new_index_1_1) - f64::from(new_index_2_1)).abs() < 1024.0);
    }

    #[test]
    fn test_much_far_peer_ip_different_tried_bucket_different_index() {
        let sk = 0;
        let peer_addr_168 = ip("192.168.1.1:21337");
        let peer_addr_169 = ip("192.169.1.1:21337");

        let new_index_168 = tried_bucket_index(sk, &peer_addr_168);
        let new_index_169 = tried_bucket_index(sk, &peer_addr_169);

        assert_ne!(new_index_168, new_index_169);
    }

    #[test]
    fn test_different_sk_different_bucket() {
        let sk_1 = 1;
        let sk_2 = 2;
        let peer_addr = ip("192.168.1.1:21337");

        let new_index_sk_1 = tried_bucket_index(sk_1, &peer_addr);
        let new_index_sk_2 = tried_bucket_index(sk_2, &peer_addr);

        assert_ne!(new_index_sk_1, new_index_sk_2);
    }
}

fn new_bucket_index(sk: u64, socket_addr: &SocketAddr, src_socket_addr: &SocketAddr) -> u16 {
    let (_, group, host_id) = split_socket_addresses(socket_addr);
    let (_, src_group, _) = split_socket_addresses(src_socket_addr);

    calculate_index_for_new(sk, &src_group, &group, &host_id)
}

fn tried_bucket_index(sk: u64, socket_addr: &SocketAddr) -> u16 {
    let (ip, group, host_id) = split_socket_addresses(socket_addr);

    calculate_index_for_tried(sk, &ip, &group, &host_id)
}

fn ip(string: &str) -> SocketAddr {
    use std::str::FromStr;

    SocketAddr::from_str(string).unwrap()
}
