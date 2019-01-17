/// Function that parses a hex string into vec
pub fn parse_hex(hex_asm: &str) -> Vec<u8> {
    let mut hex_bytes = hex_asm
        .as_bytes()
        .iter()
        .filter_map(|b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        })
        .fuse();

    let mut bytes = Vec::new();
    while let (Some(h), Some(l)) = (hex_bytes.next(), hex_bytes.next()) {
        bytes.push(h << 4 | l)
    }

    bytes
}

#[test]
fn parse_hex_test() {
    let result = parse_hex("0123456789abcdefABCDEF");
    let expected = vec![1, 35, 69, 103, 137, 171, 205, 239, 171, 205, 239];

    assert_eq!(result, expected);
}
