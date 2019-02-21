# IP Address

IP addresses in Witnet protocol may be IPv4 or IPv6 and they are encoded
as bytes, as a concatenation of ip and port.

The kind is inferred based on the length: 6 bytes for IPv4 and
18 bytes for IPv6. The fields are encoded using Big-Endian
representation.

(`||` denotes concatenation)

```
[u8; 6]  => (Ipv4) ip || port 
[u8; 18] => (Ipv6) ip0 || ip1 || ip2 || ip3 || port
```