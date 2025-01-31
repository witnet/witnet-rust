use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, EnumString, IntoStaticStr};

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    EnumIter,
    EnumString,
    Eq,
    Hash,
    IntoStaticStr,
    PartialEq,
    serde::Deserialize,
    serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "snake_case")]
pub enum Capability {
    /// The base block mining and superblock voting capability
    #[default]
    Mining = 0,
    /// The universal HTTP GET / HTTP POST / WIP-0019 RNG capability
    Witnessing = 1,
}

pub const ALL_CAPABILITIES: [Capability; 2] = [Capability::Mining, Capability::Witnessing];

#[derive(Copy, Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CapabilityMap<T>
where
    T: Default,
{
    pub mining: T,
    pub witnessing: T,
}

impl<T> CapabilityMap<T>
where
    T: Copy + Default,
{
    #[inline]
    pub fn get(&self, capability: Capability) -> T {
        match capability {
            Capability::Mining => self.mining,
            Capability::Witnessing => self.witnessing,
        }
    }

    #[inline]
    pub fn update(&mut self, capability: Capability, value: T) {
        match capability {
            Capability::Mining => self.mining = value,
            Capability::Witnessing => self.witnessing = value,
        }
    }

    #[inline]
    pub fn update_all(&mut self, value: T) {
        self.mining = value;
        self.witnessing = value;
    }
}
