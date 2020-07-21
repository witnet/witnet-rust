use witnet_crypto::hash::{calculate_sha256, Sha256};
use witnet_crypto::merkle::{
    merkle_tree_root, FullMerkleTree, InclusionProof, ProgressiveMerkleTree,
};

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
    let hash_a = Sha256([0x00; 32]);
    let hash_b = Sha256([0x11; 32]);
    let hash_c = Sha256([0x22; 32]);
    let hash_d = Sha256([0x33; 32]);
    let hash_e = Sha256([0x44; 32]);
    let hash_f = Sha256([0x55; 32]);
    let hash_g = Sha256([0x66; 32]);

    let hc = hash_concat;
    // Verify the expected hash by manually hashing the elements in order
    assert_eq!(merkle_tree_root(&[hash_a, hash_b]), hc(hash_a, hash_b));
    assert_eq!(
        merkle_tree_root(&[hash_a, hash_b, hash_c]),
        hc(hc(hash_a, hash_b), hash_c)
    );
    assert_eq!(
        merkle_tree_root(&[hash_a, hash_b, hash_c, hash_d]),
        hc(hc(hash_a, hash_b), hc(hash_c, hash_d))
    );
    assert_eq!(
        merkle_tree_root(&[hash_a, hash_b, hash_c, hash_d, hash_e]),
        hc(hc(hc(hash_a, hash_b), hc(hash_c, hash_d)), hash_e)
    );
    assert_eq!(
        merkle_tree_root(&[hash_a, hash_b, hash_c, hash_d, hash_e, hash_f]),
        hc(
            hc(hc(hash_a, hash_b), hc(hash_c, hash_d)),
            hc(hash_e, hash_f)
        )
    );
    assert_eq!(
        merkle_tree_root(&[hash_a, hash_b, hash_c, hash_d, hash_e, hash_f, hash_g]),
        hc(
            hc(hc(hash_a, hash_b), hc(hash_c, hash_d)),
            hc(hc(hash_e, hash_f), hash_g)
        )
    );
}

#[test]
fn progressive() {
    // Compare the ProgressiveMerkleTree against the slice-based one
    let hash_a = Sha256([0x00; 32]);
    let hash_b = Sha256([0x11; 32]);
    let hash_c = Sha256([0x22; 32]);
    let hash_d = Sha256([0x33; 32]);
    let hash_e = Sha256([0x44; 32]);
    let hash_f = Sha256([0x55; 32]);
    let hash_g = Sha256([0x66; 32]);
    let hashes = vec![hash_a, hash_b, hash_c, hash_d, hash_e, hash_f, hash_g];

    let mut mt = ProgressiveMerkleTree::sha256();
    // Empty merkle tree: empty hash
    assert_eq!(merkle_tree_root(&[]), mt.root());
    let mut mhashes = vec![];

    for hash in hashes {
        mt.push(hash);
        mhashes.push(hash);
        assert_eq!(merkle_tree_root(&mhashes), mt.root());
    }
}

#[test]
fn full_merkle_tree() {
    // Compare the FullMerkleTree against the slice-based one
    let hash_a = Sha256([0x00; 32]);
    let hash_b = Sha256([0x11; 32]);
    let hash_c = Sha256([0x22; 32]);
    let hash_d = Sha256([0x33; 32]);
    let hash_e = Sha256([0x44; 32]);
    let hash_f = Sha256([0x55; 32]);
    let hadh_g = Sha256([0x66; 32]);
    let hashes = vec![hash_a, hash_b, hash_c, hash_d, hash_e, hash_f, hadh_g];

    // Empty merkle tree: empty hash
    assert_eq!(merkle_tree_root(&[]), FullMerkleTree::sha256(vec![]).root());

    for i in 0..hashes.len() {
        let mhashes = &hashes[..i];
        assert_eq!(
            merkle_tree_root(mhashes),
            FullMerkleTree::sha256(mhashes.to_vec()).root()
        );
    }
}

#[test]
fn inclusion_proofs() {
    let leaves = vec![
        Sha256([0; 32]),
        Sha256([1; 32]),
        Sha256([2; 32]),
        Sha256([3; 32]),
        Sha256([4; 32]),
        Sha256([5; 32]),
        Sha256([6; 32]),
        Sha256([7; 32]),
        Sha256([8; 32]),
        Sha256([9; 32]),
    ];
    for j in 0..10 {
        let leaves = &leaves[..j];
        let mt = FullMerkleTree::sha256(leaves.to_vec());
        for (idx, lidx) in leaves.iter().enumerate() {
            let p0 = mt.inclusion_proof(idx).unwrap();
            assert!(p0.verify(*lidx, mt.root()), "{:#?}", (j, idx, p0, mt));
            // Verifying with a different hash or root fails
            assert!(
                !p0.verify(Sha256([0xFF; 32]), mt.root()),
                "{:#?}",
                (j, idx, p0, mt)
            );
            assert!(
                !p0.verify(*lidx, Sha256([0xFF; 32])),
                "{:#?}",
                (j, idx, p0, mt)
            );
        }
    }
}

#[test]
fn manual_inclusion_proof() {
    let h = hash_concat;
    // Manually build an inclusion proof
    // The merkle tree is pretty simple: the element at index x has hash [x; 32]
    let leaves = vec![
        Sha256([0; 32]),
        Sha256([1; 32]),
        Sha256([2; 32]),
        Sha256([3; 32]),
        Sha256([4; 32]),
        Sha256([5; 32]),
        Sha256([6; 32]),
        Sha256([7; 32]),
        Sha256([8; 32]),
        Sha256([9; 32]),
    ];
    let mt = FullMerkleTree::sha256(leaves);
    let mt_root = mt.root();
    // We will prove that the element at index 0 is [0; 32]
    let lemma = vec![
        // R 1
        Sha256([1; 32]),
        // R 23
        h(Sha256([2; 32]), Sha256([3; 32])),
        // R 4567
        h(
            h(Sha256([4; 32]), Sha256([5; 32])),
            h(Sha256([6; 32]), Sha256([7; 32])),
        ),
        // R 89
        h(Sha256([8; 32]), Sha256([9; 32])),
    ];
    let proof_index = 0;

    let proof = InclusionProof::sha256(proof_index, lemma);
    assert!(proof.verify(Sha256([0; 32]), mt_root));

    // Now let's prove element 7
    let lemma = vec![
        // L 6
        Sha256([6; 32]),
        // L 45
        h(Sha256([4; 32]), Sha256([5; 32])),
        // L 0123
        h(
            h(Sha256([0; 32]), Sha256([1; 32])),
            h(Sha256([2; 32]), Sha256([3; 32])),
        ),
        // R 89
        h(Sha256([8; 32]), Sha256([9; 32])),
    ];
    let proof_index = 7;

    let proof = InclusionProof::sha256(proof_index, lemma);
    assert!(proof.verify(Sha256([7; 32]), mt_root));

    // Now let's prove element 9
    // Get the hash from the full merkle tree, to avoid having to manually
    // calculate h(h(h(0,1),h(2,3)),h(h(4,5),h(6,7)))
    let h07 = mt.nodes()[3][0];

    // Note how this proof will be shorter than the others, because the merkle
    // tree is not balanced
    let lemma = vec![
        // L 8
        Sha256([8; 32]),
        // L 01234567
        h07,
    ];
    // But now the proof index is not 9 but 3, because we concatenate two times
    // on the left, so the 2 least significant bits must be set
    let proof_index = 3;
    let proof = InclusionProof::sha256(proof_index, lemma);
    assert!(proof.verify(Sha256([9; 32]), mt_root));
}
