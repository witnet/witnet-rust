use self::schema::witnet;
use crate::types::IpAddress;
use crate::{chain, types};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use failure::{ensure, format_err, Error};
use protobuf::Message;
use std::convert::TryFrom;
use std::fmt::Debug;

pub mod schema;

/// Used for establishing correspondence between rust struct
/// and protobuf rust struct
pub trait ProtobufConvert: Sized {
    /// Type of the protobuf clone of Self
    type ProtoStruct;

    /// Struct -> ProtoStruct
    fn to_pb(&self) -> Self::ProtoStruct;

    /// ProtoStruct -> Struct
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error>;

    /// Struct -> ProtoStruct -> Bytes
    fn to_pb_bytes(&self) -> Result<Vec<u8>, Error>
    where
        Self::ProtoStruct: Message,
    {
        // Serialize
        self.to_pb().write_to_bytes().map_err(Into::into)
    }

    /// Bytes -> ProtoStruct -> Struct
    fn from_pb_bytes(bytes: &[u8]) -> Result<Self, Error>
    where
        Self::ProtoStruct: Message,
    {
        // Deserialize
        let mut a = Self::ProtoStruct::new();
        a.merge_from_bytes(bytes)?;
        Self::from_pb(a)
    }
}

impl ProtobufConvert for chain::RADType {
    type ProtoStruct = witnet::DataRequestOutput_RADRequest_RADType;

    fn to_pb(&self) -> Self::ProtoStruct {
        match self {
            chain::RADType::Unknown => witnet::DataRequestOutput_RADRequest_RADType::Unknown,
            chain::RADType::HttpGet => witnet::DataRequestOutput_RADRequest_RADType::HttpGet,
            chain::RADType::Rng => witnet::DataRequestOutput_RADRequest_RADType::Rng,
            chain::RADType::HttpPost => witnet::DataRequestOutput_RADRequest_RADType::HttpPost,
        }
    }

    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        Ok(match pb {
            witnet::DataRequestOutput_RADRequest_RADType::Unknown => chain::RADType::Unknown,
            witnet::DataRequestOutput_RADRequest_RADType::HttpGet => chain::RADType::HttpGet,
            witnet::DataRequestOutput_RADRequest_RADType::Rng => chain::RADType::Rng,
            witnet::DataRequestOutput_RADRequest_RADType::HttpPost => chain::RADType::HttpPost,
        })
    }
}

impl ProtobufConvert for chain::PublicKey {
    type ProtoStruct = witnet::PublicKey;

    fn to_pb(&self) -> Self::ProtoStruct {
        let mut m = witnet::PublicKey::new();
        let mut v = vec![];
        v.extend(&[self.compressed]);
        v.extend(&self.bytes);
        m.set_public_key(v);

        m
    }

    fn from_pb(mut pb: Self::ProtoStruct) -> Result<Self, Error> {
        let v = pb.take_public_key();
        ensure!(v.len() == 33, "Invalid array length");

        let mut bytes = [0; 32];
        bytes.copy_from_slice(&v[1..]);

        Ok(Self {
            compressed: v[0],
            bytes,
        })
    }
}

impl ProtobufConvert for types::Address {
    type ProtoStruct = witnet::Address;

    fn to_pb(&self) -> Self::ProtoStruct {
        let mut address = witnet::Address::new();
        let mut bytes = vec![];
        match self.ip {
            IpAddress::Ipv4 { ip } => {
                bytes.write_u32::<BigEndian>(ip).unwrap();
            }
            IpAddress::Ipv6 { ip0, ip1, ip2, ip3 } => {
                bytes.write_u32::<BigEndian>(ip0).unwrap();
                bytes.write_u32::<BigEndian>(ip1).unwrap();
                bytes.write_u32::<BigEndian>(ip2).unwrap();
                bytes.write_u32::<BigEndian>(ip3).unwrap();
            }
        }
        bytes.write_u16::<BigEndian>(self.port).unwrap();
        address.set_address(bytes);

        address
    }

    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        let mut bytes = pb.get_address();
        match bytes.len() {
            6 => {
                // Ipv4
                let ip = bytes.read_u32::<BigEndian>()?;
                let ip = types::IpAddress::Ipv4 { ip };
                let port = bytes.read_u16::<BigEndian>()?;

                Ok(types::Address { ip, port })
            }
            18 => {
                // Ipv6
                let ip0 = bytes.read_u32::<BigEndian>()?;
                let ip1 = bytes.read_u32::<BigEndian>()?;
                let ip2 = bytes.read_u32::<BigEndian>()?;
                let ip3 = bytes.read_u32::<BigEndian>()?;
                let port = bytes.read_u16::<BigEndian>()?;
                let ip = types::IpAddress::Ipv6 { ip0, ip1, ip2, ip3 };

                Ok(types::Address { ip, port })
            }
            _ => Err(format_err!("Invalid address size")),
        }
    }
}

impl ProtobufConvert for String {
    type ProtoStruct = Self;
    fn to_pb(&self) -> Self::ProtoStruct {
        self.clone()
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        Ok(pb)
    }
}

impl<T> ProtobufConvert for Vec<T>
where
    T: ProtobufConvert,
{
    type ProtoStruct = Vec<T::ProtoStruct>;
    fn to_pb(&self) -> Self::ProtoStruct {
        self.iter().map(ProtobufConvert::to_pb).collect()
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        pb.into_iter()
            .map(ProtobufConvert::from_pb)
            .collect::<Result<Vec<_>, _>>()
    }
}

impl ProtobufConvert for Vec<u8> {
    type ProtoStruct = Self;
    fn to_pb(&self) -> Self::ProtoStruct {
        self.clone()
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        Ok(pb)
    }
}

impl ProtobufConvert for [u8; 20] {
    type ProtoStruct = Vec<u8>;
    fn to_pb(&self) -> Self::ProtoStruct {
        self.to_vec()
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        ensure!(pb.len() == 20, "Invalid array length");
        let mut x = [0; 20];
        x.copy_from_slice(&pb);
        Ok(x)
    }
}

impl ProtobufConvert for [u8; 32] {
    type ProtoStruct = Vec<u8>;
    fn to_pb(&self) -> Self::ProtoStruct {
        self.to_vec()
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        ensure!(pb.len() == 32, "Invalid array length");
        let mut x = [0; 32];
        x.copy_from_slice(&pb);
        Ok(x)
    }
}

impl<T> ProtobufConvert for Option<T>
where
    T: ProtobufConvert + Default + Eq + Debug,
{
    type ProtoStruct = <T as ProtobufConvert>::ProtoStruct;
    fn to_pb(&self) -> Self::ProtoStruct {
        match self {
            Some(x) => x.to_pb(),
            None => T::default().to_pb(),
        }
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        let res = T::from_pb(pb)?;
        if res == T::default() {
            Ok(None)
        } else {
            Ok(Some(res))
        }
    }
}

macro_rules! impl_protobuf_convert_scalar {
    ($name:tt) => {
        impl ProtobufConvert for $name {
            type ProtoStruct = $name;
            fn to_pb(&self) -> Self::ProtoStruct {
                *self
            }
            fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
                Ok(pb)
            }
        }
    };
}

impl_protobuf_convert_scalar!(bool);
impl_protobuf_convert_scalar!(u32);
impl_protobuf_convert_scalar!(u64);
impl_protobuf_convert_scalar!(i32);
impl_protobuf_convert_scalar!(i64);
impl_protobuf_convert_scalar!(f32);
impl_protobuf_convert_scalar!(f64);

// Conflicts with Vec<u8>
/*
impl ProtobufConvert for u8 {
    type ProtoStruct = u32;
    fn to_pb(&self) -> Self::ProtoStruct {
        Self::ProtoStruct::from(*self)
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        ensure!(
            pb <= Self::ProtoStruct::from(Self::max_value()),
            "Integer out of range"
        );
        Ok(pb as Self)
    }
}
*/

impl ProtobufConvert for i8 {
    type ProtoStruct = i32;
    fn to_pb(&self) -> Self::ProtoStruct {
        Self::ProtoStruct::from(*self)
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        ensure!(
            pb <= Self::ProtoStruct::from(Self::max_value()),
            "Integer out of range"
        );
        Ok(Self::try_from(pb)?)
    }
}

impl ProtobufConvert for u16 {
    type ProtoStruct = u32;
    fn to_pb(&self) -> Self::ProtoStruct {
        Self::ProtoStruct::from(*self)
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        ensure!(
            pb <= Self::ProtoStruct::from(Self::max_value()),
            "Integer out of range"
        );
        Ok(Self::try_from(pb)?)
    }
}

impl ProtobufConvert for i16 {
    type ProtoStruct = i32;
    fn to_pb(&self) -> Self::ProtoStruct {
        Self::ProtoStruct::from(*self)
    }
    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        ensure!(
            pb <= Self::ProtoStruct::from(Self::max_value()),
            "Integer out of range"
        );
        Ok(Self::try_from(pb)?)
    }
}

impl ProtobufConvert for (String, String) {
    type ProtoStruct = witnet::StringPair;

    fn to_pb(&self) -> Self::ProtoStruct {
        let mut pb = Self::ProtoStruct::new();
        pb.set_left(self.0.clone());
        pb.set_right(self.1.clone());
        pb
    }

    fn from_pb(pb: Self::ProtoStruct) -> Result<Self, Error> {
        Ok((pb.get_left().to_string(), pb.get_right().to_string()))
    }
}
