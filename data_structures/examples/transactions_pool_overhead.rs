use rand::{thread_rng, Rng};
use witnet_data_structures::chain::{
    DataRequestOutput, Hash, Input, KeyedSignature, OutputPointer, PublicKeyHash, RADAggregate,
    RADRequest, RADRetrieve, RADTally, RADType, Secp256k1Signature, Signature, TransactionsPool,
    ValueTransferOutput,
};
use witnet_data_structures::transaction::{
    DRTransaction, DRTransactionBody, Transaction, VTTransaction, VTTransactionBody,
};

fn random_request() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![
            RADRetrieve {
                kind: RADType::HttpGet,
                url: String::from("https://www.bitstamp.net/api/ticker/"),
                script: vec![130, 24, 119, 130, 24, 100, 100, 108, 97, 115, 116],
                body: vec![],
                headers: vec![],
            },
            RADRetrieve {
                kind: RADType::HttpGet,
                url: String::from("https://api.coindesk.com/v1/bpi/currentprice.json"),
                script: vec![
                    132, 24, 119, 130, 24, 102, 99, 98, 112, 105, 130, 24, 102, 99, 85, 83, 68,
                    130, 24, 100, 106, 114, 97, 116, 101, 95, 102, 108, 111, 97, 116,
                ],
                body: vec![],
                headers: vec![],
            },
        ],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: 3,
        },
        tally: RADTally {
            filters: vec![],
            reducer: 3,
        },
    }
}

fn random_dr_output() -> DataRequestOutput {
    let mut rng = thread_rng();

    DataRequestOutput {
        data_request: random_request(),
        witness_reward: rng.gen(),
        // The number of witnesses changes the RAM usage considerably
        // More witnesses = more weight = less transactions in pool = less RAM usage
        witnesses: 2,
        commit_and_reveal_fee: rng.gen(),
        min_consensus_percentage: rng.gen(),
        collateral: rng.gen(),
    }
}

fn random_transaction() -> (Transaction, u64) {
    let mut rng = thread_rng();

    let num_inputs = rng.gen_range(1, 3);
    let num_outputs = 2;

    let mut inputs = vec![];
    for _ in 0..num_inputs {
        let random_32_bytes: [u8; 32] = rng.gen();
        let transaction_id = Hash::from(random_32_bytes.to_vec());
        let output_pointer = OutputPointer {
            transaction_id,
            output_index: rng.gen(),
        };
        inputs.push(Input::new(output_pointer));
    }

    let mut outputs = vec![];
    for _ in 0..num_outputs {
        let random_20_bytes: [u8; 20] = rng.gen();
        let pkh = PublicKeyHash::from_bytes(&random_20_bytes).unwrap();
        outputs.push(ValueTransferOutput {
            pkh,
            value: 0,
            time_lock: 0,
        });
    }

    let signature = KeyedSignature {
        signature: Signature::Secp256k1(Secp256k1Signature {
            // DER encoded signature = 72 bytes
            der: vec![0xFF; 72],
        }),
        public_key: Default::default(),
    };

    let t = if rng.gen() {
        Transaction::ValueTransfer(VTTransaction {
            body: VTTransactionBody::new(inputs, outputs),
            signatures: vec![signature; num_inputs],
        })
    } else {
        let dr_output = random_dr_output();
        Transaction::DataRequest(DRTransaction {
            body: DRTransactionBody::new(inputs, outputs, dr_output),
            signatures: vec![signature; num_inputs],
        })
    };

    let fee = rng.gen();

    (t, fee)
}

fn main() {
    let mut pool = TransactionsPool::new();
    let testnet_weight_limit = 192_000_000;
    println!("Setting weight limit to {}", testnet_weight_limit);
    pool.set_total_weight_limit(testnet_weight_limit, 1.0);

    let mut limit_reached = false;

    for i in 0..1_000_000 {
        let (transaction, fee) = random_transaction();
        let removed_transactions = pool.insert(transaction, fee);

        if !limit_reached && !removed_transactions.is_empty() {
            println!("Limit reached after {} transactions", i);
            limit_reached = true;
        }
    }
}
