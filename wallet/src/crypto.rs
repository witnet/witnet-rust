use crate::types;

pub fn gen_mnemonics(len: types::MnemonicLength) -> String {
    types::MnemonicGen::new()
        .with_len(len)
        .generate()
        .words()
        .to_string()
}
