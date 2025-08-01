use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Debug, Formatter},
    marker::Send,
    net::SocketAddr,
    ops::{Bound, RangeBounds},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use actix::{
    Actor, Addr, Handler, Message,
    dev::{MessageResponse, OneshotSender, ToEnvelope},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::net::TcpStream;

use witnet_data_structures::{
    chain::{
        Block, CheckpointBeacon, DataRequestInfo, DataRequestOutput, Epoch, EpochConstants, Hash,
        InventoryEntry, InventoryItem, KeyedSignature, NodeStats, PointerToBlock, PublicKeyHash,
        PublicKeyHashParseError, RADRequest, RADTally, Reputation, StakeOutput, StateMachine,
        SuperBlock, SuperBlockVote, SupplyInfo, SupplyInfo2, ValueTransferOutput,
        priority::PrioritiesEstimate,
        tapi::{ActiveWips, BitVotesCounter},
    },
    fee::{Fee, deserialize_fee_backwards_compatible},
    get_environment,
    proto::versioning::ProtocolInfo,
    radon_report::RadonReport,
    staking::prelude::*,
    transaction::{
        CommitTransaction, DRTransaction, RevealTransaction, StakeTransaction, Transaction,
        UnstakeTransaction, VTTransaction,
    },
    transaction_factory::{NodeBalance, NodeBalance2},
    types::LastBeacon,
    utxo_pool::{UtxoInfo, UtxoSelectionStrategy},
    wit::{WIT_DECIMAL_PLACES, Wit},
};
use witnet_p2p::{
    error::SessionsError,
    sessions::{GetConsolidatedPeersResult, SessionStatus, SessionType},
};
use witnet_rad::{error::RadError, types::RadonTypes};

use crate::{
    actors::{
        chain_manager::{ChainManagerError, ImportError, MAX_BLOCKS_SYNC},
        connections_manager::resolver::ResolverError,
        epoch_manager::{
            AllEpochSubscription, EpochManagerError, SendableNotification, SingleEpochSubscription,
        },
        inventory_manager::InventoryManagerError,
        rad_manager::RadManager,
        session::Session,
    },
    utils::Force,
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
    type Result = Result<CheckpointBeacon, anyhow::Error>;
}

/// Message to obtain the last super block votes managed by the `ChainManager`
/// actor.
pub struct GetSuperBlockVotes;

impl Message for GetSuperBlockVotes {
    type Result = Result<HashSet<SuperBlockVote>, anyhow::Error>;
}

/// Add a new block
pub struct AddBlocks {
    /// Blocks
    pub blocks: Vec<Block>,
    /// Sender peer
    pub sender: Option<SocketAddr>,
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
    type Result = Result<(), anyhow::Error>;
}

/// Add a new transaction
pub struct AddTransaction {
    /// Transaction
    pub transaction: Transaction,
    /// Broadcasting flag
    pub broadcast_flag: bool,
}

impl Message for AddTransaction {
    type Result = Result<(), anyhow::Error>;
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
    #[serde(deserialize_with = "deserialize_fee_backwards_compatible")]
    pub fee: Fee,
    /// Strategy to sort the unspent outputs pool
    #[serde(default)]
    pub utxo_strategy: UtxoSelectionStrategy,
    /// Construct the transaction but do not broadcast it
    #[serde(default)]
    pub dry_run: bool,
}

impl Message for BuildVtt {
    type Result = Result<VTTransaction, anyhow::Error>;
}

/// Builds a `StakeTransaction` from a list of `ValueTransferOutput`s
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildStake {
    /// One instance of `StakeOutput`
    pub stake_output: StakeOutput,
    /// Fee
    #[serde(default)]
    pub fee: Fee,
    /// Strategy to sort the unspent outputs pool
    #[serde(default)]
    pub utxo_strategy: UtxoSelectionStrategy,
    /// Construct the transaction but do not broadcast it
    #[serde(default)]
    pub dry_run: bool,
}

impl Message for BuildStake {
    type Result = Result<StakeTransaction, anyhow::Error>;
}

/// Builds an `UnstakeTransaction`
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildUnstake {
    /// Node address operating the staked coins
    pub operator: PublicKeyHash,
    /// Amount to unstake
    #[serde(default)]
    pub value: u64,
    /// Fee for the unstake transaction
    #[serde(default)]
    pub fee: u64,
    /// Construct the transaction but do not broadcast it
    #[serde(default)]
    pub dry_run: bool,
}

impl Message for BuildUnstake {
    type Result = Result<UnstakeTransaction, anyhow::Error>;
}

/// Builds a `UnstakeTransaction` from a list of `ValueTransferOutput`s
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildUnstakeParams {
    /// Node address operating the staked coins
    pub operator: MagicEither<String, PublicKeyHash>,
    /// Amount to unstake
    #[serde(default)]
    pub value: u64,
    /// Fee for the unstake transaction
    #[serde(default)]
    pub fee: u64,
    /// Construct the transaction but do not broadcast it
    #[serde(default)]
    pub dry_run: bool,
}

/// Builds a `StakeTransaction` from a list of `ValueTransferOutput`s
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildStakeParams {
    /// Authorization signature and public key
    #[serde(default)]
    pub authorization: MagicEither<String, KeyedSignature>,
    /// List of `ValueTransferOutput`s
    #[serde(default)]
    pub value: u64,
    /// Withdrawer
    #[serde(default)]
    pub withdrawer: MagicEither<String, PublicKeyHash>,
    /// Fee
    #[serde(default)]
    pub fee: Fee,
    /// Strategy to sort the unspent outputs pool
    #[serde(default)]
    pub utxo_strategy: UtxoSelectionStrategy,
    /// Construct the transaction but do not broadcast it
    #[serde(default)]
    pub dry_run: bool,
}

/// The response to a `BuildStake` message. It gives important feedback about the addresses that will be involved in a
/// stake transactions, subject to review and confirmation from the user.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildStakeResponse {
    /// A stake transaction that has been created as a response to a `BuildStake` message.
    pub transaction: StakeTransaction,
    /// The addresses of the staker. These are the addresses used in the stake transaction inputs.
    pub staker: Vec<PublicKeyHash>,
    /// The address of the validator. This shall be the address of the node that will operate this stake on behalf of
    /// the staker.
    pub validator: PublicKeyHash,
    /// The address of the withdrawer. This shall be the an address controlled by the staker. When unstaking, the
    /// staked principal plus any yield will only be spendable by this address.
    pub withdrawer: PublicKeyHash,
}

/// Builds an `AuthorizeStake`
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeStake {
    /// Address that can withdraw the stake
    #[serde(default)]
    pub withdrawer: Option<String>,
}

impl Message for AuthorizeStake {
    type Result = Result<String, anyhow::Error>;
}

/// Builds an `StakeAuthorization`
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StakeAuthorization {
    /// Address that can withdraw the stake
    pub withdrawer: PublicKeyHash,
    /// A node's signature of a withdrawer's address
    pub signature: KeyedSignature,
}

impl Message for StakeAuthorization {
    type Result = Result<String, anyhow::Error>;
}

/// Message for querying stakes
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryStakingPowers {
    /// Retrieve only first appearence for every distinct validator (default: false)
    pub distinct: Option<bool>,
    /// Limits max number of entries to return (default: 0 == u16::MAX)
    pub limit: Option<u16>,
    /// Skips first found entries (default: 0)
    pub offset: Option<usize>,
    /// Order by specified capability (default: reverse order by coins)
    pub order_by: Capability,
}

impl Default for QueryStakingPowers {
    fn default() -> Self {
        QueryStakingPowers {
            distinct: Some(false),
            limit: Some(u16::MAX),
            offset: Some(0),
            order_by: Capability::Mining,
        }
    }
}

impl Message for QueryStakingPowers {
    type Result = Vec<(usize, StakeKey<PublicKeyHash>, Power)>;
}

/// Stake key for quering stakes
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
pub struct QueryStakesFilter {
    /// To search by the validator public key hash
    pub validator: Option<MagicEither<String, PublicKeyHash>>,
    /// To search by the withdrawer public key hash
    pub withdrawer: Option<MagicEither<String, PublicKeyHash>>,
}

/// Limits when querying stake entries
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryStakesParams {
    /// Retrieve only first appearence for every distinct validator (default: false)
    pub distinct: Option<bool>,
    /// Limits max number of entries to return (default: 0 == u16::MAX)
    pub limit: Option<u16>,
    /// Skips first found entries (default: 0)
    pub offset: Option<usize>,
    /// Order by specified stake entry field (default: reverse order by coins)
    pub order: Option<QueryStakesOrderBy>,
    /// Select entries having either the nonce, last mining or last witnessing epoch,
    /// greater than specified absolute epoch, or relative epoch if negative (default: 0)
    pub since: Option<i64>,
}

impl Default for QueryStakesParams {
    fn default() -> Self {
        QueryStakesParams {
            distinct: Some(false),
            limit: Some(u16::MAX),
            offset: Some(0),
            order: Some(QueryStakesOrderBy::default()),
            since: Some(0),
        }
    }
}

/// Order by parameter for QueryStakes
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub struct QueryStakesOrderBy {
    /// Data field to order by
    pub by: QueryStakesOrderByOptions,
    /// Reverse order (default: false)
    pub reverse: Option<bool>,
}

impl Default for QueryStakesOrderBy {
    fn default() -> Self {
        QueryStakesOrderBy {
            by: QueryStakesOrderByOptions::Coins,
            reverse: Some(true),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
/// Ordering options for QueryStakes
pub enum QueryStakesOrderByOptions {
    /// Order by stake entry coins
    Coins = 0,
    /// Order by stake entry nonce
    Nonce = 1,
    /// Order by last validation epoch
    Mining = 2,
    /// Order by last witnessing epoch
    Witnessing = 3,
}

/// Message for querying stakes
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryStakes {
    /// query's where clause
    pub filter: Option<QueryStakesFilter>,
    /// query's limit params
    pub params: Option<QueryStakesParams>,
}

impl Message for QueryStakes {
    type Result = Result<
        Vec<StakeEntry<WIT_DECIMAL_PLACES, PublicKeyHash, Wit, Epoch, u64, u64>>,
        anyhow::Error,
    >;
}

impl<Address> TryFrom<QueryStakesFilter> for QueryStakesKey<Address>
where
    Address: Default + Ord + From<PublicKeyHash>,
{
    type Error = PublicKeyHashParseError;

    fn try_from(filter: QueryStakesFilter) -> Result<Self, Self::Error> {
        Ok(match (filter.validator, filter.withdrawer) {
            (Some(validator), Some(withdrawer)) => QueryStakesKey::Key(StakeKey {
                validator: try_do_magic_into_pkh(validator)?.into(),
                withdrawer: try_do_magic_into_pkh(withdrawer)?.into(),
            }),
            (Some(validator), None) => {
                QueryStakesKey::Validator(try_do_magic_into_pkh(validator)?.into())
            }
            (None, Some(withdrawer)) => {
                QueryStakesKey::Withdrawer(try_do_magic_into_pkh(withdrawer)?.into())
            }
            (None, None) => QueryStakesKey::All,
        })
    }
}

/// Builds a `DataRequestTransaction` from a `DataRequestOutput`
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuildDrt {
    /// `DataRequestOutput`
    pub dro: DataRequestOutput,
    /// Fee
    #[serde(deserialize_with = "deserialize_fee_backwards_compatible")]
    pub fee: Fee,
    /// Construct the transaction but do not broadcast it
    #[serde(default)]
    pub dry_run: bool,
}

impl Message for BuildDrt {
    type Result = Result<DRTransaction, anyhow::Error>;
}

/// Get ChainManager State (WaitingConsensus, Synchronizing, AlmostSynced, Synced)
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetState;

impl Message for GetState {
    type Result = Result<StateMachine, ()>;
}

/// Get Data Request Info
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetDataRequestInfo {
    /// `DataRequest` transaction hash
    pub dr_pointer: Hash,
}

impl Message for GetDataRequestInfo {
    type Result = Result<DataRequestInfo, anyhow::Error>;
}

/// Get Wit/2 balance
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum GetBalance2 {
    /// Get balance info for all addresses in the network.
    All(GetBalance2Limits),
    /// Get balance info for a specific address.
    Address(MagicEither<String, PublicKeyHash>),
    /// Sum up balances of specified addresses.
    Sum(Vec<MagicEither<String, PublicKeyHash>>),
}

/// Limits when querying balances for all holders
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalance2Limits {
    // /// Just count records
    // pub count: Option<bool>,
    /// Search for identities holding at least this amount of nanowits
    pub min_balance: u64,
    /// Search for identities holding at most this amout of nanowits
    pub max_balance: Option<u64>,
}

impl Message for GetBalance2 {
    type Result = Result<NodeBalance2, anyhow::Error>;
}

/// Tells the `getBalance` method whether to get the balance of all addresses, one provided address,
/// or our own.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum GetBalanceTarget {
    /// Get balance for all addresses in the network.
    #[default]
    All,
    /// Get balance for a specific address.
    Address(PublicKeyHash),
    /// Get balance for our own address.
    Own,
}

impl GetBalanceTarget {
    /// Obtain a `GetBalanceTarget::Own` from a `GetBalanceTarget::Pkh` that contains our own
    /// address.
    pub fn resolve_own(self, own_pkh: PublicKeyHash) -> Self {
        if self == GetBalanceTarget::Address(own_pkh) {
            GetBalanceTarget::Own
        } else {
            self
        }
    }
}

impl From<Option<PublicKeyHash>> for GetBalanceTarget {
    /// Provides a convenient way to derive a `GetBalanceTarget` from an optional address parameter.
    fn from(address: Option<PublicKeyHash>) -> Self {
        if let Some(address) = address {
            GetBalanceTarget::Address(address)
        } else {
            GetBalanceTarget::Own
        }
    }
}

impl FromStr for GetBalanceTarget {
    type Err = PublicKeyHashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "all" => GetBalanceTarget::All,
            "own" => GetBalanceTarget::Own,
            address => {
                let env = witnet_data_structures::get_environment();
                let address = PublicKeyHash::from_bech32(env, address)?;
                GetBalanceTarget::Address(address)
            }
        })
    }
}

struct GetBalanceTargetVisitor;

impl serde::de::Visitor<'_> for GetBalanceTargetVisitor {
    type Value = GetBalanceTarget;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string, either containing `all`, `own` or a Bech32 address")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Self::Value::from_str(s).map_err(|e| E::custom(e))
    }
}
impl<'de> Deserialize<'de> for GetBalanceTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(GetBalanceTargetVisitor)
    }
}

impl serde::ser::Serialize for GetBalanceTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string = match self {
            GetBalanceTarget::All => "all".into(),
            GetBalanceTarget::Address(address) => {
                let env = witnet_data_structures::get_environment();
                address.bech32(env)
            }
            GetBalanceTarget::Own => "own".into(),
        };

        serializer.serialize_str(&string)
    }
}

/// Get Balance
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetBalance {
    /// Public key hash
    pub target: GetBalanceTarget,
    /// Distinguish between fetching a simple balance or fetching confirmed and unconfirmed balance
    pub simple: bool,
}

impl Message for GetBalance {
    type Result = Result<NodeBalance, anyhow::Error>;
}

/// Get Supply
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetSupplyInfo;

impl Message for GetSupplyInfo {
    type Result = Result<SupplyInfo, anyhow::Error>;
}

/// Get Supply after V1_8 activation
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetSupplyInfo2;

impl Message for GetSupplyInfo2 {
    type Result = Result<SupplyInfo2, anyhow::Error>;
}

/// Get Balance
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetUtxoInfo {
    /// Public key hash
    pub pkh: PublicKeyHash,
}

impl Message for GetUtxoInfo {
    type Result = Result<UtxoInfo, anyhow::Error>;
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
    type Result = Result<GetReputationResult, anyhow::Error>;
}

/// Get all the pending transactions
#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetMempool;

impl Message for GetMempool {
    type Result = Result<GetMempoolResult, anyhow::Error>;
}

/// Result of GetMempool message: list of pending transactions categorized by type
#[derive(Serialize)]
pub struct GetMempoolResult {
    /// Pending value transfer transactions
    pub value_transfer: Vec<Hash>,
    /// Pending data request transactions
    pub data_request: Vec<Hash>,
    /// Pending stake transactions
    pub stake: Vec<Hash>,
    /// Pending unstake transactions
    pub unstake: Vec<Hash>,
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
    type Result = Result<(), anyhow::Error>;
}

/// Get transaction from mempool by hash
pub struct GetMemoryTransaction {
    /// item hash
    pub hash: Hash,
}

impl Message for GetMemoryTransaction {
    type Result = Result<Transaction, ()>;
}

/// Used to set the target superblock needed for synchronization
pub struct AddSuperBlock {
    /// Superblock
    pub superblock: SuperBlock,
}

impl Message for AddSuperBlock {
    type Result = ();
}

/// Returns true if the provided block hash is the consolidated block for the provided epoch, and
/// there exists a superblock with a majority of votes to confirm that.
pub struct IsConfirmedBlock {
    /// Block hash
    pub block_hash: Hash,
    /// Block checkpoint
    pub block_epoch: u32,
}

impl Message for IsConfirmedBlock {
    type Result = Result<bool, anyhow::Error>;
}

/// Rewind
pub struct Rewind {
    /// Epoch
    pub epoch: u32,
}

impl Message for Rewind {
    type Result = Result<bool, anyhow::Error>;
}

/** Commands for exporting and importing chain state snapshots **/
/// Create and export a snapshot of the current chain state.
pub struct SnapshotExport {
    /// The output path where the snapshot file should be written to.
    pub path: Force<PathBuf>,
}

impl Message for SnapshotExport {
    type Result = Result<String, anyhow::Error>;
}

/// Create and export a snapshot of the current chain state.
pub struct SnapshotImport {
    /// The path to the snapshot file to read.
    pub path: Force<PathBuf>,
}

impl Message for SnapshotImport {
    type Result = Result<CheckpointBeacon, ImportError>;
}

/// Set the EpochConstants
#[derive(Clone, Debug)]
pub struct SetEpochConstants {
    /// Current epoch constants
    pub epoch_constants: EpochConstants,
}

impl Message for SetEpochConstants {
    type Result = ();
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM CONNECTIONS MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// Actor message that holds the TCP stream from an inbound TCP connection
pub struct InboundTcpConnect {
    /// Tcp stream of the inbound connections
    pub stream: TcpStream,
}

impl Message for InboundTcpConnect {
    type Result = ();
}

impl InboundTcpConnect {
    /// Method to create a new InboundTcpConnect message from a TCP stream
    pub fn new(stream: TcpStream) -> InboundTcpConnect {
        InboundTcpConnect { stream }
    }
}

/// Actor message to request the creation of an outbound TCP connection to a peer.
pub struct OutboundTcpConnect {
    /// Address of the outbound connection
    pub address: SocketAddr,
    /// Flag to indicate if it is a peers provided from the feeler function
    pub session_type: SessionType,
}

impl Message for OutboundTcpConnect {
    type Result = ();
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
pub struct SubscribeEpoch {
    /// Checkpoint to be subscribed to
    pub checkpoint: Epoch,

    /// Notification to be sent when the checkpoint is reached
    pub notification: Box<dyn SendableNotification>,
}

impl Message for SubscribeEpoch {
    type Result = ();
}

/// Subscribe to all new checkpoints
pub struct SubscribeAll {
    /// Notification
    pub notification: Box<dyn SendableNotification>,
}

impl Message for SubscribeAll {
    type Result = ();
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
pub struct EpochNotification<T: Send> {
    /// Epoch that has just started
    pub checkpoint: Epoch,

    /// Timestamp of the start of the epoch.
    /// This is used to verify that the messages arrive on time
    pub timestamp: i64,

    /// Payload for the epoch notification
    pub payload: T,
}

impl<T: Send> Message for EpochNotification<T> {
    type Result = ();
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
    /// Superblocks are stored as the list of block hashes
    Superblock(SuperBlockNotify),
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
    type Result = Result<(Transaction, PointerToBlock, Epoch), InventoryManagerError>;
}

/// Ask for a superblock identified by its index
pub struct GetItemSuperblock {
    /// item hash
    pub superblock_index: u32,
}

impl Message for GetItemSuperblock {
    type Result = Result<SuperBlockNotify, InventoryManagerError>;
}

/// Ask for every superblock in the storage
pub struct GetAllSuperblocks;

impl Message for GetAllSuperblocks {
    type Result = Result<Vec<SuperBlock>, InventoryManagerError>;
}

/// Get TAPI Signaling Info
pub struct GetSignalingInfo {}

/// Result of GetSignalingInfo
#[derive(Deserialize, Serialize)]
pub struct SignalingInfo {
    /// List of protocol upgrades that are already active, and their activation epoch
    pub active_upgrades: HashMap<String, Epoch>,
    /// List of protocol upgrades that are currently being polled for activation signaling
    pub pending_upgrades: Vec<BitVotesCounter>,
    /// Last epoch
    pub epoch: Epoch,
}

impl Message for GetSignalingInfo {
    type Result = Result<SignalingInfo, anyhow::Error>;
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGES FROM PEERS MANAGER
////////////////////////////////////////////////////////////////////////////////////////

/// One peer
pub type PeersSocketAddrResult = Result<Option<SocketAddr>, anyhow::Error>;
/// One or more peer addresses
pub type PeersSocketAddrsResult = Result<Vec<SocketAddr>, anyhow::Error>;

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

/// Message to clear peers from buckets
pub struct ClearPeers;

impl Message for ClearPeers {
    type Result = Result<(), anyhow::Error>;
}

/// Message to clear peers from buckets and initialize to those in config
pub struct InitializePeers {
    /// Peers with which to initialize the buckets
    pub known_peers: Vec<SocketAddr>,
}

impl Message for InitializePeers {
    type Result = Result<(), anyhow::Error>;
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
    type Result = Result<PeersNewTried, anyhow::Error>;
}

/// Message to get node stats
pub struct GetNodeStats;

impl Message for GetNodeStats {
    type Result = Result<NodeStats, anyhow::Error>;
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
    /// Active Witnet protocol improvements as of the current epoch.
    /// Used to select the correct version of the validation logic.
    pub active_wips: ActiveWips,
    /// Whether too many witnesses have been requested.
    pub too_many_witnesses: bool,
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
    /// Active Witnet protocol improvements as of the block that will include this tally.
    /// Used to select the correct version of the validation logic.
    pub active_wips: ActiveWips,
    /// Variable indicating if the amount of requested witnesses exceeds a certain fraction of the amount of stakers
    pub too_many_witnesses: bool,
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
    fn handle(self, _: &mut <RadManager as Actor>::Context, tx: Option<OneshotSender<M::Result>>) {
        if let Some(tx) = tx {
            if let Err(_self) = tx.send(self) {
                // TODO: can this ever happen?
                log::error!("Failed to send RadonReport through OneshotSender channel");
            }
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
#[derive(Clone, Debug)]
pub struct SendInventoryAnnouncement {
    /// Inventory entries
    pub items: Vec<InventoryEntry>,
}

impl Message for SendInventoryAnnouncement {
    type Result = ();
}

impl fmt::Display for SendInventoryAnnouncement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendInventoryAnnouncement")
    }
}

/// Message to request new inventory entries through the network
#[derive(Clone, Debug)]
pub struct SendInventoryRequest {
    /// Inventory entries
    pub items: Vec<InventoryEntry>,
}

impl Message for SendInventoryRequest {
    type Result = ();
}

impl fmt::Display for SendInventoryRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendInventoryRequest")
    }
}

/// Message to send inventory items through the network
#[derive(Clone, Debug)]
pub struct SendInventoryItem {
    /// InventoryItem
    pub item: InventoryItem,
}

impl Message for SendInventoryItem {
    type Result = ();
}

impl fmt::Display for SendInventoryItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendInventoryItem")
    }
}

/// Message to send beacon through the network
#[derive(Clone, Debug)]
pub struct SendLastBeacon {
    /// Last block and superblock checkpoints
    pub last_beacon: LastBeacon,
}

impl Message for SendLastBeacon {
    type Result = ();
}

impl fmt::Display for SendLastBeacon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendLastBeacon")
    }
}

/// Message to send beacon through the network
#[derive(Clone, Debug)]
pub struct SendSuperBlockVote {
    /// The superblock vote
    pub superblock_vote: SuperBlockVote,
}

impl Message for SendSuperBlockVote {
    type Result = ();
}

impl fmt::Display for SendSuperBlockVote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendSuperBlockVote")
    }
}

/// Message to close an open session
#[derive(Clone, Debug)]
pub struct CloseSession;

impl Message for CloseSession {
    type Result = ();
}

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
#[derive(Clone, Debug)]
pub struct PeerBeacon {
    /// Socket address which identifies the peer
    pub address: SocketAddr,
    /// Last beacon received from peer
    pub beacon: LastBeacon,
}

impl Message for PeerBeacon {
    type Result = ();
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

/// Drop outbound peers
#[derive(Clone, Debug)]
pub struct DropOutboundPeers {
    /// peers to be dropped
    pub peers_to_drop: Vec<SocketAddr>,
}
impl Message for DropOutboundPeers {
    type Result = ();
}

/// Drop all peers
#[derive(Clone, Debug)]
pub struct DropAllPeers;

impl Message for DropAllPeers {
    type Result = ();
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

/// Set the SuperBlock Target Beacon
#[derive(Clone, Debug)]
pub struct SetSuperBlockTargetBeacon {
    /// Target superblock beacon
    pub beacon: Option<CheckpointBeacon>,
}

impl Message for SetSuperBlockTargetBeacon {
    type Result = ();
}

/// Set the outbound limit
#[derive(Clone, Debug)]
pub struct SetPeersLimits {
    /// Inbound peers limit
    pub inbound: u16,
    /// Outbound peers limit
    pub outbound: u16,
}

impl Message for SetPeersLimits {
    type Result = ();
}

// JsonRpcServer messages (notifications)

/// New block notification
pub struct BlockNotify {
    /// Block
    pub block: Block,
}

impl Message for BlockNotify {
    type Result = ();
}

/// Notification signaling that a superblock has been consolidated.
///
/// As per current consensus algorithm, "consolidated blocks" implies that there exists at least one
/// superblock in the chain that builds upon the superblock where those blocks were anchored.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SuperBlockNotify {
    /// The superblock that we are signaling as consolidated.
    pub superblock: SuperBlock,
    /// The hashes of the blocks that we are signaling as consolidated.
    pub consolidated_block_hashes: Vec<Hash>,
}

impl Message for SuperBlockNotify {
    type Result = ();
}

/// Notification signaling that the node's state has changed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NodeStatusNotify {
    /// The node status.
    pub node_status: StateMachine,
}

impl Message for NodeStatusNotify {
    type Result = ();
}

/// Message for ordering a transaction priority estimation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EstimatePriority;

impl Message for EstimatePriority {
    type Result = Result<PrioritiesEstimate, anyhow::Error>;
}

/// Message for fetching protocol information.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GetProtocolInfo;

impl Message for crate::actors::messages::GetProtocolInfo {
    type Result = Result<Option<ProtocolInfo>, anyhow::Error>;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
/// A value that can either be L, R, where an R can always be obtained through the `do_magic` method.
pub enum MagicEither<L, R> {
    /// A first variant.
    Left(L),
    /// A second variant.
    Right(R),
}

impl<L, R> MagicEither<L, R> {
    /// Obtain an R value, even if this was an instance of L.
    pub fn do_magic<F>(self, trick: F) -> R
    where
        F: Fn(L) -> R,
    {
        match self {
            Self::Left(l) => trick(l),
            Self::Right(r) => r,
        }
    }

    /// Fallible version of `do_magic`.
    pub fn try_do_magic<F, E>(self, trick: F) -> Result<R, E>
    where
        F: Fn(L) -> Result<R, E>,
    {
        match self {
            Self::Left(l) => trick(l),
            Self::Right(r) => Ok(r),
        }
    }
}

impl<L: Default, R: Default> Default for MagicEither<L, R> {
    fn default() -> Self {
        MagicEither::Left(L::default())
    }
}

/// Checks whether passed value is a String or a PublicKeyHash, and in case of being
/// a String, tries to parse it and convert it into a PublicKeyHash value.
pub fn try_do_magic_into_pkh(
    address: MagicEither<String, PublicKeyHash>,
) -> Result<PublicKeyHash, PublicKeyHashParseError> {
    let trick = |hex_str: String| PublicKeyHash::from_bech32(get_environment(), &hex_str);
    address.try_do_magic(trick)
}
