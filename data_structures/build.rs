//! This crate simplifies writing build.rs for exonum and exonum services.
extern crate exonum_build;

use std::env;

fn create_path_to_protobuf_schema_env() {
    // Workaround for https://github.com/rust-lang/cargo/issues/3544
    // We "link" exonum with exonum_protobuf library
    // and dependents in their `build.rs` will have access to `$DEP_EXONUM_PROTOBUF_PROTOS`.
    let path = env::current_dir()
        .expect("Failed to get current dir.")
        .join("../schemas/witnet");
    println!("cargo:protos={}", path.to_str().unwrap());
}

fn main() {
    create_path_to_protobuf_schema_env();

    exonum_build::protobuf_generate(
        "../schemas/witnet",
        &["../schemas/witnet"],
        "witnet_proto_mod.rs",
    );
}
