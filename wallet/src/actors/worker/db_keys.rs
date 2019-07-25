macro_rules! bytes {
    ($($arg:tt)*) => {
        format!($($arg)*).as_bytes().to_vec()
    }
}

#[inline]
pub fn wallets_key() -> Vec<u8> {
    bytes!("wallets")
}

#[inline]
pub fn wallet_info_key(wallet_id: &str) -> Vec<u8> {
    bytes!("info-{}", wallet_id)
}

#[inline]
pub fn wallet_pkhs_key(wallet_id: &str) -> Vec<u8> {
    bytes!("pkhs-{}", wallet_id)
}

#[inline]
pub fn salt_key(wallet_id: &str) -> Vec<u8> {
    bytes!("salt-{}", wallet_id)
}

#[inline]
pub fn accounts_key(wallet_id: &str) -> Vec<u8> {
    bytes!("accounts-{}", wallet_id)
}
#[inline]
pub fn account_key(wallet_id: &str, account_index: u32) -> Vec<u8> {
    bytes!("account-{}-{}", wallet_id, account_index)
}

#[inline]
pub fn address_index_key(wallet_id: &str, account_index: u32) -> Vec<u8> {
    bytes!("address-index-{}-{}", wallet_id, account_index)
}

#[inline]
pub fn address_key(wallet_id: &str, account_index: u32, index: u32) -> Vec<u8> {
    bytes!("address-{}-{}-{}", wallet_id, account_index, index)
}
