use actix::{
    actors::resolver::ResolverError,
    dev::{MessageResponse, ResponseChannel, ToEnvelope},
    Actor, Addr, Handler, Message,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    fmt::Debug,
    marker::Send,
    net::SocketAddr,
    ops::{Bound, RangeBounds},
    time::Duration,
};
use tokio::net::TcpStream;

use witnet_data_structures::{
    chain::{
        Block, CheckpointBeacon, DataRequestInfo, DataRequestOutput, Epoch, EpochConstants, Hash,
        InventoryEntry, InventoryItem, NodeStats, PointerToBlock, PublicKeyHash, RADRequest,
        RADTally, Reputation, SuperBlockVote, UtxoInfo, UtxoSelectionStrategy, ValueTransferOutput,
    },
    radon_report::RadonReport,
    transaction::{CommitTransaction, RevealTransaction, Transaction},
    types::LastBeacon,
};
use witnet_p2p::{
    error::SessionsError,
    sessions::{GetConsolidatedPeersResult, SessionStatus, SessionType},
};
use witnet_rad::{error::RadError, types::RadonTypes};

use super::{
    chain_manager::{ChainManagerError, StateMachine, MAX_BLOCKS_SYNC},
    epoch_manager::{
        AllEpochSubscription, EpochManagerError, SendableNotification, SingleEpochSubscription,
    },
    inventory_manager::InventoryManagerError,
    rad_manager::RadManager,
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

/// Message to obtain the last super block votes managed by the `ChainManager`
/// actor.
pub struct GetSuperBlockVotes;

impl Message for GetSuperBlockVotes {
    type Result = Result<HashSet<SuperBlockVote>, failure::Error>;
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

/// Add a superblock vote
pub struct AddSuperBlockVote {
    /// Superblock vote
    pub superblock_vote: SuperBlockVote,
}

impl Message for AddSuperBlockVote {
    type Result = Result<(), failure::Error>;
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
    /// Maximum blocks limit. 0 means unlimited
    pub limit: usize,
    /// Whether to apply the limit from the end: return the last n blocks
    pub limit_from_end: bool,
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
            limit_from_end: false,
        }
    }
    /// new method with a specified limit, returning the last `limit` items
    pub fn new_with_limit_from_end<R: RangeBounds<Epoch>>(r: R, limit: usize) -> Self {
        let mut rb = Self::new_with_limit(r, limit);
        rb.limit_from_end = true;

        rb
    }
}

impl Message for GetBlocksEpochRange {
    type Result = Result<Vec<(Epoch, Hash)>, ChainManagerError>;
}

/// A list of peers and their respective last beacon, used to establish consensus
pub struct PeersBeacons {
    /// A list of peers and their respective last beacon
    pub pb: Vec<(SocketAddr, Option<LastBeacon>)>,
    /// Outbound limit: how many beacons did we expect in total
    pub outbound_limit: Option<u16>,
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
    /// Strategy to sort the unspent outputs pool
    #[serde(default)]
    pub utxo_strategy: UtxoSelectionStrategy,
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

/// Get ChainManager State (WaitingConsensus, Synchronizing, AlmostSynced, Synced)
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

/// Get Balance
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetUtxoInfo {
    /// Public key hash
    pub pkh: PublicKeyHash,
}

impl Message for GetUtxoInfo {
    type Result = Result<UtxoInfo, failure::Error>;
}

/// Reputation info
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReputationStats {
    /// Reputation
    pub reputation: Reputation,
    /// Eligibility: Trapezoidal reputation based on ranking
    pub eligibility: u32,
    /// Is active flag
    pub is_active: bool,
}

/// GetReputation result
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetReputationResult {
    /// Map of identity public key hash to reputation stats
    pub stats: HashMap<PublicKeyHash, ReputationStats>,
    /// Total active reputation
    pub total_reputation: u64,
}

/// Get reputation of one identity if `all` is set to `false`,
/// or all identities if `all` is set to `true`
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetReputation {
    /// Public key hash
    pub pkh: PublicKeyHash,
    /// All flag
    pub all: bool,
}

impl Message for GetReputation {
    type Result = Result<GetReputationResult, failure::Error>;
}

/// Get all the pending transactions
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetMempool;

impl Message for GetMempool {
    type Result = Result<GetMempoolResult, failure::Error>;
}

/// Result of GetMempool message: list of pending transactions categorized by type
#[derive(Serialize)]
pub struct GetMempoolResult {
    /// Pending value transfer transactions
    pub value_transfer: Vec<Hash>,
    /// Pending data request transactions
    pub data_request: Vec<Hash>,
}

/// Try to mine a block: signal the ChainManager to check if it can produce a new block
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct TryMineBlock;

impl Message for TryMineBlock {
    type Result = ();
}

/// Add a commit-reveal pair to ChainManager.
/// This will broadcast the commit and save the reveal for later
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddCommitReveal {
    /// Signed commit transaction
    pub commit_transaction: CommitTransaction,
    /// Signed reveal transaction for the commit transaction
    pub reveal_transaction: RevealTransaction,
}

impl Message for AddCommitReveal {
    type Result = Result<(), failure::Error>;
}

/// Get transaction from mempool by hash
pub struct GetMemoryTransaction {
    /// item hash
    pub hash: Hash,
}

impl Message for GetMemoryTransaction {
    type Result = Result<Transaction, ()>;
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
        T: 'static + Send,
        U: Actor + Handler<EpochNotification<T>>,
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
        T: 'static + Send + Clone,
        U: Actor + Handler<EpochNotification<T>>,
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

    /// Timestamp of the start of the epoch.
    /// This is used to verify that the messages arrive on time
    pub timestamp: i64,

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

/// Inventory element: block, txns
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum StoreInventoryItem {
    /// Blocks are stored with all the transactions inside
    Block(Box<Block>),
    /// Transactions are stored as pointers to blocks
    Transaction(Hash, PointerToBlock),
}

/// Add a new item
pub struct AddItem {
    /// Item
    pub item: StoreInventoryItem,
}

impl Message for AddItem {
    type Result = Result<(), InventoryManagerError>;
}

/// Add a new item
pub struct AddItems {
    /// Item
    pub items: Vec<StoreInventoryItem>,
}

impl Message for AddItems {
    type Result = Result<(), InventoryManagerError>;
}

/// Ask for an item identified by its hash
pub struct GetItem {
    /// item kind and hash
    pub item: InventoryEntry,
}

impl Message for GetItem {
    type Result = Result<InventoryItem, InventoryManagerError>;
}

/// Ask for an item identified by its hash
pub struct GetItemBlock {
    /// item hash
    pub hash: Hash,
}

impl Message for GetItemBlock {
    type Result = Result<Block, InventoryManagerError>;
}

/// Ask for an item identified by its hash
pub struct GetItemTransaction {
    /// item hash
    pub hash: Hash,
}

impl Message for GetItemTransaction {
    type Result = Result<(Transaction, PointerToBlock), InventoryManagerError>;
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

    /// Address of the peer that sent us this peers using the Peers protocol message, or None if
    /// the peers were added from config or from command line
    pub src_address: Option<SocketAddr>,
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
pub struct RemoveAddressesFromTried {
    /// Address of the peer
    pub addresses: Vec<SocketAddr>,
    /// Request the removed peer addresses to be iced
    pub ice: bool,
}

impl Message for RemoveAddressesFromTried {
    type Result = PeersSocketAddrsResult;
}

/// Message to get a (random) peer address from the list
pub struct GetRandomPeers {
    /// Number of random peers
    pub n: usize,
}

impl Message for GetRandomPeers {
    type Result = PeersSocketAddrsResult;
}

/// Message to get all the peer addresses from the tried list
pub struct RequestPeers;

impl Message for RequestPeers {
    type Result = PeersSocketAddrsResult;
}

/// Message to get all the peer addresses from the new and tried lists
pub struct GetKnownPeers;

impl Message for GetKnownPeers {
    type Result = Result<PeersNewTried, failure::Error>;
}

/// Message to get node stats
pub struct GetNodeStats;

impl Message for GetNodeStats {
    type Result = Result<NodeStats, failure::Error>;
}

/// List of known peers sorted by bucket
pub struct PeersNewTried {
    /// Peers in new bucket
    pub new: Vec<SocketAddr>,
    /// Peers in tried bucket
    pub tried: Vec<SocketAddr>,
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
    /// Timeout: if the execution does not finish before the timeout, it is cancelled.
    pub timeout: Option<Duration>,
}

/// Message for running the tally step of a data request.
#[derive(Debug)]
pub struct RunTally {
    /// RAD tally to be executed
    pub script: RADTally,
    /// Reveals vector for tally
    pub reports: Vec<RadonReport<RadonTypes>>,
    /// Minimum values vs. errors ratio over which tally is run, otherwise return mode of errors
    pub min_consensus_ratio: f64,
    /// Number of commits
    pub commits_count: usize,
}

impl Message for ResolveRA {
    type Result = Result<RadonReport<RadonTypes>, RadError>;
}

impl Message for RunTally {
    type Result = RadonReport<RadonTypes>;
}

impl<M> MessageResponse<RadManager, M> for RadonReport<RadonTypes>
where
    M: Message<Result = Self>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut <RadManager as Actor>::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
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
    /// Last block and superblock checkpoints
    pub last_beacon: LastBeacon,
}

impl fmt::Display for SendLastBeacon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendLastBeacon")
    }
}

/// Message to send beacon through the network
#[derive(Clone, Debug, Message)]
pub struct SendSuperBlockVote {
    /// The superblock vote
    pub superblock_vote: SuperBlockVote,
}

impl fmt::Display for SendSuperBlockVote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendSuperBlockVote")
    }
}

/// Message to close an open session
#[derive(Clone, Debug, Message)]
pub struct CloseSession;

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM SESSIONS MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Message result of unit
pub type SessionsUnitResult = Result<(), SessionsError>;

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
    pub potential_new_peer: Option<SocketAddr>,

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
    type Result = Result<T::Result, ()>;
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
    pub beacon: LastBeacon,
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

/// Get list of inbound and outbound peers
#[derive(Clone, Debug)]
pub struct GetConsolidatedPeers;

impl Message for GetConsolidatedPeers {
    type Result = Result<GetConsolidatedPeersResult, ()>;
}

/// Request logging a session message
#[derive(Clone, Debug)]
pub struct LogMessage {
    /// Message for logging
    pub log_data: String,
    /// Server address
    pub addr: SocketAddr,
}

impl Message for LogMessage {
    type Result = SessionsUnitResult;
}

/// Set the LastBeacon
#[derive(Clone, Debug)]
pub struct SetLastBeacon {
    /// Current tip of the chain
    pub beacon: LastBeacon,
}

impl Message for SetLastBeacon {
    type Result = ();
}

// JsonRpcServer messages (notifications)

/// New block notification
#[derive(Message)]
pub struct NewBlock {
    /// Block
    pub block: Block,
}

/// Notification signaling that a block has been consolidated.
///
/// As per current consensus algorithm, "consolidated" implies that there exists at least one
/// superblock in the chain that builds upon the superblock where this block was anchored.
#[derive(Message)]
pub struct ConsolidatedBlocks {
    /// The hashes of the blocks that we are signaling as consolidated.
    pub hashes: Vec<Hash>,
}
