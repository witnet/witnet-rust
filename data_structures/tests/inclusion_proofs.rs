use std::convert::TryFrom;
use witnet_crypto::{
    hash::Sha256,
    merkle::{sha256_concat, InclusionProof},
};
use witnet_data_structures::{chain::*, transaction::*};

fn h(left: Hash, right: Hash) -> Hash {
    let left = match left {
        Hash::SHA256(x) => Sha256(x),
    };
    let right = match right {
        Hash::SHA256(x) => Sha256(x),
    };
    sha256_concat(left, right).into()
}

fn example_block(txns: BlockTransactions) -> Block {
    let current_epoch = 1000;
    let last_block_hash = "62adde3e36db3f22774cc255215b2833575f66bf2204011f80c03d34c7c9ea41"
        .parse()
        .unwrap();

    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let block_header = BlockHeader {
        merkle_roots: BlockMerkleRoots::from_transactions(&txns),
        beacon: block_beacon,
        ..Default::default()
    };
    let block_sig = KeyedSignature::default();

    Block::new(block_header, block_sig, txns)
}

fn example_dr(id: usize) -> DRTransaction {
    let dr_output = DataRequestOutput {
        witness_reward: id as u64,
        ..Default::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dr_output);

    DRTransaction::new(dr_body, vec![])
}

fn example_ta(id: usize) -> TallyTransaction {
    let dr_pointer = Hash::with_first_u32(u32::try_from(id).unwrap());
    let tally = vec![u8::try_from(id).unwrap(); 32];
    TallyTransaction::new(dr_pointer, tally, vec![], vec![], vec![])
}

#[test]
fn dr_inclusion_0_drs() {
    let block = example_block(BlockTransactions {
        data_request_txns: vec![],
        ..Default::default()
    });

    let dr = example_dr(0);
    assert_eq!(dr.proof_of_inclusion(&block), None);
}

#[test]
fn dr_inclusion_1_drs() {
    let drx = example_dr(0);
    let dr0 = example_dr(1);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![dr0.clone()],
        ..Default::default()
    });

    assert_eq!(drx.proof_of_inclusion(&block), None);
    assert_eq!(
        dr0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![],
        })
    );
}

#[test]
fn dr_inclusion_2_drs() {
    let drx = example_dr(0);
    let dr0 = example_dr(1);
    let dr1 = example_dr(2);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![dr0.clone(), dr1.clone()],
        ..Default::default()
    });

    assert_eq!(drx.proof_of_inclusion(&block), None);
    assert_eq!(
        dr0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![dr1.hash()],
        })
    );
    assert_eq!(
        dr1.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![dr0.hash()],
        })
    );
}

#[test]
fn dr_inclusion_3_drs() {
    let drx = example_dr(0);
    let dr0 = example_dr(1);
    let dr1 = example_dr(2);
    let dr2 = example_dr(3);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![dr0.clone(), dr1.clone(), dr2.clone()],
        ..Default::default()
    });

    assert_eq!(drx.proof_of_inclusion(&block), None);
    assert_eq!(
        dr0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![dr1.hash(), dr2.hash()],
        })
    );
    assert_eq!(
        dr1.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![dr0.hash(), dr2.hash()],
        })
    );
    assert_eq!(
        dr2.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![h(dr0.hash(), dr1.hash())],
        })
    );
}

#[test]
fn dr_inclusion_5_drs() {
    let drx = example_dr(0);
    let dr0 = example_dr(1);
    let dr1 = example_dr(2);
    let dr2 = example_dr(3);
    let dr3 = example_dr(4);
    let dr4 = example_dr(5);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![
            dr0.clone(),
            dr1.clone(),
            dr2.clone(),
            dr3.clone(),
            dr4.clone(),
        ],
        ..Default::default()
    });

    assert_eq!(drx.proof_of_inclusion(&block), None);
    assert_eq!(
        dr0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![dr1.hash(), h(dr2.hash(), dr3.hash()), dr4.hash()],
        })
    );
    assert_eq!(
        dr1.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![dr0.hash(), h(dr2.hash(), dr3.hash()), dr4.hash()],
        })
    );
    assert_eq!(
        dr2.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 2,
            lemma: vec![dr3.hash(), h(dr0.hash(), dr1.hash()), dr4.hash()],
        })
    );
    assert_eq!(
        dr3.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 3,
            lemma: vec![dr2.hash(), h(dr0.hash(), dr1.hash()), dr4.hash()],
        })
    );
    assert_eq!(
        dr4.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![h(h(dr0.hash(), dr1.hash()), h(dr2.hash(), dr3.hash()))],
        })
    );
}

#[test]
fn ta_inclusion_0_tas() {
    let block = example_block(BlockTransactions {
        tally_txns: vec![],
        ..Default::default()
    });

    let ta = example_ta(0);
    assert_eq!(ta.proof_of_inclusion(&block), None);
}

#[test]
fn ta_inclusion_1_tas() {
    let tax = example_ta(0);
    let ta0 = example_ta(1);

    let block = example_block(BlockTransactions {
        tally_txns: vec![ta0.clone()],
        ..Default::default()
    });

    assert_eq!(tax.proof_of_inclusion(&block), None);
    assert_eq!(
        ta0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![],
        })
    );
}

#[test]
fn ta_inclusion_2_tas() {
    let tax = example_ta(0);
    let ta0 = example_ta(1);
    let ta1 = example_ta(2);

    let block = example_block(BlockTransactions {
        tally_txns: vec![ta0.clone(), ta1.clone()],
        ..Default::default()
    });

    assert_eq!(tax.proof_of_inclusion(&block), None);
    assert_eq!(
        ta0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![ta1.hash()],
        })
    );
    assert_eq!(
        ta1.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![ta0.hash()],
        })
    );
}

#[test]
fn ta_inclusion_3_tas() {
    let tax = example_ta(0);
    let ta0 = example_ta(1);
    let ta1 = example_ta(2);
    let ta2 = example_ta(3);

    let block = example_block(BlockTransactions {
        tally_txns: vec![ta0.clone(), ta1.clone(), ta2.clone()],
        ..Default::default()
    });

    assert_eq!(tax.proof_of_inclusion(&block), None);
    assert_eq!(
        ta0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![ta1.hash(), ta2.hash()],
        })
    );
    assert_eq!(
        ta1.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![ta0.hash(), ta2.hash()],
        })
    );
    assert_eq!(
        ta2.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![h(ta0.hash(), ta1.hash())],
        })
    );
}

#[test]
fn ta_inclusion_5_tas() {
    let tax = example_ta(0);
    let ta0 = example_ta(1);
    let ta1 = example_ta(2);
    let ta2 = example_ta(3);
    let ta3 = example_ta(4);
    let ta4 = example_ta(5);

    let block = example_block(BlockTransactions {
        tally_txns: vec![
            ta0.clone(),
            ta1.clone(),
            ta2.clone(),
            ta3.clone(),
            ta4.clone(),
        ],
        ..Default::default()
    });

    assert_eq!(tax.proof_of_inclusion(&block), None);
    assert_eq!(
        ta0.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 0,
            lemma: vec![ta1.hash(), h(ta2.hash(), ta3.hash()), ta4.hash()],
        })
    );
    assert_eq!(
        ta1.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![ta0.hash(), h(ta2.hash(), ta3.hash()), ta4.hash()],
        })
    );
    assert_eq!(
        ta2.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 2,
            lemma: vec![ta3.hash(), h(ta0.hash(), ta1.hash()), ta4.hash()],
        })
    );
    assert_eq!(
        ta3.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 3,
            lemma: vec![ta2.hash(), h(ta0.hash(), ta1.hash()), ta4.hash()],
        })
    );
    assert_eq!(
        ta4.proof_of_inclusion(&block),
        Some(TxInclusionProof {
            index: 1,
            lemma: vec![h(h(ta0.hash(), ta1.hash()), h(ta2.hash(), ta3.hash()))],
        })
    );
}

fn check_dr_data_proof_inclusion(dr: DRTransaction, block: &Block) {
    let mt_root = block.block_header.merkle_roots.dr_hash_merkle_root.into();

    let old_poi = dr.proof_of_inclusion(block).unwrap();
    let data_hash = dr.body.data_poi_hash();
    let new_index = old_poi.index << 1;
    let mut new_lemma = old_poi.lemma;
    new_lemma.insert(0, dr.body.rest_poi_hash());

    let poi = dr.data_proof_of_inclusion(block);
    assert_eq!(
        poi,
        Some(TxInclusionProof {
            index: new_index,
            lemma: new_lemma,
        })
    );
    let poi = poi.unwrap();

    let lemma = poi
        .lemma
        .iter()
        .map(|h| match *h {
            Hash::SHA256(x) => Sha256(x),
        })
        .collect();

    let proof = InclusionProof::sha256(poi.index, lemma);
    assert!(proof.verify(data_hash.into(), mt_root));
}

#[test]
fn dr_inclusion_1_drs_plus_leaves() {
    let dr0 = example_dr(1);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![dr0.clone()],
        ..Default::default()
    });

    check_dr_data_proof_inclusion(dr0, &block);
}

#[test]
fn dr_inclusion_2_drs_plus_leaves() {
    let dr0 = example_dr(1);
    let dr1 = example_dr(2);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![dr0.clone(), dr1.clone()],
        ..Default::default()
    });

    check_dr_data_proof_inclusion(dr0, &block);
    check_dr_data_proof_inclusion(dr1, &block);
}

#[test]
fn dr_inclusion_3_drs_plus_leaves() {
    let dr0 = example_dr(1);
    let dr1 = example_dr(2);
    let dr2 = example_dr(3);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![dr0.clone(), dr1.clone(), dr2.clone()],
        ..Default::default()
    });

    check_dr_data_proof_inclusion(dr0, &block);
    check_dr_data_proof_inclusion(dr1, &block);
    check_dr_data_proof_inclusion(dr2, &block);
}

#[test]
fn dr_inclusion_5_drs_plus_leaves() {
    let dr0 = example_dr(1);
    let dr1 = example_dr(2);
    let dr2 = example_dr(3);
    let dr3 = example_dr(4);
    let dr4 = example_dr(5);

    let block = example_block(BlockTransactions {
        data_request_txns: vec![
            dr0.clone(),
            dr1.clone(),
            dr2.clone(),
            dr3.clone(),
            dr4.clone(),
        ],
        ..Default::default()
    });

    check_dr_data_proof_inclusion(dr0, &block);
    check_dr_data_proof_inclusion(dr1, &block);
    check_dr_data_proof_inclusion(dr2, &block);
    check_dr_data_proof_inclusion(dr3, &block);
    check_dr_data_proof_inclusion(dr4, &block);
}

fn check_ta_data_proof_inclusion(ta: TallyTransaction, block: &Block) {
    let mt_root = block
        .block_header
        .merkle_roots
        .tally_hash_merkle_root
        .into();

    let old_poi = ta.proof_of_inclusion(block).unwrap();
    let data_hash = ta.data_poi_hash();
    let new_index = old_poi.index << 1;
    let mut new_lemma = old_poi.lemma;
    new_lemma.insert(0, ta.rest_poi_hash());

    let poi = ta.data_proof_of_inclusion(block);
    assert_eq!(
        poi,
        Some(TxInclusionProof {
            index: new_index,
            lemma: new_lemma,
        })
    );
    let poi = poi.unwrap();

    let lemma = poi
        .lemma
        .iter()
        .map(|h| match *h {
            Hash::SHA256(x) => Sha256(x),
        })
        .collect();

    let proof = InclusionProof::sha256(poi.index, lemma);
    assert!(proof.verify(data_hash.into(), mt_root));
}

#[test]
fn ta_inclusion_1_tas_plus_leaves() {
    let ta0 = example_ta(1);

    let block = example_block(BlockTransactions {
        tally_txns: vec![ta0.clone()],
        ..Default::default()
    });

    check_ta_data_proof_inclusion(ta0, &block);
}

#[test]
fn ta_inclusion_2_tas_plus_leaves() {
    let ta0 = example_ta(1);
    let ta1 = example_ta(2);

    let block = example_block(BlockTransactions {
        tally_txns: vec![ta0.clone(), ta1.clone()],
        ..Default::default()
    });

    check_ta_data_proof_inclusion(ta0, &block);
    check_ta_data_proof_inclusion(ta1, &block);
}

#[test]
fn ta_inclusion_3_tas_plus_leaves() {
    let ta0 = example_ta(1);
    let ta1 = example_ta(2);
    let ta2 = example_ta(3);

    let block = example_block(BlockTransactions {
        tally_txns: vec![ta0.clone(), ta1.clone(), ta2.clone()],
        ..Default::default()
    });

    check_ta_data_proof_inclusion(ta0, &block);
    check_ta_data_proof_inclusion(ta1, &block);
    check_ta_data_proof_inclusion(ta2, &block);
}

#[test]
fn ta_inclusion_5_tas_plus_leaves() {
    let ta0 = example_ta(1);
    let ta1 = example_ta(2);
    let ta2 = example_ta(3);
    let ta3 = example_ta(4);
    let ta4 = example_ta(5);

    let block = example_block(BlockTransactions {
        tally_txns: vec![
            ta0.clone(),
            ta1.clone(),
            ta2.clone(),
            ta3.clone(),
            ta4.clone(),
        ],
        ..Default::default()
    });

    check_ta_data_proof_inclusion(ta0, &block);
    check_ta_data_proof_inclusion(ta1, &block);
    check_ta_data_proof_inclusion(ta2, &block);
    check_ta_data_proof_inclusion(ta3, &block);
    check_ta_data_proof_inclusion(ta4, &block);
}
