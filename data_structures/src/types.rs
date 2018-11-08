#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Command {
    GetPeers,
    Peers {
        peers: Vec<Address>,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    Verack,
    Version {
        version: u32,
        timestamp: i64,
        capabilities: u64,
        sender_address: Address,
        receiver_address: Address,
        user_agent: String,
        last_epoch: u32,
        genesis: u64,
        nonce: u64,
    },
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum IpAddress {
    Ipv4 {
        ip: u32,
    },
    Ipv6 {
        ip0: u32,
        ip1: u32,
        ip2: u32,
        ip3: u32,
    },
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Address {
    pub ip: IpAddress,
    pub port: u16,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Message {
    pub kind: Command,
    pub magic: u16,
}
