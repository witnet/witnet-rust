macro_rules! bytes {
    ($($arg:tt)*) => {
        format!($($arg)*).as_bytes().to_vec()
    }
}

#[inline]
pub fn wallet_infos() -> Vec<u8> {
    bytes!("wallet_infos")
}
