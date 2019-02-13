# Serialization

An open protocol needs portable serialization to easily enable
alternative implementations.

Witnet uses [Protocol Buffers][protobuf] ([version 3][protobuf3]) to
achieve this goal.

The Witnet protocol schema is available [here][schema].

## Why Protocol Buffers?

At the time of this writing Protocol Buffers is best alternative with support
for a standard schema. It has wide support for most popular programming
languages and supports the most common data structures.

## Custom encodings

Sometimes Protocol Buffers do not provide the necessary flexibility when
defining custom types. For example, Protocol Buffers do not support
fixed size arrays so the verification that a hash has the correct size
is done at a higher level.

All the structures which use a custom serialization can be found in
[proto/mod.rs][protomodrs]. The following structures are represented as
bytes in the protobuf schema:

(`||` denotes concatenation)

```
Signature: bytes
[u8; 65] => r || s || v

Address: bytes
[u8; 6]  => (Ipv4) ip || port 
[u8; 18] => (Ipv6) ip0 || ip1 || ip2 || ip3 || port
```

## Integers

Another important point is integer support: in Protocol Buffers the
smallest integer size is 32 bits. But a `uint32` uses variable length
encoding, meaning that it can take from 1 to 5 bytes to encode a number
depending on its magnitude. Fixed-size integers are also available, as
`fixed32` and `fixed64`.

The default integer mapping is the following:

| Rust | Protobuf |
|:-----|:---------|
| `u16` | `uint32` |
| `u32` | `fixed32` |
| `u64` | `fixed64` |
| `i8` | `sint32` |
| `i16` | `sint32` |
| `i32` | `sfixed32` |
| `i64` | `sfixed64` |

However 32 and 64-bit integers can also be encoded using variable length
encoding when that makes sense, for example when the number is expected
to be low. The Rust structs do not need any modifications for this type
of changes, a `u32` can be converted from and into a `sfixed32` as well
as a `sint32`.

## Hashing

It is possible to construct two different protobuf messages which
decode to the same value. In order to ensure that these two
messages have the same hash, the message bytes are not hashed directly:
they are first decoded from protobuf to Rust structs, running the
necessary validations, then encoded again as protobuf, and that new
encoding is hashed.

[protobuf]: https://developers.google.com/protocol-buffers
[protobuf3]: https://developers.google.com/protocol-buffers/docs/proto3
[schema]: https://github.com/witnet/witnet-rust/blob/master/schemas/witnet/witnet.proto
[protomodrs]: https://github.com/witnet/witnet-rust/blob/master/data_structures/src/proto/mod.rs
