use std::{cmp, collections::VecDeque, convert, fmt, ops, time::Duration};

use itertools::Itertools;

use failure::Fail;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use witnet_util::timestamp::seconds_to_human_string;

use crate::{
    transaction::Transaction,
    types::visitor::{StatefulVisitor, Visitor},
    wit::Wit,
};
use std::num::TryFromIntError;

// Assuming no missing epochs, this will keep track of priority used by transactions in the last 24
// hours. This is rounded up to the closest `2 ^ n - 1` because `PriorityEngine` uses a `VecDeque`
// under the hood.
const DEFAULT_QUEUE_CAPACITY_EPOCHS: usize = 2047;
// The minimum number of epochs that we need to track before estimating transaction priority
const MINIMUM_TRACKED_EPOCHS: u32 = 20;

/// Keeps track of fees being paid by transactions included in recent blocks, and provides methods
/// for estimating sensible priority values for future transactions.
///
/// This supports _value transfer transactions_ (VTTs) as well as _data requests_ (DRs).
///
/// All across this module, fees are always expressed in their relative form (nanowits per weight
/// unit), aka "transaction priority".
#[derive(Clone, Eq, PartialEq)]
pub struct PriorityEngine {
    /// Soft-capped capacity for the inner priorities queue.
    capacity: usize,
    /// Queue for storing fees info for recent transactions.
    priorities: VecDeque<Priorities>,
}

impl PriorityEngine {
    /// Retrieve the inner fee information as a vector.
    pub fn as_vec(&self) -> Vec<Priorities> {
        self.priorities.iter().rev().cloned().collect_vec()
    }

    /// Provide suggestions for sensible transaction priority values, together with their expected
    /// time-to-block in epochs.
    ///
    /// This is only a first approach to an estimation algorithm. There is abundant prior art about
    /// fee estimation in other blockchains. We might revisit this once we collect more insights
    /// about our fees market and user feedback.
    pub fn estimate_priority(
        &self,
        seconds_per_epoch: Duration,
    ) -> Result<PrioritiesEstimate, PriorityError> {
        // Short-circuit if there are too few tracked epochs for an accurate estimation.
        let len = u32::try_from(self.priorities.len()).unwrap_or(u32::MAX);
        if len < MINIMUM_TRACKED_EPOCHS {
            Err(PriorityError::NotEnoughSampledEpochs {
                current: len,
                required: MINIMUM_TRACKED_EPOCHS,
                wait_minutes: (MINIMUM_TRACKED_EPOCHS - len)
                    * u32::try_from(seconds_per_epoch.as_secs())?
                    / 60
                    + 1,
            })?;
        }

        Ok(strategies::target_minutes(
            self.priorities.iter(),
            [360, 60, 15, 5, 1],
            u16::try_from(seconds_per_epoch.as_secs())?,
        ))
    }

    /// Creates a new engine with the default capacity from a vector of `Priorities`.
    ///
    /// This assumes that the vector is ordered from newest to oldest.
    pub fn from_vec(priorities: Vec<Priorities>) -> Self {
        Self::from_vec_with_capacity(priorities, DEFAULT_QUEUE_CAPACITY_EPOCHS)
    }

    /// Creates a new engine with a custom queue capacity from a vector of `Priorities`.
    ///
    /// This assumes that the vector is ordered from newest to oldest.
    pub fn from_vec_with_capacity(priorities: Vec<Priorities>, capacity: usize) -> Self {
        // Create a new queue with the desired capacity
        let mut fees = VecDeque::with_capacity(capacity);
        // Push as many elements from the input as they can fit in the queue
        priorities.into_iter().take(capacity).for_each(|entry| {
            fees.push_back(entry);
        });

        Self {
            capacity,
            priorities: fees,
        }
    }

    /// Get the entry at a certain position, if an item at that position exists, or None otherwise.
    #[cfg(test)]
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Priorities> {
        self.priorities.get(index)
    }

    /// Push a new `Priorities` entry into the engine.
    #[inline]
    pub fn push_priorities(&mut self, priorities: Priorities) {
        log::trace!("Pushing new transaction priorities entry: {:?}", priorities);
        // If we hit the capacity limit, pop from the back first so the queue does not grow
        if self.priorities.len() == self.capacity {
            self.priorities.pop_back();
        }
        self.priorities.push_front(priorities);
    }

    /// Create a new engine of a certain queue capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            priorities: VecDeque::with_capacity(capacity),
        }
    }
}

/// Different errors that the `PriorityEngine` can produce.
#[derive(Debug, Eq, Fail, PartialEq)]
pub enum PriorityError {
    #[fail(display = "Conversion error: {}", _0)]
    Conversion(String),
    /// The number of sampled epochs in the engine is not enough for providing a reliable estimate.
    #[fail(
        display = "The node has only sampled priority from {} blocks but at least {} are needed to provide a reliable priority estimate. Please retry after {} minutes.",
        current, required, wait_minutes
    )]
    NotEnoughSampledEpochs {
        current: u32,
        required: u32,
        wait_minutes: u32,
    },
}

impl From<std::num::TryFromIntError> for PriorityError {
    fn from(error: TryFromIntError) -> Self {
        Self::Conversion(error.to_string())
    }
}

impl fmt::Debug for PriorityEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fees = self
            .priorities
            .iter()
            .enumerate()
            .map(|(i, fees)| format!("{:>4} → {:?}", i, fees))
            .join("\n");

        write!(
            f,
            "[PriorityEngine] There is priority information for {} epochs:\n{}",
            self.priorities.len(),
            fees,
        )
    }
}

impl Default for PriorityEngine {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY_EPOCHS)
    }
}

/// Conveniently wraps a priority value with sub-nanoWit precision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Priority(OrderedFloat<f64>);

impl Priority {
    #[inline]
    pub fn as_f64(&self) -> f64 {
        self.0.into_inner()
    }

    /// The default priority for tier "High".
    #[inline]
    pub fn default_high() -> Self {
        Self::from(0.4)
    }

    /// The default priority for tier "Low".
    #[inline]
    pub fn default_low() -> Self {
        Self::from(0.2)
    }

    /// The default priority for tier "Medium".
    #[inline]
    pub fn default_medium() -> Self {
        Self::from(0.3)
    }

    /// The default priority for tier "Opulent".
    #[inline]
    pub fn default_opulent() -> Self {
        Self::from(0.5)
    }

    /// The default priority for tier "Stinky".
    #[inline]
    pub fn default_stinky() -> Self {
        Self::from(0.1)
    }

    /// Derive fee from priority and weight.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    pub fn derive_fee_wit(&self, weight: u32) -> Wit {
        Wit::from_nanowits((self.0.into_inner() * f64::from(weight)) as u64)
    }

    /// Constructs a Priority from a transaction fee and weight.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn from_absolute_fee_weight(fee: u64, weight: u32) -> Self {
        Self::from(fee as f64 / f64::from(weight))
    }
}

/// Conveniently create a Priority value from an OrderedFloat<f64> value.
impl convert::From<OrderedFloat<f64>> for Priority {
    #[inline]
    fn from(input: OrderedFloat<f64>) -> Self {
        Self(input)
    }
}

/// Conveniently create a Priority value from an f64 value.
impl convert::From<f64> for Priority {
    #[inline]
    fn from(input: f64) -> Self {
        Self::from(OrderedFloat(input))
    }
}

/// Conveniently create a Priority value from a u64 value.
impl convert::From<u64> for Priority {
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn from(input: u64) -> Self {
        Self::from(input as f64)
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}", self.0)
    }
}

impl cmp::Ord for Priority {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl cmp::PartialEq<u64> for Priority {
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn eq(&self, other: &u64) -> bool {
        self.as_f64().eq(&(*other as f64))
    }
}

impl cmp::PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Allow adding two Priority values together.
impl ops::Add for Priority {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

/// Allow `+=` on `Priority`.
impl ops::AddAssign for Priority {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = ops::Add::add(*self, rhs);
    }
}

/// Allow dividing `Priority` values by `u64` values.
impl ops::Div<u64> for Priority {
    type Output = Self;

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn div(self, rhs: u64) -> Self::Output {
        Self(self.0 / rhs as f64)
    }
}

/// Allow multiplying `Priority` values by `u64` values.
impl ops::Mul<u64> for Priority {
    type Output = Self;

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn mul(self, rhs: u64) -> Self::Output {
        self.mul(rhs as f64)
    }
}

/// Allow multiplying `Priority` values by `u64` values.
impl ops::Mul<f64> for Priority {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: f64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

/// Allow substraction of two Priority values together.
impl ops::Sub for Priority {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl<'de> Deserialize<'de> for Priority {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        f64::deserialize(deserializer).map(Self::from)
    }
}

impl Serialize for Priority {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        Serialize::serialize(&self.0.into_inner(), serializer)
    }
}

impl num_traits::Zero for Priority {
    fn zero() -> Self {
        Self(OrderedFloat::from(0.0))
    }

    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl Default for Priority {
    fn default() -> Self {
        <Self as num_traits::Zero>::zero()
    }
}

/// Type for each of the entries in `FeesEngine`.
///
/// Fees are always expressed in their relative form (nanowits per weight unit), aka "transaction
/// priority".
#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Priorities {
    /// The highest priority used by data request transactions in a block.
    pub drt_highest: Priority,
    /// The lowest priority used by data requests transactions in a block.
    pub drt_lowest: Option<Priority>,
    /// The highest priority used by value transfer transactions in a block.
    pub vtt_highest: Priority,
    /// The lowest priority used by data requests transactions in a block.
    pub vtt_lowest: Option<Priority>,
    /// The highest priority used by stake transactions in a block.
    pub st_highest: Priority,
    /// The lowest priority used by stake transactions in a block.
    pub st_lowest: Option<Priority>,
    /// The highest priority used by unstake transactions in a block.
    pub ut_highest: Priority,
    /// The lowest priority used by unstake transactions in a block.
    pub ut_lowest: Option<Priority>,
}

impl Priorities {
    /// Process the priority of a data request transaction, and update the highest and lowest values
    /// accordingly, if the provided value is higher or lower than the previously set values.
    #[inline]
    pub fn digest_drt_priority(&mut self, priority: Priority) {
        // Update highest
        if priority > self.drt_highest {
            self.drt_highest = priority;
        }
        // Update lowest
        if let Some(drt_lowest) = &self.drt_lowest {
            if &priority < drt_lowest {
                self.drt_lowest = Some(priority);
            }
        } else if priority != 0 {
            self.drt_lowest = Some(priority);
        }
    }

    /// Process the priority of a value transfer transaction, and update the highest and lowest
    /// values accordingly, if the provided value is higher or lower than the previously set values.
    #[inline]
    pub fn digest_vtt_priority(&mut self, priority: Priority) {
        // Update highest
        if priority > self.vtt_highest {
            self.vtt_highest = priority;
        }
        // Update lowest
        if let Some(vtt_lowest) = &self.vtt_lowest {
            if &priority < vtt_lowest {
                self.vtt_lowest = Some(priority);
            }
        } else if priority != 0 {
            self.vtt_lowest = Some(priority);
        }
    }

    /// Process the priority of a stake transaction, and update the highest and lowest
    /// values accordingly, if the provided value is higher or lower than the previously set values.
    #[inline]
    pub fn digest_st_priority(&mut self, priority: Priority) {
        // Update highest
        if priority > self.st_highest {
            self.st_highest = priority;
        }
        // Update lowest
        if let Some(st_lowest) = &self.st_lowest {
            if &priority < st_lowest {
                self.st_lowest = Some(priority);
            }
        } else if priority != 0 {
            self.st_lowest = Some(priority);
        }
    }

    /// Process the priority of a unstake transaction, and update the highest and lowest
    /// values accordingly, if the provided value is higher or lower than the previously set values.
    #[inline]
    pub fn digest_ut_priority(&mut self, priority: Priority) {
        // Update highest
        if priority > self.ut_highest {
            self.ut_highest = priority;
        }
        // Update lowest
        if let Some(ut_lowest) = &self.ut_lowest {
            if &priority < ut_lowest {
                self.ut_lowest = Some(priority);
            }
        } else if priority != 0 {
            self.ut_lowest = Some(priority);
        }
    }
}

impl fmt::Debug for Priorities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[DRT](↑{:<10}, ↓{:<10}) | [VTT](↑{:<10}, ↓{:<10})",
            self.drt_highest,
            self.drt_lowest.unwrap_or_default(),
            self.vtt_highest,
            self.vtt_lowest.unwrap_or_default()
        )
    }
}

/// A visitor for `Priorities` values.
///
/// To be used with `witnet_validations::validations::validate_block_transactions`.
#[derive(Default)]
pub struct PriorityVisitor(Priorities);

impl Visitor for PriorityVisitor {
    type Visitable = (Transaction, /* fee */ u64, /* weight */ u32);

    fn visit(&mut self, (transaction, fee, weight): &Self::Visitable) {
        match transaction {
            Transaction::DataRequest(_) => {
                self.0
                    .digest_drt_priority(Priority::from_absolute_fee_weight(*fee, *weight));
            }
            Transaction::ValueTransfer(_) => {
                self.0
                    .digest_vtt_priority(Priority::from_absolute_fee_weight(*fee, *weight));
            }
            Transaction::Stake(_) => {
                self.0
                    .digest_st_priority(Priority::from_absolute_fee_weight(*fee, *weight));
            }
            Transaction::Unstake(_) => {
                self.0
                    .digest_ut_priority(Priority::from_absolute_fee_weight(*fee, *weight));
            }
            _ => (),
        }
    }
}

impl StatefulVisitor for PriorityVisitor {
    type State = Priorities;

    fn take_state(self) -> Self::State {
        self.0
    }
}

/// A whole set of estimates for priority of DRT and VTT transactions.
///
/// Each estimate contains values for 5 different tiers of priority:
/// - Stinky
/// - low
/// - Medium
/// - High
/// - Opulent
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrioritiesEstimate {
    pub drt_stinky: PriorityEstimate,
    pub drt_low: PriorityEstimate,
    pub drt_medium: PriorityEstimate,
    pub drt_high: PriorityEstimate,
    pub drt_opulent: PriorityEstimate,
    pub vtt_stinky: PriorityEstimate,
    pub vtt_low: PriorityEstimate,
    pub vtt_medium: PriorityEstimate,
    pub vtt_high: PriorityEstimate,
    pub vtt_opulent: PriorityEstimate,
    pub st_stinky: PriorityEstimate,
    pub st_low: PriorityEstimate,
    pub st_medium: PriorityEstimate,
    pub st_high: PriorityEstimate,
    pub st_opulent: PriorityEstimate,
    pub ut_stinky: PriorityEstimate,
    pub ut_low: PriorityEstimate,
    pub ut_medium: PriorityEstimate,
    pub ut_high: PriorityEstimate,
    pub ut_opulent: PriorityEstimate,
}

impl fmt::Display for PrioritiesEstimate {
    #[allow(clippy::to_string_in_format_args)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"╔══════════════════════════════════════════════════════════╗
║ TRANSACTION PRIORITY ESTIMATION REPORT                   ║
╠══════════════════════════════════════════════════════════╣
║ Data request transactions                                ║
╟──────────┬───────────────┬───────────────────────────────║
║     Tier │ Time-to-block │ Priority                      ║
╟──────────┼───────────────┼───────────────────────────────║
║   Stinky │ {:>13} │ {:<28}  ║
║      Low │ {:>13} │ {:<28}  ║
║   Medium │ {:>13} │ {:<28}  ║
║     High │ {:>13} │ {:<28}  ║
║  Opulent │ {:>13} │ {:<28}  ║
╠══════════════════════════════════════════════════════════╣
║ Value transfer transactions                              ║
╟──────────┬───────────────┬───────────────────────────────║
║     Tier │ Time-to-block │ Priority                      ║
╟──────────┼───────────────┼───────────────────────────────║
║   Stinky │ {:>13} │ {:<28}  ║
║      Low │ {:>13} │ {:<28}  ║
║   Medium │ {:>13} │ {:<28}  ║
║     High │ {:>13} │ {:<28}  ║
║  Opulent │ {:>13} │ {:<28}  ║
╠══════════════════════════════════════════════════════════╣
║ Stake transactions                                       ║
╟──────────┬───────────────┬───────────────────────────────║
║     Tier │ Time-to-block │ Priority                      ║
╟──────────┼───────────────┼───────────────────────────────║
║   Stinky │ {:>13} │ {:<28}  ║
║      Low │ {:>13} │ {:<28}  ║
║   Medium │ {:>13} │ {:<28}  ║
║     High │ {:>13} │ {:<28}  ║
║  Opulent │ {:>13} │ {:<28}  ║

╠══════════════════════════════════════════════════════════╣
║ Unstake transactions                                     ║
╟──────────┬───────────────┬───────────────────────────────║
║     Tier │ Time-to-block │ Priority                      ║
╟──────────┼───────────────┼───────────────────────────────║
║   Stinky │ {:>13} │ {:<28}  ║
║      Low │ {:>13} │ {:<28}  ║
║   Medium │ {:>13} │ {:<28}  ║
║     High │ {:>13} │ {:<28}  ║
║  Opulent │ {:>13} │ {:<28}  ║
╚══════════════════════════════════════════════════════════╝"#,
            // Believe it or not, these `to_string` are needed for proper formatting, hence the
            // clippy allow directive above.
            self.drt_stinky.time_to_block.to_string(),
            self.drt_stinky.priority.to_string(),
            self.drt_low.time_to_block.to_string(),
            self.drt_low.priority.to_string(),
            self.drt_medium.time_to_block.to_string(),
            self.drt_medium.priority.to_string(),
            self.drt_high.time_to_block.to_string(),
            self.drt_high.priority.to_string(),
            self.drt_opulent.time_to_block.to_string(),
            self.drt_opulent.priority.to_string(),
            self.vtt_stinky.time_to_block.to_string(),
            self.vtt_stinky.priority.to_string(),
            self.vtt_low.time_to_block.to_string(),
            self.vtt_low.priority.to_string(),
            self.vtt_medium.time_to_block.to_string(),
            self.vtt_medium.priority.to_string(),
            self.vtt_high.time_to_block.to_string(),
            self.vtt_high.priority.to_string(),
            self.vtt_opulent.time_to_block.to_string(),
            self.vtt_opulent.priority.to_string(),
            self.st_stinky.time_to_block.to_string(),
            self.st_stinky.priority.to_string(),
            self.st_low.time_to_block.to_string(),
            self.st_low.priority.to_string(),
            self.st_medium.time_to_block.to_string(),
            self.st_medium.priority.to_string(),
            self.st_high.time_to_block.to_string(),
            self.st_high.priority.to_string(),
            self.st_opulent.time_to_block.to_string(),
            self.st_opulent.priority.to_string(),
            self.ut_stinky.time_to_block.to_string(),
            self.ut_stinky.priority.to_string(),
            self.ut_low.time_to_block.to_string(),
            self.ut_low.priority.to_string(),
            self.ut_medium.time_to_block.to_string(),
            self.ut_medium.priority.to_string(),
            self.ut_high.time_to_block.to_string(),
            self.ut_high.priority.to_string(),
            self.ut_opulent.time_to_block.to_string(),
            self.ut_opulent.priority.to_string(),
        )
    }
}

/// A estimate for priority and time-to-block.
///
/// Time-to-block states what is the expected time (in epochs) that it would take for a transaction
/// with this priority to be included into a block.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PriorityEstimate {
    pub priority: Priority,
    pub time_to_block: TimeToBlock,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimeToBlock(u64);

impl TimeToBlock {
    pub fn from_secs(secs: u64) -> Self {
        Self(secs)
    }
}

impl fmt::Display for TimeToBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&seconds_to_human_string(self.0))
    }
}

/// Updates an `Option` with the value from another candidate `Option` always that the old `Option`
/// was `None` or contained a value that was greater than the candidate.
///
/// This differs from `std::cmp::min` in the case that the existing `Option` is `None`, because
/// `impl Ord for Option` considers that `None` is always smaller than `Some(_)`:
///
/// ```
/// use witnet_data_structures::chain::priority::Priority;
///
/// let mut option = None;
/// option = std::cmp::min(option, Some(Priority::from(2337)));
/// option = std::cmp::min(option, Some(Priority::from(1337)));
/// option = std::cmp::min(option, Some(Priority::from(3337)));
///
/// assert_eq!(option, None);
/// ```
///
/// ```
/// use witnet_data_structures::chain::priority::{option_update_if_less_than, Priority};
///
/// let mut option = None;
/// option_update_if_less_than(&mut option, Some(Priority::from(2337)));
/// option_update_if_less_than(&mut option, Some(Priority::from(1337)));
/// option_update_if_less_than(&mut option, Some(Priority::from(3337)));
///
/// assert_eq!(option, Some(Priority::from(1337)))
/// ```
#[inline]
pub fn option_update_if_less_than(option: &mut Option<Priority>, candidate: Option<Priority>) {
    match (&candidate, &option) {
        (Some(new), Some(old)) if new < old => {
            *option = candidate;
        }
        (Some(_), None) => {
            *option = candidate;
        }
        _ => {}
    }
}

/// Priority estimation strategies. To be used with `PriorityEngine::estimate_priority`.
pub mod strategies {
    use super::*;

    /// A priority estimation strategy that receives a list of targetted time-to-blocks expressed in
    /// minutes and derives the priority from there by using a counter and enabling frequency
    /// queries on the counted items.
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn target_minutes<'a, I>(
        priorities: I,
        target_minutes: [u16; 5],
        seconds_per_epoch: u16,
    ) -> PrioritiesEstimate
    where
        I: IntoIterator<Item = &'a Priorities>,
        I::IntoIter: Clone + ExactSizeIterator,
    {
        // Make the priorities argument an iterator (if it was not already) and measure its length.
        let priorities = priorities.into_iter();
        let priorities_count = priorities.len() as f64;

        // Set the number of buckets used to ease the frequency counting
        let buckets_count = 50.0;

        // Create counters for measuring frequency of priorities separately for DRTs and VTTs.
        let mut drt_counter = counter::Counter::<u64>::new();
        let mut vtt_counter = counter::Counter::<u64>::new();
        let mut st_counter = counter::Counter::<u64>::new();
        let mut ut_counter = counter::Counter::<u64>::new();

        // This is a first pass over the priorities in the engine, just to find out the absolute
        // minimum and maximum among all the lowest priorities, i.e. what was the priority for the
        // less prioritized transaction in the blocks with the lowest and highest priority
        // requirements.
        let (drt_lowest_absolute, drt_highest_absolute, vtt_lowest_absolute, vtt_highest_absolute, st_lowest_absolute, st_highest_absolute, ut_lowest_absolute, ut_highest_absolute) =
            priorities.clone().fold(
                (f64::MAX, 0.0f64, f64::MAX, 0.0f64, f64::MAX, 0.0f64, f64::MAX, 0.0f64),
                |(drt_lowest, drt_highest, vtt_lowest, vtt_highest, st_lowest, st_highest, ut_lowest, ut_highest), priorities| {
                    let drt_min = priorities
                        .drt_lowest
                        .unwrap_or(priorities.drt_highest)
                        .as_f64();
                    let vtt_min = priorities
                        .vtt_lowest
                        .unwrap_or(priorities.vtt_highest)
                        .as_f64();
                    let st_min = priorities
                        .st_lowest
                        .unwrap_or(priorities.st_highest)
                        .as_f64();
                    let ut_min = priorities
                        .ut_lowest
                        .unwrap_or(priorities.ut_highest)
                        .as_f64();
                    (
                        drt_lowest.min(drt_min),
                        drt_highest.max(drt_min),
                        vtt_lowest.min(vtt_min),
                        vtt_highest.max(vtt_min),
                        st_lowest.min(st_min),
                        st_highest.max(st_min),
                        ut_lowest.min(ut_min),
                        ut_highest.max(ut_min),
                    )
                },
            );

        // The size of each bucket in nWitWu (nano wits per weight unit)
        let drt_buckets_size = (drt_highest_absolute - drt_lowest_absolute) / buckets_count;
        let vtt_buckets_size = (vtt_highest_absolute - vtt_lowest_absolute) / buckets_count;
        let st_buckets_size = (st_highest_absolute - st_lowest_absolute) / buckets_count;
        let ut_buckets_size = (ut_highest_absolute - ut_lowest_absolute) / buckets_count;

        // Now we are ready to map priorities to buckets and insert the bucket numbers into the
        // lossy counter.
        for Priorities {
            drt_highest,
            drt_lowest,
            vtt_highest,
            vtt_lowest,
            st_highest,
            st_lowest,
            ut_highest,
            ut_lowest,
        } in priorities
        {
            // This calculates the buckets in which the lowest values should be inserted.
            let drt_bucket = ((drt_lowest.unwrap_or(*drt_highest).as_f64() - drt_lowest_absolute)
                / drt_buckets_size)
                .round() as u64;
            let vtt_bucket = ((vtt_lowest.unwrap_or(*vtt_highest).as_f64() - vtt_lowest_absolute)
                / vtt_buckets_size)
                .round() as u64;
            let st_bucket = ((st_lowest.unwrap_or(*st_highest).as_f64() - st_lowest_absolute)
                / st_buckets_size)
                .round() as u64;
            let ut_bucket = ((ut_lowest.unwrap_or(*ut_highest).as_f64() - ut_lowest_absolute)
                / ut_buckets_size)
                .round() as u64;

            // For a perfect calculation, all values lower than the lowest bucket index
            // (representing the lowest fee should be inserted. However, we can get a good enough
            // approximation while saving almost half of the CPU time and memory by inserting only
            // the 10% closest values.
            // This however creates a little downward bias, specially on small datasets. This side
            // effect can be later counteracted by applying some adjustment coefficient that needs
            // to be inversely proportional to the number of priorities, and directly proportional
            // to the standard deviation of the lowest values.
            for bucket in drt_bucket * 90 / 100..=drt_bucket {
                drt_counter.add(bucket);
            }
            for bucket in vtt_bucket * 90 / 100..=vtt_bucket {
                vtt_counter.add(bucket);
            }
            for bucket in st_bucket * 90 / 100..=st_bucket {
                st_counter.add(bucket);
            }
            for bucket in ut_bucket * 90 / 100..=ut_bucket {
                ut_counter.add(bucket);
            }
        }

        // Make an estimation for each of the targeted time-to-blocks.
        let mut drt_priorities: Vec<Priority> = vec![];
        let mut vtt_priorities: Vec<Priority> = vec![];
        let mut st_priorities: Vec<Priority> = vec![];
        let mut ut_priorities: Vec<Priority> = vec![];

        for minutes in target_minutes.into_iter() {
            // Derive the frequency threshold for this targeted time-to-block.
            let epochs = f64::from(minutes) * 60.0 / f64::from(seconds_per_epoch);
            let epochs_freq = epochs / priorities_count;
            let threshold = epochs_freq;

            // Run the frequency query on the lossy counters.
            let drt_elements = drt_counter.query(threshold);
            let vtt_elements = vtt_counter.query(threshold);
            let st_elements = st_counter.query(threshold);
            let ut_elements = ut_counter.query(threshold);

            // The priority is calculated by reverting the buckets mapping performed before, i.e.
            // mapping the bucket index back to a priority value.
            let drt_bucket = drt_elements.max().unwrap_or_default() as f64;
            let drt_priority = Priority::from(drt_lowest_absolute + drt_bucket * drt_buckets_size);
            let vtt_bucket = vtt_elements.max().unwrap_or_default() as f64;
            let vtt_priority = Priority::from(vtt_lowest_absolute + vtt_bucket * vtt_buckets_size);
            let st_bucket = st_elements.max().unwrap_or_default() as f64;
            let st_priority = Priority::from(st_lowest_absolute + st_bucket * st_buckets_size);
            let ut_bucket = ut_elements.max().unwrap_or_default() as f64;
            let ut_priority = Priority::from(ut_lowest_absolute + ut_bucket * ut_buckets_size);

            drt_priorities.push(drt_priority);
            vtt_priorities.push(vtt_priority);
            st_priorities.push(st_priority);
            ut_priorities.push(ut_priority);
        }

        let drt_stinky = PriorityEstimate {
            priority: cmp::max(drt_priorities[0], Priority::default_stinky()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[0]) * 60),
        };
        let drt_low = PriorityEstimate {
            priority: cmp::max(drt_priorities[1], Priority::default_low()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[1]) * 60),
        };
        let drt_medium = PriorityEstimate {
            priority: cmp::max(drt_priorities[2], Priority::default_medium()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[2]) * 60),
        };
        let drt_high = PriorityEstimate {
            priority: cmp::max(drt_priorities[3], Priority::default_high()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[3]) * 60),
        };
        let drt_opulent = PriorityEstimate {
            priority: cmp::max(drt_priorities[4], Priority::default_opulent()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[4]) * 60),
        };
        let vtt_stinky = PriorityEstimate {
            priority: cmp::max(vtt_priorities[0], Priority::default_stinky()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[0]) * 60),
        };
        let vtt_low = PriorityEstimate {
            priority: cmp::max(vtt_priorities[1], Priority::default_low()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[1]) * 60),
        };
        let vtt_medium = PriorityEstimate {
            priority: cmp::max(vtt_priorities[2], Priority::default_medium()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[2]) * 60),
        };
        let vtt_high = PriorityEstimate {
            priority: cmp::max(vtt_priorities[3], Priority::default_high()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[3]) * 60),
        };
        let vtt_opulent = PriorityEstimate {
            priority: cmp::max(vtt_priorities[4], Priority::default_opulent()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[4]) * 60),
        };
        let st_stinky = PriorityEstimate {
            priority: cmp::max(st_priorities[0], Priority::default_stinky()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[0]) * 60),
        };
        let st_low = PriorityEstimate {
            priority: cmp::max(st_priorities[1], Priority::default_low()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[1]) * 60),
        };
        let st_medium = PriorityEstimate {
            priority: cmp::max(st_priorities[2], Priority::default_medium()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[2]) * 60),
        };
        let st_high = PriorityEstimate {
            priority: cmp::max(st_priorities[3], Priority::default_high()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[3]) * 60),
        };
        let st_opulent = PriorityEstimate {
            priority: cmp::max(st_priorities[4], Priority::default_opulent()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[4]) * 60),
        };
        let ut_stinky = PriorityEstimate {
            priority: cmp::max(ut_priorities[0], Priority::default_stinky()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[0]) * 60),
        };
        let ut_low = PriorityEstimate {
            priority: cmp::max(ut_priorities[1], Priority::default_low()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[1]) * 60),
        };
        let ut_medium = PriorityEstimate {
            priority: cmp::max(ut_priorities[2], Priority::default_medium()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[2]) * 60),
        };
        let ut_high = PriorityEstimate {
            priority: cmp::max(ut_priorities[3], Priority::default_high()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[3]) * 60),
        };
        let ut_opulent = PriorityEstimate {
            priority: cmp::max(ut_priorities[4], Priority::default_opulent()),
            time_to_block: TimeToBlock::from_secs(u64::from(target_minutes[4]) * 60),
        };

        PrioritiesEstimate {
            drt_stinky,
            drt_low,
            drt_medium,
            drt_high,
            drt_opulent,
            vtt_stinky,
            vtt_low,
            vtt_medium,
            vtt_high,
            vtt_opulent,
            st_stinky,
            st_low,
            st_medium,
            st_high,
            st_opulent,
            ut_stinky,
            ut_low,
            ut_medium,
            ut_high,
            ut_opulent,
        }
    }
}

pub(crate) mod counter {
    use std::{collections::HashMap, hash::Hash};

    /// A dead simple counter for items of type `T` that enables running frequency queries on it.
    pub struct Counter<T> {
        counter: HashMap<T, u64>,
        n: u64,
    }

    impl<T: Eq + Hash + Clone> Counter<T> {
        /// Create a new counter.
        pub fn new() -> Self {
            Self::default()
        }

        /// Add one item to the counter. This will essentially increment by one the current count
        /// of occurrences for the referred item.
        pub fn add(&mut self, value: T) {
            self.n += 1;
            *self.counter.entry(value).or_default() += 1;
        }

        /// Run a frequency query on the counter.
        ///
        /// Frequency queries are expressed as normalized frequency thresholds. E.g.:
        /// - `0` means _appears at least once_.
        /// - `0.5` means _makes for at least 50% of the counted items_.
        /// - `1` means _all counted items are equal to this_.
        #[allow(clippy::cast_precision_loss)]
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        pub fn query(&self, threshold: f64) -> impl '_ + Iterator<Item = T> {
            let bound = ((threshold * (self.n as f64)).ceil()).max(0.) as u64;
            self.counter
                .iter()
                .filter(move |(_k, v)| **v >= bound)
                .map(|(k, _v)| k)
                .cloned()
        }
    }

    impl<T> Default for Counter<T> {
        fn default() -> Self {
            Self {
                counter: Default::default(),
                n: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::prelude::*;
    use rand_distr::Normal;

    use super::*;
    use itertools::Itertools;

    const CHECKPOINTS_PERIOD: u64 = 45;

    #[test]
    fn engine_from_vec() {
        let input = priorities_factory(10usize, 0.0..=100.0, None);
        let engine = PriorityEngine::from_vec_with_capacity(input.clone(), 5);

        assert_eq!(engine.get(0), input.first());
        assert_eq!(engine.get(1), input.get(1));
        assert_eq!(engine.get(2), input.get(2));
        assert_eq!(engine.get(3), input.get(3));
        assert_eq!(engine.get(4), input.get(4));
    }

    #[test]
    fn engine_as_vec() {
        let input = priorities_factory(2usize, 0.0..=100.0, None);
        let mut engine = PriorityEngine::default();
        for priorities in &input {
            engine.push_priorities(priorities.clone());
        }
        let output = engine.as_vec();

        assert_eq!(output, input);
    }

    #[test]
    fn drt_priorities_digestion() {
        let mut priorities = Priorities::default();
        assert_eq!(priorities.drt_highest, 0);
        assert_eq!(priorities.drt_lowest, None);

        priorities.digest_drt_priority(0.into());
        assert_eq!(priorities.drt_highest, 0);
        assert_eq!(priorities.drt_lowest, None);

        priorities.digest_drt_priority(5.into());
        assert_eq!(priorities.drt_highest, 5);
        assert_eq!(priorities.drt_lowest, Some(5.into()));

        priorities.digest_drt_priority(7.into());
        assert_eq!(priorities.drt_highest, 7);
        assert_eq!(priorities.drt_lowest, Some(5.into()));

        priorities.digest_drt_priority(3.into());
        assert_eq!(priorities.drt_highest, 7);
        assert_eq!(priorities.drt_lowest, Some(3.into()));
    }

    #[test]
    fn vtt_priorities_digestion() {
        let mut priorities = Priorities::default();
        assert_eq!(priorities.vtt_highest, 0);
        assert_eq!(priorities.vtt_lowest, None);

        priorities.digest_vtt_priority(0.into());
        assert_eq!(priorities.vtt_highest, 0);
        assert_eq!(priorities.vtt_lowest, None);

        priorities.digest_vtt_priority(5.into());
        assert_eq!(priorities.vtt_highest, 5);
        assert_eq!(priorities.vtt_lowest, Some(5.into()));

        priorities.digest_vtt_priority(7.into());
        assert_eq!(priorities.vtt_highest, 7);
        assert_eq!(priorities.vtt_lowest, Some(5.into()));

        priorities.digest_vtt_priority(3.into());
        assert_eq!(priorities.vtt_highest, 7);
        assert_eq!(priorities.vtt_lowest, Some(3.into()));
    }
    #[test]
    fn st_priorities_digestion() {
        let mut priorities = Priorities::default();
        assert_eq!(priorities.st_highest, 0);
        assert_eq!(priorities.st_lowest, None);

        priorities.digest_st_priority(0.into());
        assert_eq!(priorities.st_highest, 0);
        assert_eq!(priorities.st_lowest, None);

        priorities.digest_st_priority(5.into());
        assert_eq!(priorities.st_highest, 5);
        assert_eq!(priorities.st_lowest, Some(5.into()));

        priorities.digest_st_priority(7.into());
        assert_eq!(priorities.st_highest, 7);
        assert_eq!(priorities.st_lowest, Some(5.into()));

        priorities.digest_st_priority(3.into());
        assert_eq!(priorities.st_highest, 7);
        assert_eq!(priorities.st_lowest, Some(3.into()));
    }

    #[test]
    fn ut_priorities_digestion() {
        let mut priorities = Priorities::default();
        assert_eq!(priorities.ut_highest, 0);
        assert_eq!(priorities.ut_lowest, None);

        priorities.digest_ut_priority(0.into());
        assert_eq!(priorities.ut_highest, 0);
        assert_eq!(priorities.ut_lowest, None);

        priorities.digest_ut_priority(5.into());
        assert_eq!(priorities.ut_highest, 5);
        assert_eq!(priorities.ut_lowest, Some(5.into()));

        priorities.digest_ut_priority(7.into());
        assert_eq!(priorities.ut_highest, 7);
        assert_eq!(priorities.ut_lowest, Some(5.into()));

        priorities.digest_ut_priority(3.into());
        assert_eq!(priorities.ut_highest, 7);
        assert_eq!(priorities.ut_lowest, Some(3.into()));
    }

    // "Aligned" here means that the `PriorityEngine` capacity will match that of its inner
    // `VecDeque`, which only happens for capacities `c` satisfying `c = ℕ ^ 2 + 1`.
    #[test]
    fn engine_capacity_aligned() {
        let mut engine = PriorityEngine::with_capacity(3);
        let priorities_list = (1..=9)
            .map(|i| Priorities {
                drt_highest: Priority::from(i),
                drt_lowest: None,
                vtt_highest: Priority::from(i * 2),
                vtt_lowest: None,
                st_highest: Priority::from(i * 3),
                st_lowest: None,
                ut_highest: Priority::from(i * 4),
                ut_lowest: None,
            })
            .collect_vec();

        for priorities in &priorities_list {
            engine.push_priorities(priorities.clone())
        }

        assert_eq!(engine.get(0).unwrap(), &priorities_list[8]);
        assert_eq!(engine.get(1).unwrap(), &priorities_list[7]);
        assert_eq!(engine.get(2).unwrap(), &priorities_list[6]);
        assert_eq!(engine.get(3), None);
    }

    // "Aligned" here means that the `PriorityEngine` capacity will match that of its inner
    // `VecDeque`, which only happens for capacities `c` satisfying `c = ℕ ^ 2 + 1`.
    #[test]
    fn engine_capacity_not_aligned() {
        let mut engine = PriorityEngine::with_capacity(2);
        let priorities_list = (1..=9)
            .map(|i| Priorities {
                drt_highest: Priority::from(i),
                drt_lowest: None,
                vtt_highest: Priority::from(i * 2),
                vtt_lowest: None,
                st_highest: Priority::from(i * 3),
                st_lowest: None,
                ut_highest: Priority::from(i * 4),
                ut_lowest: None,
            })
            .collect_vec();

        for priorities in &priorities_list {
            engine.push_priorities(priorities.clone())
        }

        assert_eq!(engine.get(0).unwrap(), &priorities_list[8]);
        assert_eq!(engine.get(1).unwrap(), &priorities_list[7]);
        assert_eq!(engine.get(2), None);
    }

    #[test]
    fn cannot_estimate_with_few_epochs_in_queue() {
        let count = MINIMUM_TRACKED_EPOCHS - 1;
        let priorities = priorities_factory(count as usize, 0.0..=100.0, None);
        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine.estimate_priority(Duration::from_secs(CHECKPOINTS_PERIOD));

        assert_eq!(
            estimate,
            Err(PriorityError::NotEnoughSampledEpochs {
                current: count,
                required: MINIMUM_TRACKED_EPOCHS,
                wait_minutes: 1
            })
        );
    }

    #[test]
    fn can_estimate_over_random() {
        let priorities = priorities_factory(1920usize, 100.0..=1000.0, Some(1.0));
        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine
            .estimate_priority(Duration::from_secs(CHECKPOINTS_PERIOD))
            .unwrap();

        let expected = PrioritiesEstimate {
            drt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(179.58806455633322)),
                time_to_block: TimeToBlock(21600),
            },
            drt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(506.93216838191876)),
                time_to_block: TimeToBlock(3600),
            },
            drt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(588.7681943383152)),
                time_to_block: TimeToBlock(900),
            },
            drt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(635.5316377419701)),
                time_to_block: TimeToBlock(300),
            },
            drt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(682.2950811456253)),
                time_to_block: TimeToBlock(60),
            },
            vtt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(98.69972086291244)),
                time_to_block: TimeToBlock(21600),
            },
            vtt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(504.0641403072455)),
                time_to_block: TimeToBlock(3600),
            },
            vtt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(590.0505323105889)),
                time_to_block: TimeToBlock(900),
            },
            vtt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(626.9018431691647)),
                time_to_block: TimeToBlock(300),
            },
            vtt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(663.7531540277403)),
                time_to_block: TimeToBlock(60),
            },
            st_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(147.6148469637572)),
                time_to_block: TimeToBlock(21600),
            },
            st_low: PriorityEstimate {
                priority: Priority(OrderedFloat(498.5057734262123)),
                time_to_block: TimeToBlock(3600),
            },
            st_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(589.0582705778136)),
                time_to_block: TimeToBlock(900),
            },
            st_high: PriorityEstimate {
                priority: Priority(OrderedFloat(634.3345191536143)),
                time_to_block: TimeToBlock(300),
            },
            st_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(668.2917055854648)),
                time_to_block: TimeToBlock(60),
            },
            ut_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(94.15754187923238)),
                time_to_block: TimeToBlock(21600),
            },
            ut_low: PriorityEstimate {
                priority: Priority(OrderedFloat(500.257601935986)),
                time_to_block: TimeToBlock(3600),
            },
            ut_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(589.0919900734009)),
                time_to_block: TimeToBlock(900),
            },
            ut_high: PriorityEstimate {
                priority: Priority(OrderedFloat(627.1638707037214)),
                time_to_block: TimeToBlock(300),
            },
            ut_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(690.6170050875892)),
                time_to_block: TimeToBlock(60),
            },
        };

        assert_eq!(estimate, expected);
    }

    #[test]
    fn can_estimate_over_constant() {
        // 100 blocks where highest and lowest priorities are 1000000 and 1000
        let priorities = vec![
            Priorities {
                drt_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                drt_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
                vtt_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                vtt_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
                st_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                st_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
                ut_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                ut_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
            };
            100
        ];

        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine.estimate_priority(Duration::from_secs(45)).unwrap();

        let expected = PrioritiesEstimate {
            drt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(21600),
            },
            drt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(3600),
            },
            drt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(900),
            },
            drt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(300),
            },
            drt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(60),
            },
            vtt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(21600),
            },
            vtt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(3600),
            },
            vtt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(900),
            },
            vtt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(300),
            },
            vtt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(60),
            },
            st_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(21600),
            },
            st_low: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(3600),
            },
            st_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(900),
            },
            st_high: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(300),
            },
            st_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(60),
            },
            ut_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(21600),
            },
            ut_low: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(3600),
            },
            ut_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(900),
            },
            ut_high: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(300),
            },
            ut_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(1000.0)),
                time_to_block: TimeToBlock(60),
            },
        };

        assert_eq!(estimate, expected);
    }

    #[test]
    fn can_estimate_over_contrast() {
        let priorities = vec![
            Priorities {
                drt_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                drt_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
                vtt_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                vtt_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
                st_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                st_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
                ut_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
                ut_lowest: Some(Priority::from_absolute_fee_weight(1_000, 1)),
            };
            DEFAULT_QUEUE_CAPACITY_EPOCHS
        ];

        let mut engine = PriorityEngine::from_vec(priorities);
        let estimate1 = engine.estimate_priority(Duration::from_secs(45)).unwrap();

        engine.push_priorities(Priorities {
            drt_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
            drt_lowest: Some(Priority::from_absolute_fee_weight(1, 1)),
            vtt_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
            vtt_lowest: Some(Priority::from_absolute_fee_weight(1, 1)),
            st_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
            st_lowest: Some(Priority::from_absolute_fee_weight(1, 1)),
            ut_highest: Priority::from_absolute_fee_weight(1_000_000, 1),
            ut_lowest: Some(Priority::from_absolute_fee_weight(1, 1)),
        });

        let estimate2 = engine.estimate_priority(Duration::from_secs(45)).unwrap();

        // The estimation for "stinky" tier is the only one NOT expected to change.
        assert_ne!(estimate1.drt_stinky, estimate2.drt_stinky);
        assert_eq!(estimate1.drt_low, estimate2.drt_low);
        assert_eq!(estimate1.drt_medium, estimate2.drt_medium);
        assert_eq!(estimate1.drt_high, estimate2.drt_high);
        assert_eq!(estimate1.drt_opulent, estimate2.drt_opulent);
        assert_ne!(estimate1.vtt_stinky, estimate2.vtt_stinky);
        assert_eq!(estimate1.vtt_low, estimate2.vtt_low);
        assert_eq!(estimate1.vtt_medium, estimate2.vtt_medium);
        assert_eq!(estimate1.vtt_high, estimate2.vtt_high);
        assert_eq!(estimate1.vtt_opulent, estimate2.vtt_opulent);
    }

    #[test]
    fn test_target_minutes_algorithm_small() {
        let priorities = priorities_factory(20, 0.0..=1.0, Some(2.0));
        let estimate = strategies::target_minutes(&priorities, [360, 60, 15, 5, 1], 45);

        assert_eq!(
            estimate,
            PrioritiesEstimate {
                drt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.26012197586104785)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.26012197586104785)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.20368096121383047)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.20368096121383047)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5)),
                    time_to_block: TimeToBlock(60)
                },
                st_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.29729848625684224)),
                    time_to_block: TimeToBlock(21600)
                },
                st_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.29729848625684224)),
                    time_to_block: TimeToBlock(3600)
                },
                st_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3)),
                    time_to_block: TimeToBlock(900)
                },
                st_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4)),
                    time_to_block: TimeToBlock(300)
                },
                st_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5)),
                    time_to_block: TimeToBlock(60)
                },
                ut_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.28632348839778504)),
                    time_to_block: TimeToBlock(21600)
                },
                ut_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.28632348839778504)),
                    time_to_block: TimeToBlock(3600)
                },
                ut_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3)),
                    time_to_block: TimeToBlock(900)
                },
                ut_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4)),
                    time_to_block: TimeToBlock(300)
                },
                ut_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5)),
                    time_to_block: TimeToBlock(60)
                }
            }
        )
    }

    #[test]
    fn test_target_minutes_algorithm_medium() {
        let priorities = priorities_factory(360, 0.0..=1.0, Some(2.0));
        let estimate = strategies::target_minutes(&priorities, [360, 60, 15, 5, 1], 45);

        assert_eq!(
            estimate,
            PrioritiesEstimate {
                drt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.15702925083789035)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3965049190709514)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5118080185905735)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.573894302947293)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.17218454876604591)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4950574537029726)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5561415167991479)),
                    time_to_block: TimeToBlock(60)
                },
                st_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.15748499378925293)),
                    time_to_block: TimeToBlock(21600)
                },
                st_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2)),
                    time_to_block: TimeToBlock(3600)
                },
                st_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3541458525647886)),
                    time_to_block: TimeToBlock(900)
                },
                st_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.47929367178558413)),
                    time_to_block: TimeToBlock(300)
                },
                st_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5597458412846669)),
                    time_to_block: TimeToBlock(60)
                },
                ut_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.1906048021048631)),
                    time_to_block: TimeToBlock(21600)
                },
                ut_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2)),
                    time_to_block: TimeToBlock(3600)
                },
                ut_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.3)),
                    time_to_block: TimeToBlock(900)
                },
                ut_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4762393097434008)),
                    time_to_block: TimeToBlock(300)
                },
                ut_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5317793528953387)),
                    time_to_block: TimeToBlock(60)
                }
            }
        )
    }

    #[test]
    fn test_target_minutes_algorithm_big() {
        let priorities = priorities_factory(1_920, 0.0..=1.0, Some(2.0));
        let estimate = strategies::target_minutes(&priorities, [360, 60, 15, 5, 1], 45);

        assert_eq!(
            estimate,
            PrioritiesEstimate {
                drt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.15702925083789035)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.44074006217140804)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.509221982148464)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5483545078496388)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5874870335508137)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.1)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.423089514147716)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5103411710981076)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5539669995733034)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5757799138109014)),
                    time_to_block: TimeToBlock(60)
                },
                st_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.1440263044618641)),
                    time_to_block: TimeToBlock(21600)
                },
                st_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.43073047444064677)),
                    time_to_block: TimeToBlock(3600)
                },
                st_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5071849197683221)),
                    time_to_block: TimeToBlock(900)
                },
                st_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5454121424321599)),
                    time_to_block: TimeToBlock(300)
                },
                st_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5836393650959976)),
                    time_to_block: TimeToBlock(60)
                },
                ut_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.11341166631027368)),
                    time_to_block: TimeToBlock(21600)
                },
                ut_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.42834810653370786)),
                    time_to_block: TimeToBlock(3600)
                },
                ut_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5096220265913683)),
                    time_to_block: TimeToBlock(900)
                },
                ut_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.540099746612991)),
                    time_to_block: TimeToBlock(300)
                },
                ut_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6010551866562364)),
                    time_to_block: TimeToBlock(60)
                }
            }
        )
    }

    #[test]
    fn test_target_minutes_algorithm_humongous() {
        let priorities = priorities_factory(10_000, 0.0..=1.0, Some(2.0));
        let estimate = strategies::target_minutes(&priorities, [360, 60, 15, 5, 1], 45);

        assert_eq!(
            estimate,
            PrioritiesEstimate {
                drt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.43619650230369633)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5225426358242867)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5595481216188254)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5965536074133642)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6335590932079028)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4277241336946116)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5188610598114269)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5530374071052326)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5872137543990384)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6099979859282423)),
                    time_to_block: TimeToBlock(60)
                },
                st_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.42594491039416227)),
                    time_to_block: TimeToBlock(21600)
                },
                st_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5171826715544424)),
                    time_to_block: TimeToBlock(3600)
                },
                st_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5577327876256781)),
                    time_to_block: TimeToBlock(900)
                },
                st_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5881453746791048)),
                    time_to_block: TimeToBlock(300)
                },
                st_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6084204327147227)),
                    time_to_block: TimeToBlock(60)
                },
                ut_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4347914213819278)),
                    time_to_block: TimeToBlock(21600)
                },
                ut_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.512825964594247)),
                    time_to_block: TimeToBlock(3600)
                },
                ut_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5574171321441437)),
                    time_to_block: TimeToBlock(900)
                },
                ut_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5908605078065662)),
                    time_to_block: TimeToBlock(300)
                },
                ut_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6243038834689887)),
                    time_to_block: TimeToBlock(60)
                }
            }
        )
    }

    /// This factory produces priority values that are distributed in slight resemblance to those
    /// found on a real block chain.
    ///
    /// Namely, this produces values distributed normally within a certain range, and then applies
    /// some smoothing.
    ///
    /// A smoothing value of 1 will count the current value and the previous ones equally. A value
    /// higher than 1 will cause the older values to be weighted more than the current one. On the
    /// contrary, values below 1 effectively give the current value more weight than older ones.
    fn priorities_factory(
        count: usize,
        range: ops::RangeInclusive<f64>,
        smoothing: Option<f64>,
    ) -> Vec<Priorities> {
        let (min, max) = range.into_inner();
        let middle = (max + min) / 2.0;
        let sigma = (max - min) / 5.0;
        let normal = Normal::new(middle, sigma).unwrap();
        let mut prng = StdRng::seed_from_u64(0);
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
            middle, middle, middle, middle, middle, middle, middle, middle,
        );
        let smoothing = smoothing.unwrap_or_default();

        let mut output = vec![];
        for _ in 0..count {
            let mut ab = normal.sample(&mut prng);
            let mut bb = normal.sample(&mut prng);
            let mut cb = normal.sample(&mut prng);
            let mut db = normal.sample(&mut prng);
            let mut eb = normal.sample(&mut prng);
            let mut fb = normal.sample(&mut prng);
            let mut gb = normal.sample(&mut prng);
            let mut hb = normal.sample(&mut prng);

            if ab < bb {
                (ab, bb) = (bb, ab)
            }
            if cb < db {
                (cb, db) = (db, cb)
            }
            if eb < fb {
                (eb, fb) = (fb, eb)
            }
            if gb < hb {
                (gb, hb) = (hb, gb)
            }

            (a, b, c, d, e, f, g, h) = (
                (a * smoothing + ab) / (1.0 + smoothing),
                (b * smoothing + bb) / (1.0 + smoothing),
                (c * smoothing + cb) / (1.0 + smoothing),
                (d * smoothing + db) / (1.0 + smoothing),
                (e * smoothing + eb) / (1.0 + smoothing),
                (f * smoothing + fb) / (1.0 + smoothing),
                (g * smoothing + gb) / (1.0 + smoothing),
                (h * smoothing + hb) / (1.0 + smoothing),
            );

            output.push(Priorities {
                drt_highest: Priority::from(a),
                drt_lowest: Some(Priority::from(b)),
                vtt_highest: Priority::from(c),
                vtt_lowest: Some(Priority::from(d)),
                st_highest: Priority::from(e),
                st_lowest: Some(Priority::from(f)),
                ut_highest: Priority::from(g),
                ut_lowest: Some(Priority::from(h)),
            })
        }

        output
    }
}
