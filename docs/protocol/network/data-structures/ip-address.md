# IP Address

IP addresses in Witnet protocol may be IPv4 or IPv6 and they are formatted as follows:

- IPv4:

    | Field  | Type  | Description                                |
    | ------ | :---: | ------------------------------------------ |
    | `ipv4` | `u32` | IPv4 address of the peer                   |
    | `port` | `u16` | Port number in which the peer is listening |

- IPv6:

    | Field  | Type       | Description                                |
    | ------ | :--------: | ------------------------------------------ |
    | `ipv6` | `[u32; 4]` | IPv6 address of the peer                   |
    | `port` | `u16`      | Port number in which the peer is listening |
