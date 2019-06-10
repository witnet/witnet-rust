use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Protected;

impl<'de> Deserialize<'de> for Protected {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<u8>::deserialize(deserializer).map(Protected::new)
    }
}

impl Serialize for Protected {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.as_ref())
    }
}
