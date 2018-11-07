use witnet_data_structures::builders;
use witnet_data_structures::types;

#[test]
fn builders_build_get_peers() {
    let msg = types::Message {
        kind: types::Command::GetPeers,
        magic: builders::MAGIC,
    };

    assert_eq!(msg, builders::build_get_peers());
}
