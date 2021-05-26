//! Implement a human-friendly serialization for some types.
//! This allows us to use an efficient binary representation when serializing
//! to storage, but have a nice string-based representation when using JSON.
//!
//! For example, a `Hash` can be serialized as `[13, 53, 125, ...]` or as `"0d357d..."`.

// Ideally all this code would be generated with a `#[serde(human_readable_string)]` macro.

use crate::{
    chain::{GenesisBlockInfo, Hash, OutputPointer, PublicKeyHash, ValueTransferOutput, SHA256},
    get_environment,
    utxo_pool::UtxoSelectionStrategy,
};
use serde::{
    de::{self, IntoDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, fmt::Display, str::FromStr};

#[derive(Deserialize, Serialize)]
enum HashSerializationHelper {
    /// SHA-256 Hash
    SHA256(SHA256),
}

impl From<Hash> for HashSerializationHelper {
    fn from(x: Hash) -> Self {
        match x {
            Hash::SHA256(a) => HashSerializationHelper::SHA256(a),
        }
    }
}

impl From<HashSerializationHelper> for Hash {
    fn from(x: HashSerializationHelper) -> Self {
        match x {
            HashSerializationHelper::SHA256(a) => Hash::SHA256(a),
        }
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(&self)
        } else {
            HashSerializationHelper::from(*self).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Hash, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            String::deserialize(deserializer)?
                .parse::<Hash>()
                .map_err(de::Error::custom)
        } else {
            HashSerializationHelper::deserialize(deserializer).map(Into::into)
        }
    }
}

#[derive(Deserialize, Serialize)]
struct PublicKeyHashSerializationHelper {
    hash: [u8; 20],
}

impl From<PublicKeyHash> for PublicKeyHashSerializationHelper {
    fn from(x: PublicKeyHash) -> Self {
        Self { hash: x.hash }
    }
}

impl From<PublicKeyHashSerializationHelper> for PublicKeyHash {
    fn from(x: PublicKeyHashSerializationHelper) -> Self {
        PublicKeyHash { hash: x.hash }
    }
}

impl Serialize for PublicKeyHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(&self.bech32(get_environment()))
        } else {
            PublicKeyHashSerializationHelper::from(*self).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for PublicKeyHash {
    fn deserialize<D>(deserializer: D) -> Result<PublicKeyHash, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            PublicKeyHash::from_bech32(get_environment(), &s).map_err(de::Error::custom)
        } else {
            PublicKeyHashSerializationHelper::deserialize(deserializer).map(Into::into)
        }
    }
}

#[derive(Deserialize, Serialize)]
struct OutputPointerSerializationHelper {
    pub transaction_id: Hash,
    pub output_index: u32,
}

impl From<OutputPointer> for OutputPointerSerializationHelper {
    fn from(x: OutputPointer) -> Self {
        OutputPointerSerializationHelper {
            transaction_id: x.transaction_id,
            output_index: x.output_index,
        }
    }
}

impl From<OutputPointerSerializationHelper> for OutputPointer {
    fn from(x: OutputPointerSerializationHelper) -> Self {
        OutputPointer {
            transaction_id: x.transaction_id,
            output_index: x.output_index,
        }
    }
}

impl Serialize for OutputPointer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(&self)
        } else {
            OutputPointerSerializationHelper::from(self.clone()).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for OutputPointer {
    fn deserialize<D>(deserializer: D) -> Result<OutputPointer, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            String::deserialize(deserializer)?
                .parse::<OutputPointer>()
                .map_err(de::Error::custom)
        } else {
            OutputPointerSerializationHelper::deserialize(deserializer).map(Into::into)
        }
    }
}

/// Serialization helper for `GenesisBlockInfo`.
#[derive(Deserialize)]
pub struct GenesisBlock {
    alloc: Vec<Vec<GenesisValueTransferOutput>>,
}

#[derive(Deserialize)]
struct GenesisValueTransferOutput {
    #[serde(deserialize_with = "deserialize_from_str")]
    address: PublicKeyHash,
    #[serde(deserialize_with = "deserialize_from_str")]
    value: u64,
    #[serde(deserialize_with = "deserialize_from_str")]
    timelock: u64,
}

// https://serde.rs/attr-bound.html
/// Deserialize a type `S` by deserializing a string, then using the `FromStr`
/// impl of `S` to create the result. The generic type `S` is not required to
/// implement `Deserialize`.
fn deserialize_from_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
    S: FromStr,
    S::Err: Display,
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    S::from_str(s).map_err(de::Error::custom)
}

impl From<GenesisValueTransferOutput> for ValueTransferOutput {
    fn from(x: GenesisValueTransferOutput) -> Self {
        Self {
            pkh: x.address,
            value: x.value,
            time_lock: x.timelock,
        }
    }
}

impl From<GenesisBlock> for GenesisBlockInfo {
    fn from(x: GenesisBlock) -> Self {
        Self {
            alloc: x
                .alloc
                .into_iter()
                .map(|alloc| alloc.into_iter().map(ValueTransferOutput::from).collect())
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
enum UtxoSelectionStrategyName {
    #[serde(rename = "random", alias = "Random")]
    Random,
    #[serde(rename = "big_first", alias = "BigFirst")]
    BigFirst,
    #[serde(rename = "small_first", alias = "SmallFirst")]
    SmallFirst,
}

impl From<UtxoSelectionStrategyName> for UtxoSelectionStrategy {
    fn from(x: UtxoSelectionStrategyName) -> UtxoSelectionStrategy {
        match x {
            UtxoSelectionStrategyName::Random => UtxoSelectionStrategy::Random { from: None },
            UtxoSelectionStrategyName::BigFirst => UtxoSelectionStrategy::BigFirst { from: None },
            UtxoSelectionStrategyName::SmallFirst => {
                UtxoSelectionStrategy::SmallFirst { from: None }
            }
        }
    }
}

impl<'a> From<&'a UtxoSelectionStrategy> for UtxoSelectionStrategyName {
    fn from(x: &'a UtxoSelectionStrategy) -> UtxoSelectionStrategyName {
        match x {
            UtxoSelectionStrategy::Random { .. } => UtxoSelectionStrategyName::Random,
            UtxoSelectionStrategy::BigFirst { .. } => UtxoSelectionStrategyName::BigFirst,
            UtxoSelectionStrategy::SmallFirst { .. } => UtxoSelectionStrategyName::SmallFirst,
        }
    }
}

// #[serde(untagged)] is a great way to allow serializing as one of many possible representations,
// however the error messages are really bad:
// "data did not match any variant of untagged enum UtxoSelectionStrategyHelper"
// There is a pull request in serde to fix this:
// https://github.com/serde-rs/serde/pull/1544
// But it is not merged, so the alternative is to manually implement a visitor to deserialize this.
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum UtxoSelectionStrategyHelper {
    String(UtxoSelectionStrategyName),
    Object(UtxoSelectionStrategyObject),
}

#[derive(Deserialize, Serialize)]
struct UtxoSelectionStrategyObject {
    strategy: UtxoSelectionStrategyName,
    from: Option<PublicKeyHash>,
}

impl From<UtxoSelectionStrategyHelper> for UtxoSelectionStrategy {
    fn from(x: UtxoSelectionStrategyHelper) -> UtxoSelectionStrategy {
        match x {
            UtxoSelectionStrategyHelper::String(name) => name.into(),
            UtxoSelectionStrategyHelper::Object(UtxoSelectionStrategyObject { strategy, from }) => {
                let mut strategy = UtxoSelectionStrategy::from(strategy);
                *strategy.get_from_mut() = from;
                strategy
            }
        }
    }
}

impl<'a> From<&'a UtxoSelectionStrategy> for UtxoSelectionStrategyHelper {
    fn from(x: &'a UtxoSelectionStrategy) -> UtxoSelectionStrategyHelper {
        let name = UtxoSelectionStrategyName::from(x);
        match x.get_from() {
            None => {
                // If from field is None, serialize self as string
                UtxoSelectionStrategyHelper::String(name)
            }
            Some(from) => {
                // If from field is Some, serialize as {"strategy": name, "from": from }
                UtxoSelectionStrategyHelper::Object(UtxoSelectionStrategyObject {
                    strategy: name,
                    from: Some(*from),
                })
            }
        }
    }
}

impl Serialize for UtxoSelectionStrategy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UtxoSelectionStrategyHelper::from(self).serialize(serializer)
    }
}

struct UtxoSelectionStrategyVisitor;

impl<'de> Visitor<'de> for UtxoSelectionStrategyVisitor {
    type Value = UtxoSelectionStrategy;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a string or an object")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        // Delegate implementation of visit_str to UtxoSelectionStrategyName
        UtxoSelectionStrategyName::deserialize(v.into_deserializer())
            .map(|x| UtxoSelectionStrategyHelper::String(x).into())
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // Delegate implementation of visit_map to UtxoSelectionStrategyObject
        UtxoSelectionStrategyObject::deserialize(de::value::MapAccessDeserializer::new(map))
            .map(|x| UtxoSelectionStrategyHelper::Object(x).into())
    }
}

impl<'de> Deserialize<'de> for UtxoSelectionStrategy {
    fn deserialize<D>(deserializer: D) -> Result<UtxoSelectionStrategy, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UtxoSelectionStrategyVisitor)
    }
}
