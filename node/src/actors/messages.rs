use std::{
    collections::HashMap,
    fmt,
    fmt::Debug,
    marker::Send,
    net::SocketAddr,
    ops::{Bound, RangeBounds},
};

use actix::{actors::resolver::ResolverError, dev::ToEnvelope, Actor, Addr, Handler, Message};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use witnet_data_structures::{
    chain::{
        Block, CheckpointBeacon, DataRequestInfo, DataRequestOutput, Epoch, EpochConstants, Hash,
        InventoryEntry, InventoryItem, PublicKeyHash, RADRequest, RADTally, Reputation,
        ValueTransferOutput,
    },
    transaction::Transaction,
};
use witnet_p2p::sessions::{SessionStatus, SessionType};
use witnet_rad::error::RadError;

use super::{
    chain_manager::{ChainManagerError, StateMachine, MAX_BLOCKS_SYNC},
    epoch_manager::{
        AllEpochSubscription, EpochManagerError, SendableNotification, SingleEpochSubscription,
    },
    inventory_manager::InventoryManagerError,
    session::Session,
};

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM CHAIN MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Message result of unit
pub type SessionUnitResult = ();

/// Message to obtain the highest block checkpoint managed by the `ChainManager`
/// actor.
pub struct GetHighestCheckpointBeacon;

impl Message for GetHighestCheckpointBeacon {
    type Result = Result<CheckpointBeacon, failure::Error>;
}

/// Add a new block
pub struct AddBlocks {
    /// Blocks
    pub blocks: Vec<Block>,
}

impl Message for AddBlocks {
    type Result = SessionUnitResult;
}

/// Add a new candidate
pub struct AddCandidates {
    /// Candidates
    pub blocks: Vec<Block>,
}

impl Message for AddCandidates {
    type Result = SessionUnitResult;
}

/// Add a new transaction
pub struct AddTransaction {
    /// Transaction
    pub transaction: Transaction,
}

impl Message for AddTransaction {
    type Result = Result<(), failure::Error>;
}

/// Ask for a block identified by its hash
pub struct GetBlock {
    /// Block hash
    pub hash: Hash,
}

impl Message for GetBlock {
    type Result = Result<Block, ChainManagerError>;
}

/// Message to obtain a vector of block hashes using a range of epochs
pub struct GetBlocksEpochRange {
    /// Range of Epochs (prefer using the new method to create a range)
    pub range: (Bound<Epoch>, Bound<Epoch>),
    /// Maximum blocks limit
    pub limit: usize,
}

impl GetBlocksEpochRange {
    /// Create a GetBlockEpochRange message using range syntax:
    ///
    /// ```rust
    /// # use witnet_node::actors::messages::GetBlocksEpochRange;
    /// GetBlocksEpochRange::new(..); // Unbounded range: all items
    /// GetBlocksEpochRange::new(10..); // All items starting from epoch 10
    /// GetBlocksEpochRange::new(..10); // All items up to epoch 10 (10 excluded)
    /// GetBlocksEpochRange::new(..=9); // All items up to epoch 9 inclusive (same as above)
    /// GetBlocksEpochRange::new(4..=4); // Only epoch 4
    /// ```
    pub fn new<R: RangeBounds<Epoch>>(r: R) -> Self {
        Self::new_with_limit(r, 0)
    }
    /// new method with a constant limit
    pub fn new_with_const_limit<R: RangeBounds<Epoch>>(r: R) -> Self {
        Self::new_with_limit(r, MAX_BLOCKS_SYNC)
    }
    /// new method with a specified limit
    pub fn new_with_limit<R: RangeBounds<Epoch>>(r: R, limit: usize) -> Self {
        // Manually implement `cloned` method
        let cloned = |b: Bound<&Epoch>| match b {
            Bound::Included(x) => Bound::Included(*x),
            Bound::Excluded(x) => Bound::Excluded(*x),
            Bound::Unbounded => Bound::Unbounded,
        };

        Self {
            range: (cloned(r.start_bound()), cloned(r.end_bound())),
            limit,
        }
    }
}

impl Message for GetBlocksEpochRange {
    type Result = Result<Vec<(Epoch, Hash)>, ChainManagerError>;
}

/// A list of peers and their respective last beacon, used to establish consensus
pub struct PeersBeacons {
    /// A list of peers and their respective last beacon
    pub pb: Vec<(SocketAddr, CheckpointBeacon)>,
}

impl Message for PeersBeacons {
    /// Result: list of peers out of consensus which will be unregistered
    type Result = Result<Vec<SocketAddr>, ()>;
}

/// Builds a `ValueTransferTransaction` from a list of `ValueTransferOutput`s
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildVtt {
    /// List of `ValueTransferOutput`s
    pub vto: Vec<ValueTransferOutput>,
    /// Fee
    pub fee: u64,
}

impl Message for BuildVtt {
    type Result = Result<Hash, failure::Error>;
}

/// Builds a `DataRequestTransaction` from a `DataRequestOutput`
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildDrt {
    /// `DataRequestOutput`
    pub dro: DataRequestOutput,
    /// Fee
    pub fee: u64,
}

impl Message for BuildDrt {
    type Result = Result<Hash, failure::Error>;
}

/// Get ChainManager State (WaitingConsensus, Synchronizing, Synced)
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetState;

impl Message for GetState {
    type Result = Result<StateMachine, ()>;
}

/// Get Data Request Report
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetDataRequestReport {
    /// `DataRequest` transaction hash
    pub dr_pointer: Hash,
}

impl Message for GetDataRequestReport {
    type Result = Result<DataRequestInfo, failure::Error>;
}

/// Get Balance
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetBalance {
    /// Public key hash
    pub pkh: PublicKeyHash,
}

impl Message for GetBalance {
    type Result = Result<u64, failure::Error>;
}

/// Get Reputation of one pkh
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetReputation {
    /// Public key hash
    pub pkh: PublicKeyHash,
}

impl Message for GetReputation {
    type Result = Result<(Reputation, bool), failure::Error>;
}

/// Get all reputation from all identities
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetReputationAll;

impl Message for GetReputationAll {
    type Result = Result<HashMap<PublicKeyHash, (Reputation, bool)>, failure::Error>;
}

/// Get Reputation status: number of active identities and total active reputation
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetReputationStatus;

/// Number of active identities and total active reputation
pub struct GetReputationStatusResult {
    /// Number of active identities
    pub num_active_identities: u32,
    /// Total active reputation
    pub total_active_reputation: Reputation,
}

impl Message for GetReputationStatus {
    type Result = Result<GetReputationStatusResult, failure::Error>;
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM CONNECTIONS MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Actor message that holds the TCP stream from an inbound TCP connection
#[derive(Message)]
pub struct InboundTcpConnect {
    /// Tcp stream of the inbound connections
    pub stream: TcpStream,
}

impl InboundTcpConnect {
    /// Method to create a new InboundTcpConnect message from a TCP stream
    pub fn new(stream: TcpStream) -> InboundTcpConnect {
        InboundTcpConnect { stream }
    }
}

/// Actor message to request the creation of an outbound TCP connection to a peer.
#[derive(Message)]
pub struct OutboundTcpConnect {
    /// Address of the outbound connection
    pub address: SocketAddr,
    /// Flag to indicate if it is a peers provided from the feeler function
    pub session_type: SessionType,
}

/// Returned type by the Resolver actor for the ConnectAddr message
pub type ResolverResult = Result<TcpStream, ResolverError>;

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM EPOCH MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Returns the current epoch
pub struct GetEpoch;

/// Epoch result
pub type EpochResult<T> = Result<T, EpochManagerError>;

impl Message for GetEpoch {
    type Result = EpochResult<Epoch>;
}
/// Subscribe
pub struct Subscribe;

/// Subscribe to a single checkpoint
#[derive(Message)]
pub struct SubscribeEpoch {
    /// Checkpoint to be subscribed to
    pub checkpoint: Epoch,

    /// Notification to be sent when the checkpoint is reached
    pub notification: Box<dyn SendableNotification>,
}

/// Subscribe to all new checkpoints
#[derive(Message)]
pub struct SubscribeAll {
    /// Notification
    pub notification: Box<dyn SendableNotification>,
}

impl Subscribe {
    /// Subscribe to a specific checkpoint to get an EpochNotification
    // TODO: rename to to_checkpoint?
    // TODO: add helper Subscribe::to_next_epoch?
    // TODO: helper to subscribe to nth epoch in the future
    #[allow(clippy::wrong_self_convention)]
    pub fn to_epoch<T, U>(checkpoint: Epoch, addr: Addr<U>, payload: T) -> SubscribeEpoch
    where
        T: 'static,
        T: Send,
        U: Actor,
        U: Handler<EpochNotification<T>>,
        U::Context: ToEnvelope<U, EpochNotification<T>>,
    {
        SubscribeEpoch {
            checkpoint,
            notification: Box::new(SingleEpochSubscription {
                recipient: addr.recipient(),
                payload: Some(payload),
            }),
        }
    }
    /// Subscribe to all checkpoints to get an EpochNotification on every new epoch
    #[allow(clippy::wrong_self_convention)]
    pub fn to_all<T, U>(addr: Addr<U>, payload: T) -> SubscribeAll
    where
        T: 'static,
        T: Send + Clone,
        U: Actor,
        U: Handler<EpochNotification<T>>,
        U::Context: ToEnvelope<U, EpochNotification<T>>,
    {
        SubscribeAll {
            notification: Box::new(AllEpochSubscription {
                recipient: addr.recipient(),
                payload,
            }),
        }
    }
}

/// Message that the EpochManager sends to subscriber actors to notify a new epoch
#[derive(Message)]
pub struct EpochNotification<T: Send> {
    /// Epoch that has just started
    pub checkpoint: Epoch,

    /// Payload for the epoch notification
    pub payload: T,
}

/// Return a function which can be used to calculate the timestamp for a
/// checkpoint (the start of an epoch). This assumes that the
/// checkpoint_zero_timestamp and checkpoints_period constants never change
pub struct GetEpochConstants;

impl Message for GetEpochConstants {
    type Result = Option<EpochConstants>;
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM INVENTORY MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Add a new item
pub struct AddItem {
    /// Item
    pub item: InventoryItem,
}

impl Message for AddItem {
    type Result = Result<(), InventoryManagerError>;
}

/// Ask for an item identified by its hash
pub struct GetItem {
    /// item hash
    pub hash: Hash,
}

impl Message for GetItem {
    type Result = Result<InventoryItem, InventoryManagerError>;
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM PEERS MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// One peer
pub type PeersSocketAddrResult = Result<Option<SocketAddr>, failure::Error>;
/// One or more peer addresses
pub type PeersSocketAddrsResult = Result<Vec<SocketAddr>, failure::Error>;

/// Message to add one or more peer addresses to the list
pub struct AddPeers {
    /// Addresses of the peer
    pub addresses: Vec<SocketAddr>,

    /// Source address of the peer
    pub src_address: SocketAddr,
}

impl Message for AddPeers {
    type Result = PeersSocketAddrsResult;
}

/// Message to add one peer address to the tried addresses bucket
pub struct AddConsolidatedPeer {
    /// Tried addresses to add
    pub address: SocketAddr,
}

impl Message for AddConsolidatedPeer {
    type Result = PeersSocketAddrResult;
}

/// Message to remove one or more peer addresses from the list
pub struct RemovePeers {
    /// Address of the peer
    pub addresses: Vec<SocketAddr>,
}

impl Message for RemovePeers {
    type Result = PeersSocketAddrsResult;
}

/// Message to get a (random) peer address from the list
pub struct GetRandomPeer;

impl Message for GetRandomPeer {
    type Result = PeersSocketAddrResult;
}

/// Message to get all the peer addresses from the list
pub struct RequestPeers;

impl Message for RequestPeers {
    type Result = PeersSocketAddrsResult;
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM RAD MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Message for resolving the request-aggregate step of a data
/// request.
#[derive(Debug)]
pub struct ResolveRA {
    /// RAD request to be executed
    pub rad_request: RADRequest,
}

/// Message for running the consensus step of a data request.
#[derive(Debug)]
pub struct RunConsensus {
    /// RAD consensus to be executed
    pub script: RADTally,
    /// Reveals vector for consensus
    pub reveals: Vec<Vec<u8>>,
}

impl Message for ResolveRA {
    type Result = Result<Vec<u8>, RadError>;
}

impl Message for RunConsensus {
    type Result = Result<Vec<u8>, RadError>;
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM SESSION
////////////////////////////////////////////////////////////////////////////////////////

/// Message to indicate that the session needs to send a GetPeers message through the network
#[derive(Debug)]
pub struct SendGetPeers;

impl Message for SendGetPeers {
    type Result = SessionUnitResult;
}

impl fmt::Display for SendGetPeers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendGetPeers")
    }
}

/// Message to announce new inventory entries through the network
#[derive(Clone, Debug, Message)]
pub struct SendInventoryAnnouncement {
    /// Inventory entries
    pub items: Vec<InventoryEntry>,
}

impl fmt::Display for SendInventoryAnnouncement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendInventoryAnnouncement")
    }
}

/// Message to send inventory items through the network
#[derive(Clone, Debug, Message)]
pub struct SendInventoryItem {
    /// InventoryItem
    pub item: InventoryItem,
}

impl fmt::Display for SendInventoryItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendInventoryItem")
    }
}

/// Message to send beacon through the network
#[derive(Clone, Debug, Message)]
pub struct SendLastBeacon {
    /// The highest block checkpoint
    pub beacon: CheckpointBeacon,
}

impl fmt::Display for SendLastBeacon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendLastBeacon")
    }
}

/// Message to close an open session
#[derive(Clone, Debug, Message)]
pub struct CloseSession;

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM SESSIONS MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Message result of unit
pub type SessionsUnitResult = Result<(), failure::Error>;

/// Message indicating a new session needs to be created
pub struct Create {
    /// TCP stream
    pub stream: TcpStream,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Create {
    type Result = ();
}

/// Message indicating a new session needs to be registered
pub struct Register {
    /// Socket address which identifies the peer
    pub address: SocketAddr,

    /// Address of the session actor that is to be connected
    pub actor: Addr<Session>,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Register {
    type Result = SessionsUnitResult;
}

/// Message indicating a session needs to be unregistered
pub struct Unregister {
    /// Socket address identifying the peer
    pub address: SocketAddr,

    /// Session type
    pub session_type: SessionType,

    /// Session status
    pub status: SessionStatus,
}

impl Message for Unregister {
    type Result = SessionsUnitResult;
}

/// Message indicating a session needs to be consolidated
pub struct Consolidate {
    /// Socket address which identifies the peer
    pub address: SocketAddr,

    /// Potential peer to be added
    /// In their `Version` messages the nodes communicate the address of their server and that
    /// is a potential peer that should try to be added
    pub potential_new_peer: SocketAddr,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Consolidate {
    type Result = SessionsUnitResult;
}

/// Message indicating a message is to be forwarded to a random consolidated outbound session
pub struct Anycast<T> {
    /// Command to be sent to the session
    pub command: T,
    /// Safu flag: use only outbound peers in consensus with us?
    pub safu: bool,
}

impl<T> Message for Anycast<T>
where
    T: Message + Send + Debug,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();
}

/// Message indicating a message is to be forwarded to all the consolidated outbound sessions
pub struct Broadcast<T> {
    /// Command to be sent to all the sessions
    pub command: T,
    /// Inbound flag: use only inbound peers
    pub only_inbound: bool,
}

impl<T> Message for Broadcast<T>
where
    T: Clone + Message + Send,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();
}

/// Message indicating the last beacon received from a peer
#[derive(Clone, Debug, Message)]
pub struct PeerBeacon {
    /// Socket address which identifies the peer
    pub address: SocketAddr,
    /// Last beacon received from peer
    pub beacon: CheckpointBeacon,
}

/// Get number of inbound and outbound sessions
#[derive(Clone, Debug)]
pub struct NumSessions;

impl Message for NumSessions {
    type Result = Result<NumSessionsResult, ()>;
}

/// Number of inbound and outbound sessions
#[derive(Debug, Default)]
pub struct NumSessionsResult {
    /// Inbound
    pub inbound: usize,
    /// Outbound
    pub outbound: usize,
}

// JsonRpcServer messages (notifications)

/// New block notification
#[derive(Message)]
pub struct NewBlock {
    /// Block
    pub block: Block,
}
