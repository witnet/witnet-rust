use crate::chain::{Epoch, OutputPointer, PublicKeyHash, ValueTransferOutput};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use witnet_storage::storage::Storage;
use witnet_util::timestamp::get_timestamp;

/// Unspent Outputs Pool
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct UnspentOutputsPool {
    // Map of output pointer to a tuple of:
    // * Value transfer output
    // * The number of the block that included the transaction
    //   (how many blocks were consolidated before this one).
    //map: HashMap<OutputPointer, (ValueTransferOutput, u32)>,
    /// Set of unspent outputs. This is used to implement reorganizations: UTXOs are never removed
    /// from the database, they are only removed from this in-memory set.
    set: HashSet<OutputPointer>,
    /// Database
    #[serde(skip)]
    pub db: Option<Arc<dyn Storage + Send + Sync>>,
}

impl fmt::Debug for UnspentOutputsPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UnspentOutputsPool")
    }
}

impl PartialEq for UnspentOutputsPool {
    fn eq(&self, other: &Self) -> bool {
        self.set == other.set
    }
}

impl UnspentOutputsPool {
    pub fn get(&self, k: &OutputPointer) -> Option<ValueTransferOutput> {
        self.get_map(k).map(|(vt, _n)| vt)
    }

    pub fn get_map(&self, k: &OutputPointer) -> Option<(ValueTransferOutput, u32)> {
        if !self.set.contains(k) {
            return None;
        }

        let key_string = format!("UTXO-{}", k);
        log::debug!("GET {}", key_string);
        self.db
            .as_ref()
            .expect("no db")
            .get(key_string.as_bytes())
            .expect("db fail")
            .map(|bytes| {
                bincode::deserialize::<(ValueTransferOutput, u32)>(&bytes).expect("bincode fail")
            })
    }

    pub fn contains_key(&self, k: &OutputPointer) -> bool {
        self.set.contains(k)
    }

    pub fn insert(
        &mut self,
        k: OutputPointer,
        v: ValueTransferOutput,
        block_number: u32,
    ) -> Option<(ValueTransferOutput, u32)> {
        let old_vto = self.get_map(&k);

        let key_string = format!("UTXO-{}", k);
        log::debug!("PUT {}", key_string);
        self.db
            .as_mut()
            .unwrap()
            .put(
                key_string.into_bytes(),
                bincode::serialize(&(v, block_number)).unwrap(),
            )
            .unwrap();
        self.set.insert(k);

        old_vto
    }

    pub fn remove(&mut self, k: &OutputPointer) -> Option<(ValueTransferOutput, u32)> {
        let old_vto = self.get_map(&k);

        // Never remove UTXOs from db
        /*
        let key_string = format!("UTXO-{}", k);
        log::debug!("REMOVE {}", key_string);
        self.db
            .as_mut()
            .unwrap()
            .delete(key_string.as_bytes())
            .unwrap();
         */
        self.set.remove(k);

        old_vto
    }

    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (OutputPointer, (ValueTransferOutput, u32))> + '_ {
        self.set.iter().map(move |output_pointer| {
            (
                output_pointer.clone(),
                self.get_map(output_pointer).unwrap(),
            )
        })
    }

    pub fn db_iter(
        &self,
    ) -> impl Iterator<Item = (OutputPointer, (ValueTransferOutput, u32))> + '_ {
        self.db
            .as_ref()
            .unwrap()
            .prefix_iterator(b"UTXO-")
            .unwrap()
            .map(|(k, v)| {
                let key_string = String::from_utf8(k).unwrap();
                let output_pointer_str = key_string.strip_prefix("UTXO-").unwrap();
                let key = OutputPointer::from_str(output_pointer_str).unwrap();
                let value = bincode::deserialize(&v).unwrap();

                (key, value)
            })
    }

    /// Returns the number of the block that included the transaction referenced
    /// by this OutputPointer. The difference between that number and the
    /// current number of consolidated blocks is the "collateral age".
    pub fn included_in_block_number(&self, k: &OutputPointer) -> Option<u32> {
        self.get_map(k).map(|(_vt, n)| n)
    }

    /// Create a in-memory UnspentOutputsPool, used in tests
    pub fn in_memory() -> Self {
        let mut utxo_set = Self::default();
        utxo_set.db = Some(Arc::new(
            witnet_storage::backends::hashmap::Backend::default(),
        ));

        utxo_set
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
}

/// Strategy to sort our own unspent outputs pool
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum UtxoSelectionStrategy {
    Random,
    BigFirst,
    SmallFirst,
}

impl Default for UtxoSelectionStrategy {
    fn default() -> Self {
        UtxoSelectionStrategy::Random
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
#[derive(Debug)]
pub struct Diff {
    utxos_to_add: HashMap<OutputPointer, (ValueTransferOutput, u32)>,
    utxos_to_remove: HashSet<OutputPointer>,
    utxos_to_remove_dr: Vec<OutputPointer>,
    block_number: u32,
}

impl Diff {
    pub fn new(block_number: u32) -> Self {
        Self {
            utxos_to_add: Default::default(),
            utxos_to_remove: Default::default(),
            utxos_to_remove_dr: vec![],
            block_number,
        }
    }

    pub fn apply(mut self, utxo_set: &mut UnspentOutputsPool) {
        for (output_pointer, (output, block_number)) in self.utxos_to_add.drain() {
            utxo_set.insert(output_pointer, output, block_number);
        }

        for output_pointer in self.utxos_to_remove.iter() {
            utxo_set.remove(output_pointer);
        }

        for output_pointer in self.utxos_to_remove_dr.iter() {
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
    /// let block_number = 0;
    /// let diff = Diff::new(block_number);
    /// let mut hashmap = HashMap::new();
    /// diff.visit(&mut hashmap, |hashmap, output_pointer, output| {
    ///     hashmap.insert(output_pointer.clone(), output.clone());
    /// }, |hashmap, output_pointer| {
    ///     hashmap.remove(output_pointer);
    /// });
    /// ```
    pub fn visit<A, F1, F2>(&self, args: &mut A, fn_add: F1, fn_remove: F2)
    where
        F1: Fn(&mut A, &OutputPointer, &ValueTransferOutput),
        F2: Fn(&mut A, &OutputPointer),
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
}

impl<'a> UtxoDiff<'a> {
    /// Create a new UtxoDiff without additional insertions or deletions
    pub fn new(utxo_set: &'a UnspentOutputsPool, block_number: u32) -> Self {
        UtxoDiff {
            utxo_set,
            diff: Diff::new(block_number),
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
            (output, block_number.unwrap_or(self.diff.block_number)),
        );
    }

    /// Record a deletion to perform on the utxo set
    pub fn remove_utxo(&mut self, output_pointer: OutputPointer) {
        if self.diff.utxos_to_add.remove(&output_pointer).is_none() {
            self.diff.utxos_to_remove.insert(output_pointer);
        }
    }

    /// Record a deletion to perform on the utxo set but that it
    /// doesn't count when getting an utxo with `get` method.
    pub fn remove_utxo_dr(&mut self, output_pointer: OutputPointer) {
        self.diff.utxos_to_remove_dr.push(output_pointer);
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
