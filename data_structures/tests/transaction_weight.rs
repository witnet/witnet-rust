use witnet_data_structures::chain::{transaction_example, RADRequest};
use witnet_data_structures::proto::ProtobufConvert;
use witnet_data_structures::transaction::Transaction;

#[test]
fn rad_retrieve_header_overhead() {
    fn mutate_example_transaction<F>(f: F) -> Transaction
    where
        F: FnOnce(&mut RADRequest),
    {
        let mut t = transaction_example();
        match &mut t {
            Transaction::DataRequest(dr_transaction) => {
                let rad_request = &mut dr_transaction.body.dr_output.data_request;
                f(rad_request);
            }
            _ => unreachable!("transaction_example must return a data request transaction"),
        }

        t
    }

    let msg_no_headers = mutate_example_transaction(|_| {});

    let msg_one_empty_header = mutate_example_transaction(|rad_request| {
        let new_header = ("".to_string(), "".to_string());
        rad_request.retrieve[0].headers.push(new_header);
    });

    let msg_one_header = mutate_example_transaction(|rad_request| {
        let new_header = ("12345".to_string(), "1234567890".to_string());
        rad_request.retrieve[0].headers.push(new_header);
    });

    let msg_one_header_2_bytes = mutate_example_transaction(|rad_request| {
        let new_header = ("1".to_string(), "1".to_string());
        rad_request.retrieve[0].headers.push(new_header);
    });

    let msg_two_headers_2_bytes = mutate_example_transaction(|rad_request| {
        let new_header = ("1".to_string(), "1".to_string());
        rad_request.retrieve[0].headers.push(new_header.clone());
        rad_request.retrieve[0].headers.push(new_header);
    });

    let msg_256_headers_2_bytes = mutate_example_transaction(|rad_request| {
        let new_header = ("1".to_string(), "1".to_string());
        for _ in 0..256 {
            rad_request.retrieve[0].headers.push(new_header.clone());
        }
    });

    // Approximation of the protobuf serialization overhead of `repeated StringPair`.
    let overhead: u8 = 6;
    // Size and weight of example transaction with no headers.
    // If the example transaction changes, this values may need to be changed as well.
    let base_size = 323;
    let base_weight = 503;

    // Test message with zero headers
    assert_eq!(msg_no_headers.to_pb_bytes().unwrap().len(), base_size);
    assert_eq!(msg_no_headers.weight(), base_weight);
    // Test message with one empty header ("", "")
    // overhead - 4 because protobuf optimizes default values by not serializing them
    assert_eq!(
        msg_one_empty_header.to_pb_bytes().unwrap().len(),
        base_size + overhead as usize - 4
    );
    assert_eq!(
        msg_one_empty_header.weight(),
        base_weight + u32::from(overhead)
    );
    // Test message with 5 bytes header name and 10 bytes header value
    assert_eq!(
        msg_one_header.to_pb_bytes().unwrap().len(),
        base_size + 5 + 10 + overhead as usize
    );
    assert_eq!(
        msg_one_header.weight(),
        base_weight + 5 + 10 + u32::from(overhead)
    );
    // Test message with 1 bytes header name and 1 bytes header value
    assert_eq!(
        msg_one_header_2_bytes.to_pb_bytes().unwrap().len(),
        base_size + 1 + 1 + overhead as usize
    );
    assert_eq!(
        msg_one_header_2_bytes.weight(),
        base_weight + 1 + 1 + u32::from(overhead)
    );
    // Test message with two headers consisting of 1 bytes header name and 1 bytes header value
    assert_eq!(
        msg_two_headers_2_bytes.to_pb_bytes().unwrap().len(),
        base_size + (1 + 1 + overhead as usize) * 2
    );
    assert_eq!(
        msg_two_headers_2_bytes.weight(),
        base_weight + (1 + 1 + u32::from(overhead)) * 2
    );
    // Test message with 256 headers consisting of 1 bytes header name and 1 bytes header value.
    // Not sure why +1, probably because the `RADRequest` message needs two bytes to serialize its
    // "number of fields" or something similar.
    assert_eq!(
        msg_256_headers_2_bytes.to_pb_bytes().unwrap().len(),
        base_size + (1 + 1 + overhead as usize) * 256 + 1
    );
    assert_eq!(
        msg_256_headers_2_bytes.weight(),
        base_weight + (1 + 1 + u32::from(overhead)) * 256
    );
}
