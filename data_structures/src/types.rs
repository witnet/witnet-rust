use std::fmt;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Command {
    GetPeers(GetPeers),
    Peers(Peers),
    Ping(Ping),
    Pong(Pong),
    Verack(Verack),
    Version(Version),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GetPeers;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Peers {
    pub peers: Vec<Address>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Ping {
    pub nonce: u64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Pong {
    pub nonce: u64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Verack;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Version {
    pub version: u32,
    pub timestamp: i64,
    pub capabilities: u64,
    pub sender_address: Address,
    pub receiver_address: Address,
    pub user_agent: String,
    pub last_epoch: u32,
    pub genesis: u64,
    pub nonce: u64,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Command::GetPeers(_) => "GET_PEERS",
                Command::Peers(_) => "PEERS",
                Command::Ping(_) => "PING",
                Command::Pong(_) => "PONG",
                Command::Verack(_) => "VERACK",
                Command::Version(_) => "VERSION",
            }
        )
    }
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
