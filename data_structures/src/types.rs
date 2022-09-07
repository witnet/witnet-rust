use std::fmt;

/// Re-export `WrapingAdd` trait from `num_traits`.
pub use num_traits::ops::wrapping::WrappingAdd;
use serde::{Deserialize, Serialize};

use crate::{
    chain::{Block, CheckpointBeacon, Hashable, InventoryEntry, SuperBlock, SuperBlockVote},
    proto::{schema::witnet, ProtobufConvert},
    transaction::Transaction,
};

/// Witnet's protocol messages
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Message")]
pub struct Message {
    pub kind: Command,
    pub magic: u16,
}

/// Commands for the Witnet's protocol messages
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Message_Command")]
// FIXME(#649): Remove clippy skip error
#[allow(clippy::large_enum_variant)]
pub enum Command {
    // Peer discovery messages
    GetPeers(GetPeers),
    Peers(Peers),

    // Handshake messages
    Verack(Verack),
    Version(Version),

    // Inventory messages
    Block(Block),
    Transaction(Transaction),
    SuperBlock(SuperBlock),
    InventoryAnnouncement(InventoryAnnouncement),
    InventoryRequest(InventoryRequest),
    LastBeacon(LastBeacon),

    // Superblock
    SuperBlockVote(SuperBlockVote),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::GetPeers(_) => f.write_str("GET_PEERS"),
            Command::Peers(_) => f.write_str("PEERS"),
            Command::Verack(_) => f.write_str("VERACK"),
            Command::Version(_) => f.write_str("VERSION"),
            Command::Block(block) => write!(
                f,
                "BLOCK: #{}: {}",
                block.block_header.beacon.checkpoint,
                block.hash()
            ),
            Command::InventoryAnnouncement(_) => f.write_str("INVENTORY_ANNOUNCEMENT"),
            Command::InventoryRequest(_) => f.write_str("INVENTORY_REQUEST"),
            Command::LastBeacon(LastBeacon {
                highest_block_checkpoint: h,
                highest_superblock_checkpoint: s,
            }) => write!(
                f,
                "LAST_BEACON: Block: #{}: {} Superblock: #{}: {}",
                h.checkpoint, h.hash_prev_block, s.checkpoint, s.hash_prev_block
            ),
            Command::Transaction(tx) => {
                match tx {
                    Transaction::Commit(_) => f.write_str("COMMIT_TRANSACTION")?,
                    Transaction::ValueTransfer(_) => f.write_str("VALUE_TRANSFER_TRANSACTION")?,
                    Transaction::DataRequest(_) => f.write_str("DATA_REQUEST_TRANSACTION")?,
                    Transaction::Reveal(_) => f.write_str("REVEAL_TRANSACTION")?,
                    Transaction::Tally(_) => f.write_str("TALLY_TRANSACTION")?,
                    Transaction::Mint(_) => f.write_str("MINT_TRANSACTION")?,
                }
                write!(f, ": {}", tx.hash())
            }
            Command::SuperBlockVote(sbv) => write!(
                f,
                "SUPERBLOCK_VOTE {} #{}: {}",
                sbv.secp256k1_signature.public_key.pkh(),
                sbv.superblock_index,
                sbv.superblock_hash
            ),
            Command::SuperBlock(sb) => write!(f, "SUPERBLOCK #{}: {}", sb.index, sb.hash()),
        }
    }
}

///////////////////////////////////////////////////////////
// PEER DISCOVERY MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::GetPeers")]
pub struct GetPeers;

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Peers")]
pub struct Peers {
    pub peers: Vec<Address>,
}

///////////////////////////////////////////////////////////
// HANDSHAKE MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Verack")]
pub struct Verack;

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Version")]
pub struct Version {
    pub version: u32,
    pub timestamp: i64,
    pub capabilities: u64,
    pub sender_address: Address,
    pub receiver_address: Address,
    pub user_agent: String,
    pub nonce: u64,
    pub beacon: LastBeacon,
}

///////////////////////////////////////////////////////////
// INVENTORY MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::InventoryAnnouncement")]
pub struct InventoryAnnouncement {
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::InventoryRequest")]
pub struct InventoryRequest {
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Clone, ProtobufConvert, Serialize, Hash)]
#[protobuf_convert(pb = "witnet::LastBeacon")]
pub struct LastBeacon {
    pub highest_block_checkpoint: CheckpointBeacon,
    pub highest_superblock_checkpoint: CheckpointBeacon,
}

///////////////////////////////////////////////////////////
// AUX TYPES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum IpAddress {
    Ipv4 {
        ip: u32,
    },
    Ipv6 {
        ip0: u32,
        ip1: u32,
        ip2: u32,
        ip3: u32,
    },
}

impl Default for IpAddress {
    fn default() -> Self {
        Self::Ipv4 { ip: 0 }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Copy)]
pub struct Address {
    pub ip: IpAddress,
    pub port: u16,
}

/// A generic and iterable generator of sequential IDs.
pub struct SequentialId<T>(T);

impl<T> SequentialId<T>
where
    T: Copy + From<u8> + std::ops::Add + num_traits::ops::wrapping::WrappingAdd,
{
    /// Create a new sequence, starting with a specified value.
    ///
    /// This initial value will be the one to be returned in a first call to `next()` or `next_id()`.
    #[inline]
    pub fn initialize(initial_value: T) -> Self {
        Self(initial_value)
    }
}

impl<T> std::iter::Iterator for SequentialId<T>
where
    T: Copy + From<u8> + std::ops::Add + WrappingAdd,
{
    type Item = T;

    /// Returns the next sequencial ID.
    #[inline]
    fn next(&mut self) -> Option<T> {
        let current = self.0;
        self.0 = self.0.wrapping_add(&T::from(1u8));

        Some(current)
    }
}

/// Provides a trait modeling a basic visitor pattern, plus multiple generic implementations.
pub mod visitor {
    use std::marker::PhantomData;

    /// A trait modeling a basic visitor pattern.
    pub trait Visitor {
        type Visitable;

        /// Operate on an instance of the `Visitable` type.
        fn visit(&mut self, visitable: &Self::Visitable);
    }

    /// A trait modeling a visitor that can hold state.
    pub trait StatefulVisitor: Visitor {
        type State;

        /// Turn a visitor into its internal state.
        fn take_state(self) -> Self::State;
    }

    /// A visitor that visits values of type `T` using two underlying visitors.
    pub struct CombinedVisitor<T, A, B>
    where
        A: Visitor<Visitable = T>,
        B: Visitor<Visitable = T>,
    {
        a: A,
        b: B,
    }

    #[cfg(test)]
    impl<T, A, B> CombinedVisitor<T, A, B>
    where
        A: Visitor<Visitable = T>,
        B: Visitor<Visitable = T>,
    {
        /// Construct a `DualVisitor` from an instance of `A: Visitor<Visitable = T>` and
        /// `B: Visitor<Visitable = T>`.
        fn from_initialized_visitors(a: A, b: B) -> Self {
            Self { a, b }
        }
    }

    /// Implement `Visitor` for the `DualVisitor`.
    impl<T, A, B> Visitor for CombinedVisitor<T, A, B>
    where
        A: Visitor<Visitable = T>,
        B: Visitor<Visitable = T>,
    {
        type Visitable = T;

        fn visit(&mut self, visitable: &Self::Visitable) {
            self.a.visit(visitable);
            self.b.visit(visitable);
        }
    }

    impl<T, A, B> StatefulVisitor for CombinedVisitor<T, A, B>
    where
        A: StatefulVisitor + Visitor<Visitable = T>,
        B: StatefulVisitor + Visitor<Visitable = T>,
    {
        type State = (A::State, B::State);

        fn take_state(self) -> Self::State {
            (self.a.take_state(), self.b.take_state())
        }
    }

    /// A generic `Visitor` that can be constructed with virtually any store and visittee type.
    ///
    /// This is somehow similar to what `fold` does on iterators.
    pub struct GenericVisitor<S, V, F>
    where
        F: FnMut(&mut S, &V),
    {
        state: S,
        visitor_fn: F,
        visitable_type: PhantomData<V>,
    }

    impl<'a, S, V, F> GenericVisitor<S, V, F>
    where
        F: FnMut(&mut S, &V),
    {
        /// Allow constructing a `GenericVisitor` from an initial state and a visitor function that
        /// takes a mutable reference to the state plus individual values to visit.
        pub fn from_state_and_fn(state: S, visitor_fn: F) -> Self {
            Self {
                state,
                visitor_fn,
                visitable_type: PhantomData,
            }
        }
    }

    /// Implement `Visitor` for `GenericVisitor`.
    impl<S, V, F> Visitor for GenericVisitor<S, V, F>
    where
        F: FnMut(&mut S, &V),
    {
        type Visitable = V;

        fn visit(&mut self, visitable: &Self::Visitable) {
            (self.visitor_fn)(&mut self.state, visitable)
        }
    }

    impl<S, V, F> StatefulVisitor for GenericVisitor<S, V, F>
    where
        F: FnMut(&mut S, &V),
    {
        type State = S;

        fn take_state(self) -> Self::State {
            self.state
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[derive(Default)]
        struct AdditionVisitor<T>(pub T)
        where
            T: Copy + std::ops::AddAssign;

        impl<T> Visitor for AdditionVisitor<T>
        where
            T: Copy + std::ops::AddAssign,
        {
            type Visitable = T;

            fn visit(&mut self, visitable: &Self::Visitable) {
                self.0 += *visitable;
            }
        }

        impl<T> StatefulVisitor for AdditionVisitor<T>
        where
            T: Copy + std::ops::AddAssign,
        {
            type State = T;

            fn take_state(self) -> Self::State {
                self.0
            }
        }

        #[derive(Default)]
        struct ConcatVisitor<T>(pub String, PhantomData<T>)
        where
            T: std::fmt::Display;

        impl<T> Visitor for ConcatVisitor<T>
        where
            T: std::fmt::Display,
        {
            type Visitable = T;

            fn visit(&mut self, visitable: &Self::Visitable) {
                self.0 += visitable.to_string().as_str();
            }
        }

        impl<T> StatefulVisitor for ConcatVisitor<T>
        where
            T: std::fmt::Display,
        {
            type State = String;

            fn take_state(self) -> Self::State {
                self.0
            }
        }

        #[test]
        pub fn basic_visiting() {
            let mut visitor = AdditionVisitor(0usize);
            for i in 1usize..=100 {
                visitor.visit(&i);
            }
            let addition = visitor.0;
            let expected = 5050usize;

            assert_eq!(addition, expected);
        }

        #[test]
        pub fn dual_visiting() {
            let a = AdditionVisitor::default();
            let b = ConcatVisitor::default();
            let mut combined_visitor = CombinedVisitor::from_initialized_visitors(a, b);
            for i in 1usize..=100 {
                combined_visitor.visit(&i);
            }
            let (addition, concatenation) = combined_visitor.take_state();
            let expected_addition = 5050usize;
            let expected_concatenation = "123456789101112131415161718192021222324252627282930313233343536373839404142434445464748495051525354555657585960616263646566676869707172737475767778798081828384858687888990919293949596979899100";

            assert_eq!(addition, expected_addition);
            assert_eq!(concatenation, expected_concatenation);
        }

        #[test]
        pub fn generic_visiting() {
            let mut doubling_fn = |state: &mut Vec<usize>, value: &usize| state.push(value * 2);
            let mut doubling_visitor =
                GenericVisitor::from_state_and_fn(Vec::<usize>::new(), &mut doubling_fn);
            for i in 1usize..=10 {
                doubling_visitor.visit(&i)
            }
            let doubled = doubling_visitor.take_state();
            let expected = vec![2usize, 4, 6, 8, 10, 12, 14, 16, 18, 20];

            assert_eq!(doubled, expected);
        }
    }
}
