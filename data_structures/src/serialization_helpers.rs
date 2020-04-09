//! Implement a human-friendly serialization for some types.
//! This allows us to use an efficient binary representation when serializing
//! to storage, but have a nice string-based representation when using JSON.
//!
//! For example, a `Hash` can be serialized as `[13, 53, 125, ...]` or as `"0d357d..."`.

// Ideally all this code would be generated with a `#[serde(human_readable_string)]` macro.

use crate::chain::{
    GenesisBlockInfo, Hash, OutputPointer, PublicKeyHash, ValueTransferOutput, SHA256,
};
use crate::get_environment;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt::Display, str::FromStr};

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

impl Into<Hash> for HashSerializationHelper {
    fn into(self) -> Hash {
        match self {
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

impl Into<PublicKeyHash> for PublicKeyHashSerializationHelper {
    fn into(self) -> PublicKeyHash {
        PublicKeyHash { hash: self.hash }
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

impl Into<OutputPointer> for OutputPointerSerializationHelper {
    fn into(self) -> OutputPointer {
        OutputPointer {
            transaction_id: self.transaction_id,
            output_index: self.output_index,
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
            multiplier: 1,
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
