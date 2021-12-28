//! Implement a human-friendly serialization for some types.
//! This allows us to use an efficient binary representation when serializing
//! to storage, but have a nice string-based representation when using JSON.
//!
//! For example, a `Hash` can be serialized as `[13, 53, 125, ...]` or as `"0d357d..."`.

// Ideally all this code would be generated with a `#[serde(human_readable_string)]` macro.

use crate::{
    chain::{
        GenesisBlockInfo, Hash, OutputPointer, PublicKeyHash, RADRetrieve, RADType,
        ValueTransferOutput, SHA256,
    },
    get_environment,
    utxo_pool::UtxoSelectionStrategy,
};
use serde::{
    de::{self, IntoDeserializer, MapAccess, SeqAccess, Visitor},
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

/// Serialization helper for `RADRetrieve`.
/// Serializes `RADRetrieve` as a 2-field struct: `(db_version: u32, rad_retrieve: RADRetrieve)`.
/// Deserializes `RADRetrieve` in a backwards compatible way with bincode.
#[derive(Serialize)]
struct RADRetrieveSerializationHelperVersioned(u32, RADRetrieveSerializationHelperBincode);

impl RADRetrieveSerializationHelperVersioned {
    const LATEST_VERSION: u32 = 3;
}

/// This should be the same as `RADRetrieve`, it exists because we want to use the automatically
/// derived serialization code.
#[derive(Serialize, Deserialize)]
struct RADRetrieveSerializationHelperJson {
    /// Kind of retrieval
    pub kind: RADType,
    /// URL
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Serialized RADON script
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub script: Vec<u8>,
    /// Body of a HTTP-POST request
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<u8>,
    /// Extra headers of a HTTP-GET or HTTP-POST request
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(String, String)>,
}

/// This should be the same as `RADRetrieveSerializationHelperJson`, but bincode does not support
/// the serde attribute `skip_serializing_if`.
#[derive(Serialize, Deserialize)]
struct RADRetrieveSerializationHelperBincode {
    /// Kind of retrieval
    pub kind: RADType,
    /// URL
    pub url: String,
    /// Serialized RADON script
    pub script: Vec<u8>,
    /// Body of a HTTP-POST request
    pub body: Vec<u8>,
    /// Extra headers of a HTTP-GET or HTTP-POST request
    pub headers: Vec<(String, String)>,
}

impl From<RADRetrieve> for RADRetrieveSerializationHelperVersioned {
    fn from(x: RADRetrieve) -> Self {
        let db_version = Self::LATEST_VERSION;

        let RADRetrieve {
            kind,
            url,
            script,
            body,
            headers,
        } = x;

        Self(
            db_version,
            RADRetrieveSerializationHelperBincode {
                kind,
                url,
                script,
                body,
                headers,
            },
        )
    }
}

impl From<RADRetrieveSerializationHelperVersioned> for RADRetrieve {
    fn from(x: RADRetrieveSerializationHelperVersioned) -> Self {
        let RADRetrieveSerializationHelperVersioned(db_version, rad_retrieve) = x;

        assert_eq!(
            db_version,
            RADRetrieveSerializationHelperVersioned::LATEST_VERSION
        );

        let RADRetrieveSerializationHelperBincode {
            kind,
            url,
            script,
            body,
            headers,
        } = rad_retrieve;

        Self {
            kind,
            url,
            script,
            body,
            headers,
        }
    }
}

impl From<RADRetrieveSerializationHelperBincode> for RADRetrieveSerializationHelperVersioned {
    fn from(rad_retrieve: RADRetrieveSerializationHelperBincode) -> Self {
        let db_version = Self::LATEST_VERSION;

        Self(db_version, rad_retrieve)
    }
}

impl From<RADRetrieve> for RADRetrieveSerializationHelperJson {
    fn from(x: RADRetrieve) -> Self {
        let RADRetrieve {
            kind,
            url,
            script,
            body,
            headers,
        } = x;

        Self {
            kind,
            url,
            script,
            body,
            headers,
        }
    }
}

impl From<RADRetrieveSerializationHelperJson> for RADRetrieve {
    fn from(x: RADRetrieveSerializationHelperJson) -> Self {
        let RADRetrieveSerializationHelperJson {
            kind,
            url,
            script,
            body,
            headers,
        } = x;

        Self {
            kind,
            url,
            script,
            body,
            headers,
        }
    }
}

struct RADRetrieveSerializationHelperVersionedVisitor;

impl<'de> Visitor<'de> for RADRetrieveSerializationHelperVersionedVisitor {
    type Value = RADRetrieveSerializationHelperVersioned;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "struct RADRetrieveSerializationHelperVersioned")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        // Always deserialize as the latest version
        let latest_version = Self::Value::LATEST_VERSION;

        // Ensure backwards compatibility when using bincode.
        // The first element of the old version of RADRetrieve was a RADType, which is a simple enum
        // with 3 fields. This enum is serialized by bincode as a u32, with the possible values 0,
        // 1, 2.
        // The new helper type RADRetrieveSerializationHelperVersioned is serialized with a u32
        // db_version field as the first field.
        // Therefore, we can deserialize the first u32 value, and select the correct strategy
        // depending on it. If the db_version is 0, 1, or 2, this is the old version RADRetrieve so
        // we need to deserialize the two missing fields (url, script) next. Otherwise, this is the
        // actual db_version value, so we can use it to select the correct helper.
        // Currently there is only one helper: RADRetrieveSerializationHelperBincode, which uses
        // db_version 3.
        let db_version: u32 = seq
            .next_element()?
            .ok_or_else(|| de::Error::missing_field("db_version"))?;

        match db_version {
            0 | 1 | 2 => {
                let kind = match db_version {
                    0 => RADType::Unknown,
                    1 => RADType::HttpGet,
                    2 => RADType::Rng,
                    _ => unreachable!(),
                };
                // In the original definition, `RADRetrieve` was a 3-field struct, and in this
                // implementation it is a 2-field struct. Since we have already deserialized the
                // value of the first field, we treat the 2 remaining fields `url` and `script` as
                // if it was one field with value `(url, script)`.
                // Treating them as 2 separate fields does not work because `seq.next_element()`
                // returns `None` for the second element, because
                // `RADRetrieveSerializationHelperVersioned` does only have 2 fields.
                // This could cause problems in some serialization formats where `(a, b, c)` is not
                // equivalent to `(a, (b, c))`, but bincode seems to be happy, and we only care
                // about being backwards compatible with bincode.
                let (url, script): (String, Vec<u8>) = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("rad_retrieve"))?;

                // The new fields `body` and `headers` which were missing in this version of
                // `RADRetrieve` will have the default value
                let rad_retrieve = RADRetrieveSerializationHelperBincode {
                    kind,
                    url,
                    script,
                    body: vec![],
                    headers: vec![],
                };

                Ok(RADRetrieveSerializationHelperVersioned(
                    latest_version,
                    rad_retrieve,
                ))
            }
            3 => {
                // Version 3: deserialize as 2-field struct: `(db_version, rad_retrieve)`.
                let rad_retrieve: RADRetrieveSerializationHelperBincode = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("rad_retrieve"))?;

                Ok(RADRetrieveSerializationHelperVersioned(
                    latest_version,
                    rad_retrieve,
                ))
            }
            unknown_version => Err(de::Error::custom(format!(
                "RADRetrieve: unknown db_version {}, expected one of 0, 1, 2, 3",
                unknown_version
            ))),
        }
    }
}

impl<'de> Deserialize<'de> for RADRetrieveSerializationHelperVersioned {
    fn deserialize<D>(deserializer: D) -> Result<RADRetrieveSerializationHelperVersioned, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize using visitor. This is a 2-field tuple struct:
        // (db_version: u32, rad_retrieve: RADRetrieve)
        deserializer.deserialize_tuple_struct(
            "RADRetrieveSerializationHelperVersioned",
            2,
            RADRetrieveSerializationHelperVersionedVisitor,
        )
    }
}

impl Serialize for RADRetrieve {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            // Serialize skipping default fields to improve readability
            RADRetrieveSerializationHelperJson::from(self.clone()).serialize(serializer)
        } else {
            // Serialize by prepending a `db_version: u32` field, to ensure easier migrations in the
            // future. This always serializes in the format of the latest version.
            RADRetrieveSerializationHelperVersioned::from(self.clone()).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for RADRetrieve {
    fn deserialize<D>(deserializer: D) -> Result<RADRetrieve, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            // Deserialize by assuming that missing fields have default value
            RADRetrieveSerializationHelperJson::deserialize(deserializer).map(Into::into)
        } else {
            // Deserialize by reading the format version. Ensures backwards compatibility.
            RADRetrieveSerializationHelperVersioned::deserialize(deserializer).map(Into::into)
        }
    }
}
