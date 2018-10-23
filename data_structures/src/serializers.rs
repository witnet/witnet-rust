use std::convert::{TryFrom, Into};
extern crate flatbuffers;

use crate::flatbuffers::protocol_generated::protocol::{
    get_root_as_message, Address as FlatBufferAddress, AddressArgs, Command as FlatBuffersCommand,
    GetPeers as FlatBuffersGetPeers, GetPeersArgs, IpAddress as FlatBuffersIpAddress,
    Ipv4 as FlatBuffersIpv4, Ipv4Args as FlatBuffersIpv4Args, Ipv6 as FlatBuffersIpv6,
    Ipv6Args as FlatBuffersIpv6Args, Message as FlatBuffersMessage, MessageArgs,
    Peers as FlatBuffersPeers, PeersArgs, Ping as FlatBuffersPing, PingArgs,
    Pong as FlatBuffersPong, PongArgs, Verack as FlatBuffersVerack, VerackArgs,
    Version as FlatBuffersVersion, VersionArgs,
};
use crate::types::{
    Address, Command,
    IpAddress::{Ipv4 as WitnetIpv4, Ipv6 as WitnetIpv6},
    Message,
};

use flatbuffers::FlatBufferBuilder;

const FTB_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy)]
struct GetPeersFlatbufferArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct GetPeersWitnetArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct PeersFlatbufferArgs<'a> {
    magic: u16,
    peers: &'a [Address],
}

#[derive(Debug, Clone, Copy)]
struct PeersWitnetArgs<'a> {
    magic: u16,
    peers: FlatBuffersPeers<'a>,
}

#[derive(Debug, Clone, Copy)]
struct PingFlatbufferArgs {
    magic: u16,
    nonce: u64,
}

#[derive(Debug, Clone, Copy)]
struct PingWitnetArgs {
    nonce: u64,
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct PongFlatbufferArgs {
    magic: u16,
    nonce: u64,
}

#[derive(Debug, Clone, Copy)]
struct PongWitnetArgs {
    nonce: u64,
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct VerackFlatbufferArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct VerackWitnetArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct VersionFlatbufferArgs<'a> {
    capabilities: u64,
    genesis: u64,
    last_epoch: u32,
    magic: u16,
    nonce: u64,
    receiver_address: &'a Address,
    sender_address: &'a Address,
    timestamp: u64,
    user_agent: &'a str,
    version: u32,
}

#[derive(Debug, Clone)]
struct VersionWitnetArgs {
    capabilities: u64,
    genesis: u64,
    last_epoch: u32,
    magic: u16,
    nonce: u64,
    receiver_address: Address,
    sender_address: Address,
    timestamp: u64,
    user_agent: String,
    version: u32,
}

impl TryFrom<Vec<u8>> for Message {
    type Error = &'static str;
    // type Error = Err<&'static str>;
    fn try_from(bytes: Vec<u8>) -> Result<Self, &'static str> {
        // Get Flatbuffers Message
        let message = get_root_as_message(&bytes);
        let magic = message.magic();

        // Create witnet's message to decode a flatbuffer message
        match message.command_type() {
            FlatBuffersCommand::Ping => message.command_as_ping().map(|ping| {
                create_ping_message(PingWitnetArgs {
                    nonce: ping.nonce(),
                    magic,
                })
            }).ok_or(""),
            FlatBuffersCommand::Pong => message.command_as_pong().map(|pong| {
                create_pong_message(PongWitnetArgs {
                    nonce: pong.nonce(),
                    magic,
                })
            }).ok_or(""),
            FlatBuffersCommand::GetPeers => {
                Ok(create_get_peers_message(GetPeersWitnetArgs { magic }))
            }
            FlatBuffersCommand::Peers => message
                .command_as_peers()
                .and_then(|peers| create_peers_message(PeersWitnetArgs { magic, peers })).ok_or(""),
            FlatBuffersCommand::Verack => Ok(create_verack_message(VerackWitnetArgs { magic })),
            FlatBuffersCommand::Version => message.command_as_version().and_then(|command| {
                // Get ftb addresses and create witnet addresses
                let sender_address = command.sender_address().and_then(create_address);
                let receiver_address = command.receiver_address().and_then(create_address);
                // Check if sender address and receiver address exist
                if sender_address.and(receiver_address).is_some() {
                    Some(create_version_message(VersionWitnetArgs {
                        version: command.version(),
                        timestamp: command.timestamp(),
                        capabilities: command.capabilities(),
                        sender_address: sender_address.unwrap(),
                        receiver_address: receiver_address.unwrap(),
                        // FIXME(#65): user_agent field should be required as specified in ftb schema
                        user_agent: command.user_agent().unwrap_or("").to_string(),
                        last_epoch: command.last_epoch(),
                        genesis: command.genesis(),
                        nonce: command.nonce(),
                        magic,
                    }))
                } else {
                    None
                }
            }).ok_or(""),
            FlatBuffersCommand::NONE => Err(""),
        }
    }
}

impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        let mut builder = flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);
        // Create flatbuffer to encode a witnet message
        match &self.kind {
            Command::GetPeers => create_get_peers_flatbuffer(
                &mut builder,
                GetPeersFlatbufferArgs { magic: self.magic },
            ),
            Command::Peers { peers } => create_peers_flatbuffer(
                &mut builder,
                PeersFlatbufferArgs {
                    magic: self.magic,
                    peers,
                },
            ),
            Command::Ping { nonce } => create_ping_flatbuffer(
                &mut builder,
                PingFlatbufferArgs {
                    magic: self.magic,
                    nonce: *nonce,
                },
            ),
            Command::Pong { nonce } => create_pong_flatbuffer(
                &mut builder,
                PongFlatbufferArgs {
                    magic: self.magic,
                    nonce: *nonce,
                },
            ),
            Command::Verack => {
                create_verack_flatbuffer(&mut builder, VerackFlatbufferArgs { magic: self.magic })
            }
            Command::Version {
                version,
                timestamp,
                capabilities,
                sender_address,
                receiver_address,
                user_agent,
                last_epoch,
                genesis,
                nonce,
            } => create_version_flatbuffer(
                &mut builder,
                VersionFlatbufferArgs {
                    magic: self.magic,
                    version: *version,
                    timestamp: *timestamp,
                    capabilities: *capabilities,
                    sender_address,
                    receiver_address,
                    user_agent,
                    last_epoch: *last_epoch,
                    genesis: *genesis,
                    nonce: *nonce,
                },
            ),
        }
    }
}

// Encode a flatbuffer from a flatbuffers message
fn build_flatbuffer(
    builder: &mut FlatBufferBuilder,
    message: flatbuffers::WIPOffset<FlatBuffersMessage>,
) -> Vec<u8> {
    builder.finish(message, None);
    builder.finished_data().to_vec()
}

// Create witnet ip address
fn create_address(address: FlatBufferAddress) -> Option<Address> {
    match address.ip_type() {
        FlatBuffersIpAddress::Ipv4 => address
            .ip_as_ipv_4()
            .map(|ipv4| create_ipv4_address(ipv4.ip(), address.port())),
        FlatBuffersIpAddress::Ipv6 => match address.ip_as_ipv_6() {
            Some(hextets) => Some(create_ipv6_address(
                hextets.ip0(),
                hextets.ip1(),
                hextets.ip2(),
                hextets.ip3(),
                address.port(),
            )),
            None => None,
        },
        FlatBuffersIpAddress::NONE => None,
    }
}

// Create a get peers flatbuffer to encode a witnet's get peers message
fn create_get_peers_flatbuffer(
    builder: &mut FlatBufferBuilder,
    get_peers_args: GetPeersFlatbufferArgs,
) -> Vec<u8> {
    let get_peers_command = FlatBuffersGetPeers::create(builder, &GetPeersArgs {});

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: get_peers_args.magic,
            command_type: FlatBuffersCommand::GetPeers,
            command: Some(get_peers_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a witnet's get peers message to decode a flatbuffers' get peers message
fn create_get_peers_message(get_peers_args: GetPeersWitnetArgs) -> Message {
    Message {
        kind: Command::GetPeers,
        magic: get_peers_args.magic,
    }
}

// Create witnet ipv4 address
fn create_ipv4_address(ip: u32, port: u16) -> Address {
    Address {
        ip: WitnetIpv4 { ip },
        port,
    }
}

// Create witnet ipv6 address
fn create_ipv6_address(ip0: u32, ip1: u32, ip2: u32, ip3: u32, port: u16) -> Address {
    Address {
        ip: WitnetIpv6 { ip0, ip1, ip2, ip3 },
        port,
    }
}

// Create a peers flatbuffer to encode a witnet's peers message
fn create_peers_flatbuffer(
    builder: &mut FlatBufferBuilder,
    peers_args: PeersFlatbufferArgs,
) -> Vec<u8> {
    let addresses_command: Vec<flatbuffers::WIPOffset<FlatBufferAddress>> = peers_args
        .peers
        .iter()
        .map(|peer: &Address| match peer.ip {
            WitnetIpv4 { ip } => {
                let ip_command = FlatBuffersIpv4::create(builder, &FlatBuffersIpv4Args { ip });
                FlatBufferAddress::create(
                    builder,
                    &AddressArgs {
                        ip_type: FlatBuffersIpAddress::Ipv4,
                        ip: Some(ip_command.as_union_value()),
                        port: peer.port,
                    },
                )
            }
            WitnetIpv6 { ip0, ip1, ip2, ip3 } => {
                let ip_command =
                    FlatBuffersIpv6::create(builder, &FlatBuffersIpv6Args { ip0, ip1, ip2, ip3 });
                FlatBufferAddress::create(
                    builder,
                    &AddressArgs {
                        ip_type: FlatBuffersIpAddress::Ipv6,
                        ip: Some(ip_command.as_union_value()),
                        port: peer.port,
                    },
                )
            }
        })
        .collect();

    let addresses = Some(builder.create_vector(&addresses_command));
    let peers_command = FlatBuffersPeers::create(builder, &PeersArgs { peers: addresses });

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: peers_args.magic,
            command_type: FlatBuffersCommand::Peers,
            command: Some(peers_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a witnet's peers message to decode a flatbuffers' peers message
fn create_peers_message(peers_args: PeersWitnetArgs) -> Option<Message> {
    peers_args.peers.peers().map(|ftb_addresses| {
        // TODO: Refactor as declarative code [24-10-2018]
        let len = ftb_addresses.len();
        let mut counter = 0;
        let mut ftb_address: Option<Address>;
        let mut peer;
        let mut vec_addresses = Vec::new();
        while counter < len {
            peer = ftb_addresses.get(counter);
            ftb_address = create_address(peer);
            if ftb_address.is_some() {
                vec_addresses.push(ftb_address.unwrap());
            }
            counter += 1;
        }
        Message {
            kind: Command::Peers {
                peers: vec_addresses,
            },
            magic: peers_args.magic,
        }
    })
}

// Create a ping flatbuffer to encode a witnet's ping message
fn create_ping_flatbuffer(
    builder: &mut FlatBufferBuilder,
    ping_args: PingFlatbufferArgs,
) -> Vec<u8> {
    let ping_command = FlatBuffersPing::create(
        builder,
        &PingArgs {
            nonce: ping_args.nonce.to_owned(),
        },
    );
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: ping_args.magic,
            command_type: FlatBuffersCommand::Ping,
            command: Some(ping_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a witnet's ping message to decode a flatbuffers' ping message
fn create_ping_message(ping_args: PingWitnetArgs) -> Message {
    Message {
        kind: Command::Ping {
            nonce: ping_args.nonce,
        },
        magic: ping_args.magic,
    }
}

// Create a pong flatbuffer to encode a witnet's pong message
fn create_pong_flatbuffer(
    builder: &mut FlatBufferBuilder,
    pong_args: PongFlatbufferArgs,
) -> Vec<u8> {
    let pong_command = FlatBuffersPong::create(
        builder,
        &PongArgs {
            nonce: pong_args.nonce,
        },
    );
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: pong_args.magic,
            command_type: FlatBuffersCommand::Pong,
            command: Some(pong_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a witnet's pong message to decode a flatbuffers' pong message
fn create_pong_message(pong_args: PongWitnetArgs) -> Message {
    Message {
        kind: Command::Pong {
            nonce: pong_args.nonce,
        },
        magic: pong_args.magic,
    }
}

// Create a verack flatbuffer to encode a witnet's verack message
fn create_verack_flatbuffer(
    builder: &mut FlatBufferBuilder,
    verack_args: VerackFlatbufferArgs,
) -> Vec<u8> {
    let verack_command = FlatBuffersVerack::create(builder, &VerackArgs {});

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: verack_args.magic,
            command_type: FlatBuffersCommand::Verack,
            command: Some(verack_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a witnet's verack message to decode a flatbuffers' verack message
fn create_verack_message(verack_args: VerackWitnetArgs) -> Message {
    Message {
        kind: Command::Verack,
        magic: verack_args.magic,
    }
}

// Create a version flatbuffer to encode a witnet's version message
fn create_version_flatbuffer(
    builder: &mut FlatBufferBuilder,
    version_args: VersionFlatbufferArgs,
) -> Vec<u8> {
    let sender_address_command = match version_args.sender_address.ip {
        WitnetIpv4 { ip } => {
            let ip_command = FlatBuffersIpv4::create(builder, &FlatBuffersIpv4Args { ip });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv4,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.sender_address.port,
                },
            )
        }
        WitnetIpv6 { ip0, ip1, ip2, ip3 } => {
            let ip_command =
                FlatBuffersIpv6::create(builder, &FlatBuffersIpv6Args { ip0, ip1, ip2, ip3 });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv6,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.sender_address.port,
                },
            )
        }
    };

    let receiver_address_command = match version_args.receiver_address.ip {
        WitnetIpv4 { ip } => {
            let ip_command = FlatBuffersIpv4::create(builder, &FlatBuffersIpv4Args { ip });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv4,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.receiver_address.port,
                },
            )
        }
        WitnetIpv6 { ip0, ip1, ip2, ip3 } => {
            let ip_command =
                FlatBuffersIpv6::create(builder, &FlatBuffersIpv6Args { ip0, ip1, ip2, ip3 });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv6,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.receiver_address.port,
                },
            )
        }
    };

    let user_agent = Some(builder.create_string(&version_args.user_agent));
    let version_command = FlatBuffersVersion::create(
        builder,
        &VersionArgs {
            version: version_args.version,
            timestamp: version_args.timestamp,
            capabilities: version_args.capabilities,
            sender_address: Some(sender_address_command),
            receiver_address: Some(receiver_address_command),
            user_agent,
            last_epoch: version_args.last_epoch,
            genesis: version_args.genesis,
            nonce: version_args.nonce,
        },
    );

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: version_args.magic,
            command_type: FlatBuffersCommand::Version,
            command: Some(version_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a witnet's version message to decode a flatbuffers' version message
fn create_version_message(version_args: VersionWitnetArgs) -> Message {
    Message {
        kind: Command::Version {
            version: version_args.version,
            timestamp: version_args.timestamp,
            capabilities: version_args.capabilities,
            sender_address: version_args.sender_address,
            receiver_address: version_args.receiver_address,
            user_agent: version_args.user_agent,
            last_epoch: version_args.last_epoch,
            genesis: version_args.genesis,
            nonce: version_args.nonce,
        },
        magic: version_args.magic,
    }
}
