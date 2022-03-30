use crate::{
    chain::{Epoch, Input, OutputPointer, PublicKeyHash, ValueTransferOutput},
    transaction_factory::OutputsCollection,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    str::FromStr,
    sync::Arc,
};
use witnet_storage::storage::{Storage, WriteBatch};
use witnet_util::timestamp::get_timestamp;

/// Unspent Outputs Pool
#[derive(Clone, Default)]
pub struct UnspentOutputsPool {
    /// Unconfirmed Unspent Transaction Outputs
    pub diff: Diff,
    /// Database
    // If the database is set to None, all reads will return "not found", but all writes will panic
    // This ensures that we can use an UnspentOutputsPool with no database in tests, and it will
    // work fine as long as we don't try to persist it
    pub db: Option<Arc<dyn Storage + Send + Sync>>,
}

impl fmt::Debug for UnspentOutputsPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnspentOutputsPool")
            .field("diff", &self.diff)
            .field("db", &if self.db.is_some() { Some("db") } else { None })
            .finish()
    }
}

impl PartialEq for UnspentOutputsPool {
    fn eq(&self, other: &Self) -> bool {
        self.diff == other.diff
    }
}

impl UnspentOutputsPool {
    /// Get the value transfer output referred to by the provided `OutputPointer`
    pub fn get(&self, k: &OutputPointer) -> Option<ValueTransferOutput> {
        self.get_map(k).map(|(vt, _n)| vt)
    }

    fn get_map(&self, k: &OutputPointer) -> Option<(ValueTransferOutput, u32)> {
        if self.diff.utxos_to_remove.contains(k) {
            return None;
        }

        if let Some(x) = self.diff.utxos_to_add.get(k) {
            return Some(x.clone());
        }

        self.db_get(k)
    }

    /// Returns true if the `OutputPointer` exists inside the `UnspentOutputsPool`
    pub fn contains_key(&self, k: &OutputPointer) -> bool {
        self.get_map(k).is_some()
    }

    /// Insert a new unspent `OutputPointer`
    pub fn insert(&mut self, k: OutputPointer, v: ValueTransferOutput, block_number: u32) {
        let old = self.diff.utxos_to_add.insert(k, (v, block_number));

        assert!(old.is_none(), "UTXO did already exist");
    }

    /// Remove a spent `OutputPointer`
    pub fn remove(&mut self, k: &OutputPointer) {
        let did_exist = self.diff.utxos_to_remove.insert(k.clone());

        assert!(did_exist, "tried to remove an already removed UTXO");
    }

    fn db_get(&self, k: &OutputPointer) -> Option<(ValueTransferOutput, u32)> {
        let key_string = format!("UTXO-{}", k);

        self.db
            .as_ref()?
            .get(key_string.as_bytes())
            .expect("db fail")
            .map(|bytes| {
                bincode::deserialize::<(ValueTransferOutput, u32)>(&bytes).expect("bincode fail")
            })
    }

    fn db_insert(
        &mut self,
        batch: &mut WriteBatch,
        k: OutputPointer,
        v: ValueTransferOutput,
        block_number: u32,
    ) {
        // Sanity check that UTXOs are only written once
        let old_vto = self.get_map(&k);
        assert_eq!(
            old_vto, None,
            "Tried to consolidate an UTXO that was already consolidated"
        );

        let key_string = format!("UTXO-{}", k);
        batch.put(
            key_string.into_bytes(),
            bincode::serialize(&(v, block_number)).expect("bincode fail"),
        );
    }

    fn db_remove(
        &mut self,
        batch: &mut WriteBatch,
        k: &OutputPointer,
    ) -> Option<(ValueTransferOutput, u32)> {
        // Sanity check that UTXOs are only removed once
        let old_vto = self.get_map(k);
        assert!(
            old_vto.is_some(),
            "Tried to remove an UTXO that was already removed"
        );

        let key_string = format!("UTXO-{}", k);
        batch.delete(key_string.as_bytes().to_vec());

        old_vto
    }

    fn db_iter(&self) -> impl Iterator<Item = (OutputPointer, (ValueTransferOutput, u32))> + '_ {
        self.db
            .as_ref()
            .map(|db| {
                db.prefix_iterator(b"UTXO-").unwrap().map(|(k, v)| {
                    let key_string = String::from_utf8(k).unwrap();
                    let output_pointer_str = key_string.strip_prefix("UTXO-").unwrap();
                    let key = OutputPointer::from_str(output_pointer_str).unwrap();
                    let value = bincode::deserialize(&v).unwrap();

                    (key, value)
                })
            })
            // Transform `Option<impl Iterator>` into `impl Iterator`, with 0 elements in
            // None case
            .into_iter()
            .flatten()
    }

    /// Iterate over all the unspent outputs
    pub fn iter(&self) -> impl Iterator<Item = (OutputPointer, (ValueTransferOutput, u32))> + '_ {
        self.diff
            .utxos_to_add
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .chain(self.db_iter())
            .filter_map(move |(k, v)| {
                if self.diff.utxos_to_remove.contains(&k) {
                    None
                } else {
                    Some((k, v))
                }
            })
    }

    /// Visit all the UTXOs using two functions: the first one will visit the confirmed UTXOs, while
    /// the second one will visit all the UTXOs, confirmed and unconfirmed.
    pub fn visit<F1, F2>(&self, fn_confirmed: F1, mut fn_all: F2)
    where
        F1: FnMut(&(OutputPointer, (ValueTransferOutput, u32))),
        F2: FnMut(&(OutputPointer, (ValueTransferOutput, u32))),
    {
        self.diff
            .utxos_to_add
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .chain(self.db_iter().inspect(fn_confirmed))
            .filter_map(move |(k, v)| {
                if self.diff.utxos_to_remove.contains(&k) {
                    None
                } else {
                    Some((k, v))
                }
            })
            .for_each(|x| fn_all(&x))
    }

    /// Returns the number of the block that included the transaction referenced
    /// by this OutputPointer. The difference between that number and the
    /// current number of consolidated blocks is the "collateral age".
    pub fn included_in_block_number(&self, k: &OutputPointer) -> Option<u32> {
        self.get_map(k).map(|(_vt, n)| n)
    }

    pub fn persist(&mut self) {
        let mut batch = WriteBatch::default();

        self.persist_add_to_batch(&mut batch);

        self.db
            .as_mut()
            .expect("no db")
            .write(batch)
            .expect("write_batch fail");
    }

    pub fn persist_add_to_batch(&mut self, batch: &mut WriteBatch) {
        let mut diff = std::mem::take(&mut self.diff);
        for (k, (v, block_number)) in diff.utxos_to_add.drain() {
            if diff.utxos_to_remove.remove(&k) {
                // This UTXO would be inserted and then removed, so we can skip both operations.
                // But check that the insertion would have been valid, to detect errors.
                let old_vto = self.get_map(&k);
                assert_eq!(
                    old_vto, None,
                    "Tried to consolidate an UTXO that was already consolidated"
                );
            } else {
                self.db_insert(batch, k, v, block_number);
            }
        }

        for k in diff.utxos_to_remove.drain() {
            self.db_remove(batch, &k);
        }
    }

    pub fn remove_persisted_from_memory(&mut self, persisted: &Diff) {
        for k in persisted.utxos_to_add.keys() {
            self.diff.utxos_to_add.remove(k).unwrap();
        }

        for k in &persisted.utxos_to_remove {
            assert!(self.diff.utxos_to_remove.remove(k));
        }
    }

    pub fn migrate_old_unspent_outputs_pool_to_db<F: Fn(usize, usize)>(
        &mut self,
        old: &mut OldUnspentOutputsPool,
        progress: F,
    ) {
        let mut batch = WriteBatch::default();
        let total = old.map.len();

        for (i, (k, (v, block_number))) in old.map.drain().enumerate() {
            self.db_insert(&mut batch, k, v, block_number);
            progress(i, total);
        }

        self.db
            .as_mut()
            .expect("no db")
            .write(batch)
            .expect("write_batch fail");
    }

    /// Delete all the UTXOs stored in the database. Returns the number of removed UTXOs.
    pub fn delete_all_from_db(&mut self) -> usize {
        let mut batch = WriteBatch::default();

        let total = self.delete_all_from_db_batch(&mut batch);

        self.db
            .as_mut()
            .expect("no db")
            .write(batch)
            .expect("write_batch fail");

        total
    }

    /// Delete all the UTXOs stored in the database. Returns the number of removed UTXOs.
    pub fn delete_all_from_db_batch(&mut self, batch: &mut WriteBatch) -> usize {
        let mut total = 0;
        for (k, _v) in self.db_iter() {
            let key_string = format!("UTXO-{}", k);
            batch.delete(key_string.as_bytes().to_vec());
            total += 1;
        }

        total
    }
}

/// List of unspent outputs that can be spent by this node
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OwnUnspentOutputsPool {
    /// Map of output pointer to timestamp
    /// Those UTXOs have a timestamp value to avoid double spending
    map: HashMap<OutputPointer, u64>,
}

impl OwnUnspentOutputsPool {
    pub fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&u64>
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.get(k)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut u64>
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.get_mut(k)
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        OutputPointer: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq,
    {
        self.map.contains_key(k)
    }

    pub fn insert(&mut self, k: OutputPointer, v: u64) -> Option<u64> {
        self.map.insert(k, v)
    }

    pub fn remove(&mut self, k: &OutputPointer) -> Option<u64> {
        self.map.remove(k)
    }

    pub fn drain(&mut self) -> std::collections::hash_map::Drain<OutputPointer, u64> {
        self.map.drain()
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<OutputPointer, u64> {
        self.map.iter()
    }

    pub fn keys(&self) -> std::collections::hash_map::Keys<OutputPointer, u64> {
        self.map.keys()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Method to sort own_utxos by value
    pub fn sort(&self, all_utxos: &UnspentOutputsPool, bigger_first: bool) -> Vec<OutputPointer> {
        self.keys()
            .sorted_by_key(|o| {
                let value = all_utxos.get(o).map(|vt| i128::from(vt.value)).unwrap_or(0);

                if bigger_first {
                    -value
                } else {
                    value
                }
            })
            .cloned()
            .collect()
    }

    /// Get balance
    pub fn get_balance(&self, all_utxos: &UnspentOutputsPool) -> u64 {
        self.keys()
            .map(|o| {
                all_utxos
                    .get(o)
                    .expect("mismatch between OwnUnspentOutputsPool and UnspentOutputsPool")
                    .value
            })
            .sum()
    }
}

/// Struct that keeps the unspent outputs pool and the own unspent outputs pool
#[derive(Debug)]
pub struct NodeUtxosRef<'a> {
    /// OutputPointers of all UTXOs with the ValueTransferOutput information
    pub all_utxos: &'a UnspentOutputsPool,
    /// OutputPointers of our own UTXOs
    pub own_utxos: &'a OwnUnspentOutputsPool,
    /// Node address
    pub pkh: PublicKeyHash,
}

impl<'a> OutputsCollection for NodeUtxosRef<'a> {
    fn sort_by(&self, strategy: &UtxoSelectionStrategy) -> Vec<OutputPointer> {
        if !strategy.allows_from(&self.pkh) {
            return vec![];
        }

        match strategy {
            UtxoSelectionStrategy::BigFirst { from: _ } => {
                self.own_utxos.sort(self.all_utxos, true)
            }
            UtxoSelectionStrategy::SmallFirst { from: _ } => {
                self.own_utxos.sort(self.all_utxos, false)
            }
            UtxoSelectionStrategy::Random { from: _ } => {
                self.own_utxos.iter().map(|(o, _ts)| o.clone()).collect()
            }
        }
    }

    fn get_time_lock(&self, outptr: &OutputPointer) -> Option<u64> {
        let time_lock = self.all_utxos.get(outptr).map(|vto| vto.time_lock);
        let time_lock_by_used = self.own_utxos.get(outptr).copied();

        // The most restrictive time_lock will be used to avoid UTXOs during a transaction creation
        match (time_lock, time_lock_by_used) {
            (Some(a), Some(b)) => Some(std::cmp::max(a, b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            _ => None,
        }
    }

    fn get_value(&self, outptr: &OutputPointer) -> Option<u64> {
        self.all_utxos.get(outptr).map(|vto| vto.value)
    }

    fn get_included_block_number(&self, outptr: &OutputPointer) -> Option<u32> {
        self.all_utxos.included_in_block_number(outptr)
    }

    fn set_used_output_pointer(&mut self, _inputs: &[Input], _ts: u64) {
        log::warn!("Mutable operations not supported in `NodeUtxosRef`, use `NodeUtxos` instead");
    }
}

/// Struct that keeps the unspent outputs pool and the own unspent outputs pool
#[derive(Debug)]
pub struct NodeUtxos<'a> {
    /// OutputPointers of all UTXOs with the ValueTransferOutput information
    pub all_utxos: &'a UnspentOutputsPool,
    /// OutputPointers of our own UTXOs
    pub own_utxos: &'a mut OwnUnspentOutputsPool,
    /// Node address
    pub pkh: PublicKeyHash,
}

impl NodeUtxos<'_> {
    pub fn as_ref(&self) -> NodeUtxosRef {
        NodeUtxosRef {
            all_utxos: self.all_utxos,
            own_utxos: self.own_utxos,
            pkh: self.pkh,
        }
    }
}

impl<'a> OutputsCollection for NodeUtxos<'a> {
    fn sort_by(&self, strategy: &UtxoSelectionStrategy) -> Vec<OutputPointer> {
        self.as_ref().sort_by(strategy)
    }

    fn get_time_lock(&self, outptr: &OutputPointer) -> Option<u64> {
        self.as_ref().get_time_lock(outptr)
    }

    fn get_value(&self, outptr: &OutputPointer) -> Option<u64> {
        self.as_ref().get_value(outptr)
    }

    fn get_included_block_number(&self, outptr: &OutputPointer) -> Option<u32> {
        self.as_ref().get_included_block_number(outptr)
    }

    fn set_used_output_pointer(&mut self, inputs: &[Input], ts: u64) {
        for input in inputs {
            let current_ts = self.own_utxos.get_mut(input.output_pointer()).unwrap();
            *current_ts = ts;
        }
    }
}

/// Strategy to sort our own unspent outputs pool
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum UtxoSelectionStrategy {
    Random { from: Option<PublicKeyHash> },
    BigFirst { from: Option<PublicKeyHash> },
    SmallFirst { from: Option<PublicKeyHash> },
}

impl Default for UtxoSelectionStrategy {
    fn default() -> Self {
        UtxoSelectionStrategy::Random { from: None }
    }
}

impl UtxoSelectionStrategy {
    /// Returns the address that will be used for the inputs of the transaction,
    /// or None if any address is allowed
    // Named get_from instead of from to avoid confusion with From trait
    pub fn get_from(&self) -> &Option<PublicKeyHash> {
        match self {
            UtxoSelectionStrategy::Random { from } => from,
            UtxoSelectionStrategy::BigFirst { from } => from,
            UtxoSelectionStrategy::SmallFirst { from } => from,
        }
    }

    /// Returns the address that will be used for the inputs of the transaction,
    /// or None if any address is allowed
    pub fn get_from_mut(&mut self) -> &mut Option<PublicKeyHash> {
        match self {
            UtxoSelectionStrategy::Random { from } => from,
            UtxoSelectionStrategy::BigFirst { from } => from,
            UtxoSelectionStrategy::SmallFirst { from } => from,
        }
    }

    /// Returns true if this strategy allows UTXOs from this address
    pub fn allows_from(&self, address: &PublicKeyHash) -> bool {
        match self.get_from() {
            None => true,
            Some(from) => from == address,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UtxoMetadata {
    pub output_pointer: OutputPointer,
    pub value: u64,
    pub timelock: u64,
    pub utxo_mature: bool,
}

/// Information about our own UTXOs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UtxoInfo {
    /// Vector of OutputPointer with their values, time_locks and if it is ready for collateral
    pub utxos: Vec<UtxoMetadata>,
    /// Minimum collateral from consensus constants
    pub collateral_min: u64,
}

#[allow(clippy::cast_sign_loss)]
fn create_utxo_metadata(
    vto: &ValueTransferOutput,
    o: &OutputPointer,
    all_utxos: &UnspentOutputsPool,
    block_number_limit: u32,
) -> UtxoMetadata {
    let now = get_timestamp() as u64;
    let timelock = if vto.time_lock >= now {
        vto.time_lock
    } else {
        0
    };
    let utxo_mature: bool = all_utxos.included_in_block_number(o).unwrap() <= block_number_limit;

    UtxoMetadata {
        output_pointer: o.clone(),
        value: vto.value,
        timelock,
        utxo_mature,
    }
}

/// Get Utxo Information
#[allow(clippy::cast_sign_loss)]
pub fn get_utxo_info(
    pkh: Option<PublicKeyHash>,
    own_utxos: &OwnUnspentOutputsPool,
    all_utxos: &UnspentOutputsPool,
    collateral_min: u64,
    block_number_limit: u32,
) -> UtxoInfo {
    let utxos = if let Some(pkh) = pkh {
        all_utxos
            .iter()
            .filter_map(|(o, (vto, _))| {
                if vto.pkh == pkh {
                    Some(create_utxo_metadata(
                        &vto,
                        &o,
                        all_utxos,
                        block_number_limit,
                    ))
                } else {
                    None
                }
            })
            .collect()
    } else {
        // Read your own UtxoInfo is cheaper than from other pkhs
        own_utxos
            .iter()
            .filter_map(|(o, _)| {
                all_utxos
                    .get(o)
                    .as_ref()
                    .map(|vto| create_utxo_metadata(vto, o, all_utxos, block_number_limit))
            })
            .collect()
    };

    UtxoInfo {
        utxos,
        collateral_min,
    }
}

/// Diffs to apply to an utxo set. This type does not contains a
/// reference to the original utxo set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Diff {
    utxos_to_add: HashMap<OutputPointer, (ValueTransferOutput, u32)>,
    utxos_to_remove: HashSet<OutputPointer>,
}

impl Diff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(mut self, utxo_set: &mut UnspentOutputsPool) {
        for (output_pointer, (output, block_number)) in self.utxos_to_add.drain() {
            utxo_set.insert(output_pointer, output, block_number);
        }

        for output_pointer in self.utxos_to_remove.iter() {
            utxo_set.remove(output_pointer);
        }
    }
    /// Iterate over all the utxos_to_add and utxos_to_remove while applying a function.
    ///
    /// Any shared mutable state used by `F1` and `F2` can be used as the first argument:
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use witnet_data_structures::utxo_pool::Diff;
    ///
    /// let diff = Diff::new();
    /// let mut hashmap = HashMap::new();
    /// diff.visit(&mut hashmap, |hashmap, output_pointer, output| {
    ///     hashmap.insert(output_pointer.clone(), output.clone());
    /// }, |hashmap, output_pointer| {
    ///     hashmap.remove(output_pointer);
    /// });
    /// ```
    pub fn visit<A, F1, F2>(&self, args: &mut A, mut fn_add: F1, mut fn_remove: F2)
    where
        F1: FnMut(&mut A, &OutputPointer, &ValueTransferOutput),
        F2: FnMut(&mut A, &OutputPointer),
    {
        for (output_pointer, (output, _)) in self.utxos_to_add.iter() {
            fn_add(args, output_pointer, output);
        }

        for output_pointer in self.utxos_to_remove.iter() {
            fn_remove(args, output_pointer);
        }
    }
}

/// Contains a reference to an UnspentOutputsPool plus subsequent
/// insertions and deletions to performed on that pool.
/// Use `.take_diff()` to obtain an instance of the `Diff` type.
pub struct UtxoDiff<'a> {
    diff: Diff,
    utxo_set: &'a UnspentOutputsPool,
    block_number: u32,
}

impl<'a> UtxoDiff<'a> {
    /// Create a new UtxoDiff without additional insertions or deletions
    pub fn new(utxo_set: &'a UnspentOutputsPool, block_number: u32) -> Self {
        UtxoDiff {
            utxo_set,
            diff: Diff::new(),
            block_number,
        }
    }

    /// Record an insertion to perform on the utxo set
    pub fn insert_utxo(
        &mut self,
        output_pointer: OutputPointer,
        output: ValueTransferOutput,
        block_number: Option<u32>,
    ) {
        self.diff.utxos_to_add.insert(
            output_pointer,
            (output, block_number.unwrap_or(self.block_number)),
        );
    }

    /// Record a deletion to perform on the utxo set
    pub fn remove_utxo(&mut self, output_pointer: OutputPointer) {
        if self.diff.utxos_to_add.remove(&output_pointer).is_none() {
            self.diff.utxos_to_remove.insert(output_pointer);
        }
    }

    /// Get an utxo from the original utxo set or one that has been
    /// recorded as inserted later. If the same utxo has been recorded
    /// as removed, None will be returned.
    pub fn get(&self, output_pointer: &OutputPointer) -> Option<ValueTransferOutput> {
        self.utxo_set
            .get(output_pointer)
            .or_else(|| {
                self.diff
                    .utxos_to_add
                    .get(output_pointer)
                    .map(|(vt, _block_number)| vt.clone())
            })
            .and_then(|output| {
                if self.diff.utxos_to_remove.contains(output_pointer) {
                    None
                } else {
                    Some(output)
                }
            })
    }

    /// Consumes the UtxoDiff and returns only the diffs, without the
    /// reference to the utxo set.
    pub fn take_diff(self) -> Diff {
        self.diff
    }

    /// Returns the number of the block that included the transaction referenced
    /// by this OutputPointer. The difference between that number and the
    /// current number of consolidated blocks is the "collateral age".
    pub fn included_in_block_number(&self, output_pointer: &OutputPointer) -> Option<Epoch> {
        self.utxo_set
            .included_in_block_number(output_pointer)
            .and_then(|output| {
                if self.diff.utxos_to_remove.contains(output_pointer) {
                    None
                } else {
                    Some(output)
                }
            })
    }
}

/// Old version of Unspent Outputs Pool
/// Needed for database migrations
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OldUnspentOutputsPool {
    /// Map of output pointer to a tuple of:
    /// * Value transfer output
    /// * The number of the block that included the transaction
    ///   (how many blocks were consolidated before this one).
    map: HashMap<OutputPointer, (ValueTransferOutput, u32)>,
}

impl OldUnspentOutputsPool {
    /// Returns whether there are any UTXOs in this data structure.
    /// After the migration, this will always return true.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
