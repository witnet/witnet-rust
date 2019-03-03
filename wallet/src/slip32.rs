//! Slip32 implementation

use crate::key;

/// Byte sizes for each field used in the serialization format
// pub struct SerializationFormat {
//     version: u8,
//     depth: u8,
//     fingerpint: u8,
//     child_number: u8,
//     chain_code: u8,
//     key: u8
// }

/// Export a BIP32 extended key to the SLIP32 format
pub fn export<K>(path: key::KeyPath, extkey: key::ExtendedKey<K>) -> String
where
    K: key::Key,
{
    let depth = path.depth();
    let mut buffer = Vec::with_capacity(buffer_size(depth));

    // 1 byte for depth
    buffer[0] = depth as u8;

    // 4 bytes for each child number in the derivation path
    for (level, child_number) in path.iter().enumerate() {
        let start = 1 + level * 4;
        let end = start + 4;
        buffer.splice(start..end, child_number.to_bytes().iter().cloned());
    }

    // 32 bytes for chain code
    {
        let start = 1 + depth * 4;
        let end = start + 32;
        buffer.splice(start..end, extkey.chain_code.iter().cloned());
    }

    // 33 bytes for key
    //
    // TODO: Find a way to get access to secp256k1::SecretKey bytes,
    // at the moment it requires creating a custom serde Serializer
    {
        let start = 33 + depth * 4;
        let end = start + 33;
        // buffer.splice(start..end, extkey.key...
    }

    unimplemented!()
}

/// Import a BIP32 extended key in the SLIP32 format
pub fn import<K>(exported: String) -> key::ExtendedKey<K>
where
    K: key::Key,
{
    unimplemented!()
}

/// Calculate the size of the buffer that will contain the serialized key.
/// 1 byte: depth
/// 32 bytes: chain code
/// 33 bytes: the key
/// 4 bytes for each key in the keypath
fn buffer_size(depth: usize) -> usize {
    66 + depth * 4
}
