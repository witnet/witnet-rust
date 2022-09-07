use std::{cmp, convert, fmt, ops};

use circular_queue::CircularQueue;
use failure::Fail;
use itertools::Itertools;
use ordered_float::OrderedFloat;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::Add;

use crate::{
    transaction::Transaction,
    types::visitor::{StatefulVisitor, Visitor},
    wit::Wit,
};
use std::cmp::Ordering;

// Assuming no missing epochs, this will keep track of priority used by transactions in the last 12
// hours (960 epochs).
const DEFAULT_QUEUE_CAPACITY_EPOCHS: usize = 960;
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
    priorities: CircularQueue<Priorities>,
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
    pub fn estimate_priority(&self) -> Result<PrioritiesEstimate, PriorityError> {
        // Short-circuit if there are too few tracked epochs for an accurate estimation.
        let len = self.priorities.len() as u32;
        if len < MINIMUM_TRACKED_EPOCHS {
            return Err(PriorityError::NotEnoughSampledEpochs(
                len,
                MINIMUM_TRACKED_EPOCHS,
                (MINIMUM_TRACKED_EPOCHS - len) * 45 / 60 + 1,
            ));
        }

        // Find out the queue capacity. We can only provide estimates up to this number of epochs.
        let capacity = self.priorities.capacity();
        // Will keep track of the absolute minimum and maximum priorities found in the engine.
        let mut absolutes = Priorities::default();
        // Initialize accumulators for different priorities.
        let mut drt_low = Priority::default();
        let mut drt_medium = Priority::default();
        let mut drt_high = Priority::default();
        let mut vtt_low = Priority::default();
        let mut vtt_medium = Priority::default();
        let mut vtt_high = Priority::default();
        // To be used later as the divisors in an age weighted arithmetic means.
        // These are initialized to 1 to avoid division by zero issues.
        let mut drt_divisor = 1u64;
        let mut vtt_divisor = 1u64;

        let mut age = len as u64;
        for Priorities {
            drt_highest,
            drt_lowest,
            vtt_highest,
            vtt_lowest,
        } in self.priorities.iter().cloned()
        {
            age -= 1;

            // Digest the lowest and highest priorities in each entry to find the absolute lowest
            // (to be used for "stinky" priority estimation) and absolute highest (used for
            // "opulent" priority estimation).
            //
            // Priority values are also added to accumulators as the addition part of an age
            // weighted arithmetic mean.
            if let Some(drt_lowest) = drt_lowest {
                absolutes.digest_drt_priority(drt_lowest);
                drt_low += drt_lowest * age;
                drt_medium += (drt_lowest + drt_highest) / 2 * age;
                drt_divisor += age;
            }
            if let Some(vtt_lowest) = vtt_lowest {
                absolutes.digest_vtt_priority(vtt_lowest);
                vtt_low += vtt_lowest * age;
                vtt_medium += (vtt_lowest + vtt_highest) / 2 * age;
                vtt_divisor += age;
            }
            absolutes.digest_drt_priority(drt_highest);
            absolutes.digest_vtt_priority(vtt_highest);
            drt_high += drt_highest * age;
            vtt_high += vtt_highest * age;
        }

        // Different floors are enforced on the different tiers of priority.
        // Some are also corrected by 15% up or down to make priorities more dynamic.
        let drt_stinky_priority = absolutes
            .drt_lowest
            .unwrap_or_else(Priority::default_stinky);
        let drt_low_priority = cmp::max(drt_low * 0.85 / drt_divisor, Priority::default_low());
        let drt_medium_priority = cmp::max(drt_medium / drt_divisor, Priority::default_medium());
        let drt_high_priority = cmp::max(drt_high * 1.15 / drt_divisor, Priority::default_high());
        let drt_opulent_priority =
            cmp::max(absolutes.drt_highest * 1.15, Priority::default_opulent());
        let vtt_stinky_priority = absolutes
            .vtt_lowest
            .unwrap_or_else(Priority::default_stinky);
        let vtt_low_priority = cmp::max(vtt_low * 0.85 / vtt_divisor, Priority::default_low());
        let vtt_medium_priority = cmp::max(vtt_medium / vtt_divisor, Priority::default_medium());
        let vtt_high_priority = cmp::max(vtt_high * 1.15 / vtt_divisor, Priority::default_high());
        let vtt_opulent_priority =
            cmp::max(absolutes.vtt_highest * 1.15, Priority::default_opulent());

        // Collect the relative epochs inside the engine in which each tier of priority was enough
        // for making it into a block, by comparing to the lowest priority mined in that epoch.
        let mut drt_stinky_enough_epochs = vec![];
        let mut drt_low_enough_epochs = vec![];
        let mut drt_medium_enough_epochs = vec![];
        let mut drt_high_enough_epochs = vec![];
        let mut vtt_stinky_enough_epochs = vec![];
        let mut vtt_low_enough_epochs = vec![];
        let mut vtt_medium_enough_epochs = vec![];
        let mut vtt_high_enough_epochs = vec![];
        for (epoch, priorities) in self.priorities.iter().enumerate() {
            if Some(drt_stinky_priority) >= priorities.drt_lowest {
                drt_stinky_enough_epochs.push(epoch);
            }
            if Some(drt_low_priority) >= priorities.drt_lowest {
                drt_low_enough_epochs.push(epoch);
            }
            if Some(drt_medium_priority) >= priorities.drt_lowest {
                drt_medium_enough_epochs.push(epoch);
            }
            if Some(drt_high_priority) >= priorities.drt_lowest {
                drt_high_enough_epochs.push(epoch);
            }
            if Some(vtt_stinky_priority) >= priorities.drt_lowest {
                vtt_stinky_enough_epochs.push(epoch);
            }
            if Some(vtt_low_priority) >= priorities.vtt_lowest {
                vtt_low_enough_epochs.push(epoch);
            }
            if Some(vtt_medium_priority) >= priorities.vtt_lowest {
                vtt_medium_enough_epochs.push(epoch);
            }
            if Some(vtt_high_priority) >= priorities.vtt_lowest {
                vtt_high_enough_epochs.push(epoch);
            }
        }

        // Measure the average time between occurrences of a tier of priority being enough for
        // making it into a block.
        let drt_stinky_ttb = cmp::max(
            average_gap(drt_stinky_enough_epochs, len),
            capacity as u32 / 2,
        );
        let drt_low_ttb = cmp::max(average_gap(drt_low_enough_epochs, len), 2);
        let drt_medium_ttb = cmp::max(average_gap(drt_medium_enough_epochs, len), 2);
        let drt_high_ttb = cmp::max(average_gap(drt_high_enough_epochs, len), 2);
        let vtt_stinky_ttb = cmp::max(
            average_gap(vtt_stinky_enough_epochs, len),
            capacity as u32 / 2,
        );
        let vtt_low_ttb = cmp::max(average_gap(vtt_low_enough_epochs, len), 2);
        let vtt_medium_ttb = cmp::max(average_gap(vtt_medium_enough_epochs, len), 2);
        let vtt_high_ttb = cmp::max(average_gap(vtt_high_enough_epochs, len), 2);

        Ok(PrioritiesEstimate {
            drt_stinky: PriorityEstimate {
                priority: drt_stinky_priority,
                time_to_block: TimeToBlock::UpTo(drt_stinky_ttb),
            },
            drt_low: PriorityEstimate {
                priority: drt_low_priority,
                time_to_block: TimeToBlock::Around(drt_low_ttb),
            },
            drt_medium: PriorityEstimate {
                priority: drt_medium_priority,
                time_to_block: TimeToBlock::Around(drt_medium_ttb),
            },
            drt_high: PriorityEstimate {
                priority: drt_high_priority,
                time_to_block: TimeToBlock::Around(drt_high_ttb),
            },
            drt_opulent: PriorityEstimate {
                priority: drt_opulent_priority,
                time_to_block: TimeToBlock::LessThan(2),
            },
            vtt_stinky: PriorityEstimate {
                priority: vtt_stinky_priority,
                time_to_block: TimeToBlock::UpTo(vtt_stinky_ttb),
            },
            vtt_low: PriorityEstimate {
                priority: vtt_low_priority,
                time_to_block: TimeToBlock::Around(vtt_low_ttb),
            },
            vtt_medium: PriorityEstimate {
                priority: vtt_medium_priority,
                time_to_block: TimeToBlock::Around(vtt_medium_ttb),
            },
            vtt_high: PriorityEstimate {
                priority: vtt_high_priority,
                time_to_block: TimeToBlock::Around(vtt_high_ttb),
            },
            vtt_opulent: PriorityEstimate {
                priority: vtt_opulent_priority,
                time_to_block: TimeToBlock::LessThan(2),
            },
        })
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
        let mut fees = CircularQueue::with_capacity(capacity);
        // Push as many elements from the input as they can fit in the queue
        priorities
            .into_iter()
            .take(capacity)
            .rev()
            .for_each(|entry| {
                fees.push(entry);
            });

        Self { priorities: fees }
    }

    /// Get the entry at a certain position, if an item at that position exists, or None otherwise.
    #[cfg(test)]
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Priorities> {
        if index >= self.priorities.capacity() {
            None
        } else {
            self.priorities.iter().nth(index)
        }
    }

    /// Push a new `Priorities` entry into the engine.
    #[inline]
    pub fn push_priorities(&mut self, priorities: Priorities) {
        log::trace!("Pushing new transaction priorities entry: {:?}", priorities);
        self.priorities.push(priorities);
        log::trace!(
            "The priority engine has received new data. The priority estimate is now:\n{}",
            self.estimate_priority().unwrap_or_default()
        );
    }

    /// Create a new engine of a certain queue capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            priorities: CircularQueue::with_capacity(capacity),
        }
    }
}

/// Different errors that the `PriorityEngine` can produce.
#[derive(Debug, Eq, Fail, PartialEq)]
pub enum PriorityError {
    /// The number of sampled epochs in the engine is not enough for providing a reliable estimate.
    #[fail(
        display = "The node has only sampled priority from {} blocks but at least {} are needed to provide a reliable priority estimate. Please retry after {} minutes.",
        _0, _1, _2
    )]
    NotEnoughSampledEpochs(u32, u32, u32),
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Priority(OrderedFloat<f64>);

impl Priority {
    /// The default precision for tier "High".
    #[inline]
    pub fn default_high() -> Self {
        Self::from(0.3)
    }

    /// The default precision for tier "Low".
    #[inline]
    pub fn default_low() -> Self {
        Self::from(0.1)
    }

    /// The default precision for tier "Medium".
    #[inline]
    pub fn default_medium() -> Self {
        Self::from(0.2)
    }

    /// The default precision for tier "Opulent".
    #[inline]
    pub fn default_opulent() -> Self {
        Self::from(0.4)
    }

    /// The default precision for tier "Stinky".
    #[inline]
    pub fn default_stinky() -> Self {
        Self::from(0.0)
    }

    /// Derive fee from priority and weight.
    #[inline]
    pub fn derive_fee(&self, weight: u32) -> Wit {
        Wit::from_nanowits((self.0.into_inner() * weight as f64) as u64)
    }

    /// Constructs a Priority from a transaction fee and weight.
    #[inline]
    pub fn from_fee_weight(fee: u64, weight: u32) -> Self {
        Self::from(fee as f64 / weight as f64)
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
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl cmp::PartialEq<u64> for Priority {
    fn eq(&self, other: &u64) -> bool {
        self.0.into_inner().eq(&(*other as f64))
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

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

/// Allow `+=` on `Priority`
impl ops::AddAssign for Priority {
    fn add_assign(&mut self, rhs: Self) {
        *self = self.add(rhs);
    }
}

/// Allow multiplying `Priority` values by `u64` values.
impl ops::Mul<u64> for Priority {
    type Output = Self;

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

/// Allow dividing `Priority` values by `u64` values.
impl ops::Div<u64> for Priority {
    type Output = Self;

    fn div(self, rhs: u64) -> Self::Output {
        Self(self.0 / rhs as f64)
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

impl PrioritiesEstimate {
    /// Show a nicely formatted table with the priority and time-to-block estimates for different
    /// priority tiers.
    ///
    /// The `time_formatter` function allows to define how to format the time-to-block.
    #[allow(clippy::to_string_in_format_args)]
    pub fn pretty_print<F>(&self, mut time_formatter: F) -> String
    where
        F: FnMut(&TimeToBlock) -> String,
    {
        format!(
            r#"╔══════════════════════════════════════════════════════════╗
║ TRANSACTION PRIORITY ESTIMATION REPORT                   ║
╠══════════════════════════════════════════════════════════╣
║ Data request transactions                                ║
╟──────────┬──────────┬────────────────────────────────────║
║     Tier │ Priority │ Time-to-block                      ║
╟──────────┼──────────┼────────────────────────────────────║
║   Stinky │ {:<8} │ {:<33}  ║
║      Low │ {:<8} │ {:<33}  ║
║   Medium │ {:<8} │ {:<33}  ║
║     High │ {:<8} │ {:<33}  ║
║  Opulent │ {:<8} │ {:<33}  ║
╠══════════════════════════════════════════════════════════╣
║ Value transfer transactions                              ║
╟──────────┬──────────┬────────────────────────────────────║
║     Tier │ Priority │ Time-to-block                      ║
╟──────────┼──────────┼────────────────────────────────────║
║   Stinky │ {:<8} │ {:<33}  ║
║      Low │ {:<8} │ {:<33}  ║
║   Medium │ {:<8} │ {:<33}  ║
║     High │ {:<8} │ {:<33}  ║
║  Opulent │ {:<8} │ {:<33}  ║
╚══════════════════════════════════════════════════════════╝"#,
            // Believe it or not, these `to_string` are needed for proper formatting, hence the
            // clippy allow directive above.
            self.drt_stinky.priority.to_string(),
            time_formatter(&self.drt_stinky.time_to_block),
            self.drt_low.priority.to_string(),
            time_formatter(&self.drt_low.time_to_block),
            self.drt_medium.priority.to_string(),
            time_formatter(&self.drt_medium.time_to_block),
            self.drt_high.priority.to_string(),
            time_formatter(&self.drt_high.time_to_block),
            self.drt_opulent.priority.to_string(),
            time_formatter(&self.drt_opulent.time_to_block),
            self.vtt_stinky.priority.to_string(),
            time_formatter(&self.vtt_stinky.time_to_block),
            self.vtt_low.priority.to_string(),
            time_formatter(&self.vtt_low.time_to_block),
            self.vtt_medium.priority.to_string(),
            time_formatter(&self.vtt_medium.time_to_block),
            self.vtt_high.priority.to_string(),
            time_formatter(&self.vtt_high.time_to_block),
            self.vtt_opulent.priority.to_string(),
            time_formatter(&self.vtt_opulent.time_to_block),
        )
    }

    /// Call `TimeToBlock::pretty_print` with time-to-block pretty-printed as seconds.
    ///
    /// This requires knowledge of the number of seconds per epoch, aka __checkpoint period__.
    pub fn pretty_print_secs(&self, seconds_per_epoch: u16) -> String {
        self.pretty_print(|ttb| ttb.pretty_print_secs(seconds_per_epoch))
    }
}

impl fmt::Display for PrioritiesEstimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_print(TimeToBlock::to_string))
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

/// Allows tagging time-to-block estimations for the sake of UX.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum TimeToBlock {
    /// The time-to-block is around X epochs.
    Around(u32),
    /// The time-to-block is exactly X epochs.
    Exactly(u32),
    /// The time-to-block is less than X epochs.
    LessThan(u32),
    /// The time-to-block is unknown.
    #[default]
    Unknown,
    /// The time-to-block is up to X epochs.
    UpTo(u32),
}

impl TimeToBlock {
    /// Convert a `TimeToBlock` into seconds.
    ///
    /// This requires knowledge of the number of seconds per epoch, aka __checkpoint period__.
    pub fn as_secs(&self, seconds_per_epoch: u16) -> Option<u32> {
        match self {
            TimeToBlock::Around(x) => Some(x * seconds_per_epoch as u32),
            TimeToBlock::Exactly(x) => Some(x * seconds_per_epoch as u32),
            TimeToBlock::LessThan(x) => Some(x * seconds_per_epoch as u32),
            TimeToBlock::Unknown => None,
            TimeToBlock::UpTo(x) => Some(x * seconds_per_epoch as u32),
        }
    }

    /// Convert a `TimeToBlock` into a formatted string expressing the time-to-block in seconds.
    ///
    /// This requires knowledge of the number of seconds per epoch, aka __checkpoint period__.
    pub fn pretty_print_secs(&self, seconds_per_epoch: u16) -> String {
        fn unit_change(seconds: u32, divider: u32) -> (u32, u32, String) {
            let value = seconds / divider;
            let remainder = seconds % divider;
            let plural = String::from(if value == 1 { "" } else { "s" });

            (value, remainder, plural)
        }

        let string = self
            .as_secs(seconds_per_epoch)
            .map(|seconds| {
                let mut strings = Vec::<String>::new();
                let (days, remainder, plural) = unit_change(seconds, 24 * 60 * 60);
                if days > 0 {
                    strings.push(format!("{} day{}", days, plural))
                }
                let (hours, remainder, plural) = unit_change(remainder, 60 * 60);
                if hours > 0 {
                    let separator = if strings.is_empty() {
                        ""
                    } else if remainder > 0 {
                        ", "
                    } else {
                        " and "
                    };

                    strings.push(format!("{}{} hour{}", separator, hours, plural))
                }
                let (minutes, remainder, plural) = unit_change(remainder, 60);
                if minutes > 0 {
                    let separator = if strings.is_empty() {
                        ""
                    } else if remainder > 0 {
                        ", "
                    } else {
                        " and "
                    };
                    strings.push(format!("{}{} minute{}", separator, minutes, plural))
                }
                let (seconds, _, plural) = unit_change(remainder, 1);
                if seconds > 0 || (minutes == 0 && hours == 0 && days == 0) {
                    let separator = if strings.is_empty() { "" } else { " and " };
                    strings.push(format!("{}{} second{}", separator, seconds, plural));
                }

                strings.join("")
            })
            .unwrap_or_default();

        match self {
            TimeToBlock::Around(_) => format!("around {}", string),
            TimeToBlock::Exactly(_) => string,
            TimeToBlock::LessThan(_) => format!("less than {}", string),
            TimeToBlock::Unknown => String::from("unknown"),
            TimeToBlock::UpTo(_) => format!("up to {}", string),
        }
    }
}

impl fmt::Display for TimeToBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeToBlock::Around(x) => write!(f, "around {} epochs", x),
            TimeToBlock::Exactly(x) => write!(f, "{}", x),
            TimeToBlock::LessThan(x) => write!(f, "less than {} epochs", x),
            TimeToBlock::Unknown => write!(f, "unknown"),
            TimeToBlock::UpTo(x) => write!(f, "up to {} epochs", x),
        }
    }
}

/// Calculates the average gap between the values in a `Vec<usize>`.
fn average_gap(occurrences: Vec<usize>, sample_size: u32) -> u32 {
    let (_, gaps) = occurrences.iter().fold((0, 0), |(prev_i, sum), cur_i| {
        let gap = *cur_i - prev_i;

        (*cur_i, (sum + gap).saturating_sub(1))
    });

    ((sample_size as f64) / occurrences.len().saturating_sub(gaps).saturating_add(1) as f64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;

    fn priorities_factory(count: usize) -> Vec<Priorities> {
        let mut prng = StdRng::seed_from_u64(0);

        let mut output = vec![];
        for _ in 0..count {
            let mut a = prng.gen_range(0, 10_000);
            let mut b = prng.gen_range(0, 10_000);
            let mut c = prng.gen_range(0, 10_000);
            let mut d = prng.gen_range(0, 10_000);

            if a < b {
                (a, b) = (b, a)
            }
            if c < d {
                (c, d) = (d, c)
            }

            output.push(Priorities {
                drt_highest: Priority::from(a / 1_000),
                drt_lowest: Some(Priority::from(b / 1_000)),
                vtt_highest: Priority::from(c / 1_000),
                vtt_lowest: Some(Priority::from(d / 1_000)),
            })
        }

        output
    }

    #[test]
    fn engine_from_vec() {
        let input = priorities_factory(10usize);
        let engine = PriorityEngine::from_vec_with_capacity(input.clone(), 5);

        assert_eq!(engine.get(0), input.get(0));
        assert_eq!(engine.get(1), input.get(1));
        assert_eq!(engine.get(2), input.get(2));
        assert_eq!(engine.get(3), input.get(3));
        assert_eq!(engine.get(4), input.get(4));
    }

    #[test]
    fn engine_as_vec() {
        let input = priorities_factory(2usize);
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
    fn cannot_estimate_with_few_epochs_in_queue() {
        let count = MINIMUM_TRACKED_EPOCHS - 1;
        let priorities = priorities_factory(count as usize);
        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine.estimate_priority();

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
    fn can_estimate_correctly() {
        use TimeToBlock::*;

        let priorities = priorities_factory(100usize);
        let engine = PriorityEngine::from_vec(priorities);
        let estimate = engine.estimate_priority().unwrap();

        let expected = PrioritiesEstimate {
            drt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(0.0)),
                time_to_block: UpTo(480),
            },
            drt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(2.350333266006867)),
                time_to_block: Around(10),
            },
            drt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(4.656029085033326)),
                time_to_block: Around(2),
            },
            drt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(7.52900424156736)),
                time_to_block: Around(2),
            },
            drt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(9.9)),
                time_to_block: LessThan(2),
            },
            vtt_stinky: PriorityEstimate {
                priority: Priority(OrderedFloat(0.0)),
                time_to_block: UpTo(480),
            },
            vtt_low: PriorityEstimate {
                priority: Priority(OrderedFloat(2.540729145627146)),
                time_to_block: Around(100),
            },
            vtt_medium: PriorityEstimate {
                priority: Priority(OrderedFloat(4.41739042617653)),
                time_to_block: Around(2),
            },
            vtt_high: PriorityEstimate {
                priority: Priority(OrderedFloat(6.722540900828116)),
                time_to_block: Around(2),
            },
            vtt_opulent: PriorityEstimate {
                priority: Priority(OrderedFloat(9.9)),
                time_to_block: LessThan(2),
            },
        };

        assert_eq!(estimate, expected);
    }

    #[test]
    fn time_to_block_pretty_print_secs() {
        let checkpoint_period = 1;
        let minute = 60;
        let hour = 60 * minute;
        let day = 24 * hour;

        assert_eq!(
            TimeToBlock::Exactly(0).pretty_print_secs(checkpoint_period),
            String::from("0 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(1).pretty_print_secs(checkpoint_period),
            String::from("1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(minute - 1).pretty_print_secs(checkpoint_period),
            String::from("59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(minute).pretty_print_secs(checkpoint_period),
            String::from("1 minute")
        );
        assert_eq!(
            TimeToBlock::Exactly(minute + 1).pretty_print_secs(checkpoint_period),
            String::from("1 minute and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * minute - 1).pretty_print_secs(checkpoint_period),
            String::from("1 minute and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(hour - minute).pretty_print_secs(checkpoint_period),
            String::from("59 minutes")
        );
        assert_eq!(
            TimeToBlock::Exactly(hour - 1).pretty_print_secs(checkpoint_period),
            String::from("59 minutes and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(hour).pretty_print_secs(checkpoint_period),
            String::from("1 hour")
        );
        assert_eq!(
            TimeToBlock::Exactly(hour + 1).pretty_print_secs(checkpoint_period),
            String::from("1 hour and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(hour + minute).pretty_print_secs(checkpoint_period),
            String::from("1 hour and 1 minute")
        );
        assert_eq!(
            TimeToBlock::Exactly(hour + minute + 1).pretty_print_secs(checkpoint_period),
            String::from("1 hour, 1 minute and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(23 * hour).pretty_print_secs(checkpoint_period),
            String::from("23 hours")
        );
        assert_eq!(
            TimeToBlock::Exactly(23 * hour + minute - 1).pretty_print_secs(checkpoint_period),
            String::from("23 hours and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(day - minute).pretty_print_secs(checkpoint_period),
            String::from("23 hours and 59 minutes")
        );
        assert_eq!(
            TimeToBlock::Exactly(day - 1).pretty_print_secs(checkpoint_period),
            String::from("23 hours, 59 minutes and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(day).pretty_print_secs(checkpoint_period),
            String::from("1 day")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + 1).pretty_print_secs(checkpoint_period),
            String::from("1 day and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + minute - 1).pretty_print_secs(checkpoint_period),
            String::from("1 day and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + minute).pretty_print_secs(checkpoint_period),
            String::from("1 day and 1 minute")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + minute + 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 1 minute and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + 2 * minute - 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 1 minute and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + hour - minute).pretty_print_secs(checkpoint_period),
            String::from("1 day and 59 minutes")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + hour - 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 59 minutes and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + hour).pretty_print_secs(checkpoint_period),
            String::from("1 day and 1 hour")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + hour + 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 1 hour and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + hour + minute).pretty_print_secs(checkpoint_period),
            String::from("1 day, 1 hour and 1 minute")
        );
        assert_eq!(
            TimeToBlock::Exactly(day + hour + minute + 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 1 hour, 1 minute and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day - hour).pretty_print_secs(checkpoint_period),
            String::from("1 day and 23 hours")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day - hour + minute - 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 23 hours and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day - minute).pretty_print_secs(checkpoint_period),
            String::from("1 day, 23 hours and 59 minutes")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day - 1).pretty_print_secs(checkpoint_period),
            String::from("1 day, 23 hours, 59 minutes and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day).pretty_print_secs(checkpoint_period),
            String::from("2 days")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + 1).pretty_print_secs(checkpoint_period),
            String::from("2 days and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + minute - 1).pretty_print_secs(checkpoint_period),
            String::from("2 days and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + minute).pretty_print_secs(checkpoint_period),
            String::from("2 days and 1 minute")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + minute + 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 1 minute and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + 2 * minute - 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 1 minute and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + hour - minute).pretty_print_secs(checkpoint_period),
            String::from("2 days and 59 minutes")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + hour - 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 59 minutes and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + hour).pretty_print_secs(checkpoint_period),
            String::from("2 days and 1 hour")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + hour + 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 1 hour and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + hour + minute).pretty_print_secs(checkpoint_period),
            String::from("2 days, 1 hour and 1 minute")
        );
        assert_eq!(
            TimeToBlock::Exactly(2 * day + hour + minute + 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 1 hour, 1 minute and 1 second")
        );
        assert_eq!(
            TimeToBlock::Exactly(3 * day - hour).pretty_print_secs(checkpoint_period),
            String::from("2 days and 23 hours")
        );
        assert_eq!(
            TimeToBlock::Exactly(3 * day - hour + minute - 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 23 hours and 59 seconds")
        );
        assert_eq!(
            TimeToBlock::Exactly(3 * day - minute).pretty_print_secs(checkpoint_period),
            String::from("2 days, 23 hours and 59 minutes")
        );
        assert_eq!(
            TimeToBlock::Exactly(3 * day - 1).pretty_print_secs(checkpoint_period),
            String::from("2 days, 23 hours, 59 minutes and 59 seconds")
        );
    }
}
