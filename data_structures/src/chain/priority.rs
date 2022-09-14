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
    ///
    /// The default values used here assume that estimation operates with picoWit (10 ^ -12).
    /// That is, from a user perspective, all priority values shown here have 3 implicit decimal
    /// digits. They need to be divided by 1,000 for the real protocol-wide nanoWit value, and by
    /// 1,000,000,000,000 for the Wit value. This allows for more fine-grained estimations while the
    /// market for block space is idle.
    pub fn estimate_priority(
        &self,
        seconds_per_epoch: Duration,
    ) -> Result<PrioritiesEstimate, PriorityError> {
        // Short-circuit if there are too few tracked epochs for an accurate estimation.
        let len = u32::try_from(self.priorities.len())?;
        if len < MINIMUM_TRACKED_EPOCHS {
            Err(PriorityError::NotEnoughSampledEpochs(
                len,
                MINIMUM_TRACKED_EPOCHS,
                (MINIMUM_TRACKED_EPOCHS - len) * u32::try_from(seconds_per_epoch.as_secs())? / 60
                    + 1,
            ))?;
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

        Self { priorities: fees }
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
        if self.priorities.len() + 1 == self.priorities.capacity() {
            self.priorities.pop_back();
        }
        self.priorities.push_front(priorities);
    }

    /// Create a new engine of a certain queue capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
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
        _0, _1, _2
    )]
    NotEnoughSampledEpochs(u32, u32, u32),
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
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Priority(OrderedFloat<f64>);

impl Priority {
    #[inline]
    pub fn as_f64(&self) -> f64 {
        self.0.into_inner()
    }

    /// The default precision for tier "High".
    #[inline]
    pub fn default_high() -> Self {
        Self::from(0.4)
    }

    /// The default precision for tier "Low".
    #[inline]
    pub fn default_low() -> Self {
        Self::from(0.2)
    }

    /// The default precision for tier "Medium".
    #[inline]
    pub fn default_medium() -> Self {
        Self::from(0.3)
    }

    /// The default precision for tier "Opulent".
    #[inline]
    pub fn default_opulent() -> Self {
        Self::from(0.5)
    }

    /// The default precision for tier "Stinky".
    #[inline]
    pub fn default_stinky() -> Self {
        Self::from(0.1)
    }

    /// Derive fee from priority and weight.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    pub fn derive_fee(&self, weight: u32) -> Wit {
        Wit::from_nanowits((self.0.into_inner() * f64::from(weight)) as u64)
    }

    /// Constructs a Priority from a transaction fee and weight.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn from_fee_weight(fee: u64, weight: u32) -> Self {
        Self::from(fee as f64 / f64::from(weight))
    }

    #[inline]
    /// Turn a priority value into its internal `f64` value.
    pub fn into_inner(self) -> f64 {
        self.as_f64()
    }
}

/// Conveniently create a Priority value from an f64 value.
impl convert::From<f64> for Priority {
    #[inline]
    fn from(input: f64) -> Self {
        Self(OrderedFloat(input))
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
                    .digest_drt_priority(Priority::from_fee_weight(*fee, *weight));
            }
            Transaction::ValueTransfer(_) => {
                self.0
                    .digest_vtt_priority(Priority::from_fee_weight(*fee, *weight));
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

#[inline]
fn option_update(option: &mut Option<Priority>, candidate: Option<Priority>) {
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
    use pdatastructs::topk::lossycounter::LossyCounter;

    use super::*;

    /// A priority estimation strategy that receives a list of targetted time-to-blocks expressed in
    /// minutes and derives the priority from there by using a [lossy count algorithm].
    ///
    /// [lossy count algorithm]: https://en.wikipedia.org/wiki/Lossy_Count_Algorithm
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn target_minutes<'a>(
        priorities: impl IntoIterator<Item = &'a Priorities>,
        target_minutes: [u16; 5],
        seconds_per_epoch: u16,
    ) -> PrioritiesEstimate {
        // Make the priorities argument an iterator (if it was not already) and measure its length.
        let priorities = priorities.into_iter();
        let priorities_count = priorities
            .size_hint()
            .1
            .unwrap_or(DEFAULT_QUEUE_CAPACITY_EPOCHS) as f64;

        // Fix the capacity of the buckets and let them be as many as needed
        let bucket_capacity = 5.0;
        let buckets_count = priorities_count / bucket_capacity;
        let mut drt_lowest_absolute = None;
        let mut drt_highest_absolute = Priority::default();
        let mut vtt_lowest_absolute = None;
        let mut vtt_highest_absolute = Priority::default();

        let epsilon = 0.1 / buckets_count;
        let mut drt_counter = LossyCounter::<u64>::with_epsilon(epsilon);
        let mut vtt_counter = LossyCounter::<u64>::with_epsilon(epsilon);

        for (
            epoch,
            Priorities {
                drt_highest,
                drt_lowest,
                vtt_highest,
                vtt_lowest,
            },
        ) in priorities.enumerate()
        {
            // Keep track of the lowest and highest recorded priorities.
            option_update(&mut drt_lowest_absolute, *drt_lowest);
            if drt_highest > &drt_highest_absolute {
                drt_highest_absolute = *drt_highest;
            }
            option_update(&mut vtt_lowest_absolute, *vtt_lowest);
            if vtt_highest > &vtt_highest_absolute {
                vtt_highest_absolute = *vtt_highest;
            }

            let epoch = epoch as f64;
            let drt_lowest = drt_lowest.unwrap_or(*drt_highest).as_f64();
            let drt_highest_absolute = drt_highest_absolute.as_f64();
            let drt_lowest_absolute = drt_lowest_absolute
                .map(Priority::into_inner)
                .unwrap_or(drt_highest_absolute);
            let vtt_lowest = vtt_lowest.unwrap_or(*vtt_highest).as_f64();
            let vtt_highest_absolute = vtt_highest_absolute.as_f64();
            let vtt_lowest_absolute = vtt_lowest_absolute
                .map(Priority::into_inner)
                .unwrap_or(vtt_highest_absolute);

            // This calculates the bucket in which the lowest values should be inserted, by applying
            // a rolling "compander" mechanism. That is, we keep track of the absolute range of
            // lowest to highest priority values for all the previous epochs, to then compare the
            // range in each epoch and map one range to another to find its relative position.
            let drt_lowest_compared_to_absolute = drt_lowest / drt_lowest_absolute;
            let drt_range_absolute = drt_highest_absolute / drt_lowest_absolute;
            let drt_bucket_index =
                drt_lowest_compared_to_absolute / drt_range_absolute * buckets_count;
            let vtt_lowest_compared_to_absolute = vtt_lowest / vtt_lowest_absolute;
            let vtt_range_absolute = vtt_highest_absolute / vtt_lowest_absolute;
            let vtt_bucket_index =
                vtt_lowest_compared_to_absolute / vtt_range_absolute * buckets_count;

            // For a perfect calculation, all values lower than the lowest bucket index
            // (representing the lowest fee should be inserted. However, we can get a good enough
            // approximation while saving CPU and memory by insert only the 10% closest values from
            // below.
            if epoch > 0.0 {
                for bucket_index in (drt_bucket_index * 0.9) as u64..=drt_bucket_index as u64 {
                    drt_counter.add(bucket_index);
                }
                for bucket_index in (vtt_bucket_index * 0.9) as u64..=vtt_bucket_index as u64 {
                    vtt_counter.add(bucket_index);
                }
            }
        }

        // Make an estimation for each of the targeted time-to-blocks.
        let mut drt_priorities: Vec<Priority> = vec![];
        let mut vtt_priorities: Vec<Priority> = vec![];
        for minutes in target_minutes.into_iter() {
            // Derive the frequency threshold for this targeted time-to-block.
            let epochs = f64::from(minutes) * 60.0 / f64::from(seconds_per_epoch);
            let epochs_freq = epochs / priorities_count;
            let threshold = epochs_freq / bucket_capacity;

            // Run the frequency query on the lossy counters.
            let drt_elements = drt_counter.query(threshold);
            let vtt_elements = vtt_counter.query(threshold);

            // The priority is calculated by reverting the buckets mapping performed before, i.e.
            // mapping the bucket index back to a priority value.
            let drt_bucket = drt_elements.max().unwrap_or_default();
            let drt_priority = drt_lowest_absolute.unwrap_or_default()
                + (drt_highest_absolute - drt_lowest_absolute.unwrap_or_default())
                    * (drt_bucket as f64 / buckets_count);
            let vtt_bucket = vtt_elements.max().unwrap_or_default();
            let vtt_priority = vtt_lowest_absolute.unwrap_or_default()
                + (vtt_highest_absolute - drt_lowest_absolute.unwrap_or_default())
                    * (vtt_bucket as f64 / buckets_count);

            drt_priorities.push(drt_priority);
            vtt_priorities.push(vtt_priority);
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

        assert_eq!(engine.get(0), input.get(0));
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
    fn engine_capacity() {
        let mut engine = PriorityEngine::with_capacity(2);
        let priorities_list = (1..=9)
            .map(|i| Priorities {
                drt_highest: Priority::from(i),
                drt_lowest: None,
                vtt_highest: Priority::from(i * 2),
                vtt_lowest: None,
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
            Err(PriorityError::NotEnoughSampledEpochs(
                count,
                MINIMUM_TRACKED_EPOCHS,
                1
            ))
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
                priority: Priority(OrderedFloat(115.34829276116884)),
                time_to_block: TimeToBlock(21600),
            },
            drt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(576.3935239638417)),
                time_to_block: TimeToBlock(3600),
            },
            drt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(674.3934764900139)),
                time_to_block: TimeToBlock(900),
            },
            drt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(721.1661811047777)),
                time_to_block: TimeToBlock(300),
            },
            drt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(736.7570826430324)),
                time_to_block: TimeToBlock(60),
            },
            vtt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(99.58023815374057)),
                time_to_block: TimeToBlock(21600),
            },
            vtt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(550.3186193446192)),
                time_to_block: TimeToBlock(3600),
            },
            vtt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(626.1640200257766)),
                time_to_block: TimeToBlock(900),
            },
            vtt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(671.671260434471)),
                time_to_block: TimeToBlock(300),
            },
            vtt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(684.6733291226694)),
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
                drt_highest: Priority::from_fee_weight(1_000_000, 1),
                drt_lowest: Some(Priority::from_fee_weight(1_000, 1)),
                vtt_highest: Priority::from_fee_weight(1_000_000, 1),
                vtt_lowest: Some(Priority::from_fee_weight(1_000, 1)),
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
        };

        assert_eq!(estimate, expected);
    }

    #[test]
    fn test_target_minutes_algorithm_small() {
        let priorities = priorities_factory(20, 0.0..=1.0, Some(2.0));
        let estimate = strategies::target_minutes(&priorities, [360, 60, 15, 5, 1], 45);

        assert_eq!(
            estimate,
            PrioritiesEstimate {
                drt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2891084568633235)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2891084568633235)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.548211837008209)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.548211837008209)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.548211837008209)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.17393312762758165)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.2)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4209289040400327)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4209289040400327)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5444267922462582)),
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
                    priority: Priority(OrderedFloat(0.18979375840681975)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5157078616340202)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5843213570502729)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6272047916854309)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6357814786124625)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.17393312762758165)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5091580120460959)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5743406284608069)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6302114425305594)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.639523244875518)),
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
                    priority: Priority(OrderedFloat(0.13668844087951934)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5136436152555278)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5916999897475296)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6316800839995306)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6488144101075309)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.1)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.44606067409752925)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5197487439029155)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5527144593421672)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5701668969276534)),
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
                    priority: Priority(OrderedFloat(0.13668844087951934)),
                    time_to_block: TimeToBlock(21600)
                },
                drt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.521736455827834)),
                    time_to_block: TimeToBlock(3600)
                },
                drt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5913325606907907)),
                    time_to_block: TimeToBlock(900)
                },
                drt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6295347704253484)),
                    time_to_block: TimeToBlock(300)
                },
                drt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.6412602011359553)),
                    time_to_block: TimeToBlock(60)
                },
                vtt_stinky: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.1)),
                    time_to_block: TimeToBlock(21600)
                },
                vtt_low: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.4637394804623482)),
                    time_to_block: TimeToBlock(3600)
                },
                vtt_medium: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5306326697892428)),
                    time_to_block: TimeToBlock(900)
                },
                vtt_high: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5766968420802181)),
                    time_to_block: TimeToBlock(300)
                },
                vtt_opulent: PriorityEstimate {
                    priority: Priority(OrderedFloat(0.5879124666380209)),
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
        let (mut a, mut b, mut c, mut d) = (middle, middle, middle, middle);
        let smoothing = smoothing.unwrap_or_default();

        let mut output = vec![];
        for _ in 0..count {
            let mut ab = normal.sample(&mut prng);
            let mut bb = normal.sample(&mut prng);
            let mut cb = normal.sample(&mut prng);
            let mut db = normal.sample(&mut prng);

            if ab < bb {
                (ab, bb) = (bb, ab)
            }
            if cb < db {
                (cb, db) = (db, cb)
            }

            (a, b, c, d) = (
                (a * smoothing + ab) / (1.0 + smoothing),
                (b * smoothing + bb) / (1.0 + smoothing),
                (c * smoothing + cb) / (1.0 + smoothing),
                (d * smoothing + db) / (1.0 + smoothing),
            );

            output.push(Priorities {
                drt_highest: Priority::from(a),
                drt_lowest: Some(Priority::from(b)),
                vtt_highest: Priority::from(c),
                vtt_lowest: Some(Priority::from(d)),
            })
        }

        output
    }
}
