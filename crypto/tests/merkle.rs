use witnet_crypto::hash::{calculate_sha256, Sha256};
use witnet_crypto::merkle::merkle_tree_root;

#[test]
fn empty() {
    // An empty merkle tree results in a "empty" hash, the hash of an empty array
    let empty_hash = calculate_sha256(b"");
    assert_eq!(merkle_tree_root(&[]), empty_hash);
}

#[test]
fn one() {
    let a = calculate_sha256(b"a");
    assert_eq!(merkle_tree_root(&[a]), a);
}

// Helper function to test hash order
fn hash_concat(Sha256(a): Sha256, Sha256(b): Sha256) -> Sha256 {
    let mut h = a.to_vec();
    h.extend(&b);
    calculate_sha256(&h)
}

#[test]
fn two() {
    let a = [0x00; 32];
    let b = [0xFF; 32];
    let a = Sha256(a);
    let b = Sha256(b);

    // expected:
    // python -c "import sys; sys.stdout.write('\x00' * 32 + '\xFF' * 32)" | sha256sum
    let expected = [
        0xbb, 0xa9, 0x1c, 0xa8, 0x5d, 0xc9, 0x14, 0xb2, 0xec, 0x3e, 0xfb, 0x9e, 0x16, 0xe7, 0x26,
        0x7b, 0xf9, 0x19, 0x3b, 0x14, 0x35, 0x0d, 0x20, 0xfb, 0xa8, 0xa8, 0xb4, 0x06, 0x73, 0x0a,
        0xe3, 0x0a,
    ];
    let expected = Sha256(expected);
    assert_eq!(merkle_tree_root(&[a, b]), expected);

    // Test the hash_concat function
    let expected2 = hash_concat(a, b);
    assert_eq!(expected, expected2);
}

#[test]
fn manual_hash_test() {
    let a = Sha256([0x00; 32]);
    let b = Sha256([0x11; 32]);
    let c = Sha256([0x22; 32]);
    let d = Sha256([0x33; 32]);
    let e = Sha256([0x44; 32]);
    let f = Sha256([0x55; 32]);
    let g = Sha256([0x66; 32]);

    let h = hash_concat;
    // Verify the expected hash by manually hashing the elements in order
    assert_eq!(merkle_tree_root(&[a, b]), h(a, b));
    assert_eq!(merkle_tree_root(&[a, b, c]), h(h(a, b), c));
    assert_eq!(merkle_tree_root(&[a, b, c, d]), h(h(a, b), h(c, d)));
    assert_eq!(
        merkle_tree_root(&[a, b, c, d, e]),
        h(h(h(a, b), h(c, d)), e)
    );
    assert_eq!(
        merkle_tree_root(&[a, b, c, d, e, f]),
        h(h(h(a, b), h(c, d)), h(e, f))
    );
    assert_eq!(
        merkle_tree_root(&[a, b, c, d, e, f, g]),
        h(h(h(a, b), h(c, d)), h(h(e, f), g))
    );
}
