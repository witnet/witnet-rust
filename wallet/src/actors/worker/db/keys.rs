macro_rules! bytes {
    ($($arg:tt)*) => {
        format!($($arg)*).as_bytes().to_vec()
    }
}

/// The list of wallet ids stored in the database.
#[inline]
pub fn wallet_ids() -> Vec<u8> {
    bytes!("wallets")
}

/// A wallet's name.
#[inline]
pub fn wallet_name(wallet_id: &str) -> Vec<u8> {
    bytes!("wallet-{}-name", wallet_id)
}

/// A wallet's caption.
#[inline]
pub fn wallet_caption(wallet_id: &str) -> Vec<u8> {
    bytes!("wallet-{}-caption", wallet_id)
}

/// A wallet's encryption salt.
#[inline]
pub fn wallet_salt(wallet_id: &str) -> Vec<u8> {
    bytes!("wallet-{}-salt", wallet_id)
}

/// A wallet's generated account indexes.
#[inline]
pub fn wallet_accounts(wallet_id: &str) -> Vec<u8> {
    bytes!("wallet-{}-accounts", wallet_id)
}

/// The default account that an unlocked wallet will use.
#[inline]
pub fn wallet_default_account(wallet_id: &str) -> Vec<u8> {
    bytes!("wallet-{}-default-account", wallet_id)
}

/// A wallet's pkhs.
#[inline]
pub fn wallet_pkhs(wallet_id: &str) -> Vec<u8> {
    bytes!("wallet-{}-pkhs", wallet_id)
}

/// An account's external key.
#[inline]
pub fn account_ek(wallet_id: &str, account_index: u32) -> Vec<u8> {
    bytes!("wallet-{}-account-{}-ek", wallet_id, account_index)
}

/// An account's internal key.
#[inline]
pub fn account_ik(wallet_id: &str, account_index: u32) -> Vec<u8> {
    bytes!("wallet-{}-account-{}-ik", wallet_id, account_index)
}

/// An account's rad key.
#[inline]
pub fn account_rk(wallet_id: &str, account_index: u32) -> Vec<u8> {
    bytes!("wallet-{}-account-{}-rk", wallet_id, account_index)
}

/// An account's next index to use for generating an external key.
#[inline]
pub fn account_next_ek_index(wallet_id: &str, account_index: u32) -> Vec<u8> {
    bytes!(
        "wallet-{}-account-{}-next-ek-index",
        wallet_id,
        account_index
    )
}

/// A wallet's account address.
#[inline]
pub fn address(wallet_id: &str, account_index: u32, key_index: u32) -> Vec<u8> {
    bytes!(
        "wallet-{}-account-{}-key-{}-address",
        wallet_id,
        account_index,
        key_index
    )
}

/// An address' path.
#[inline]
pub fn address_path(wallet_id: &str, account_index: u32, key_index: u32) -> Vec<u8> {
    bytes!(
        "wallet-{}-account-{}-key-{}-address-path",
        wallet_id,
        account_index,
        key_index
    )
}

/// An address's label.
#[inline]
pub fn address_label(wallet_id: &str, account_index: u32, key_index: u32) -> Vec<u8> {
    bytes!(
        "wallet-{}-account-{}-key-{}-address-label",
        wallet_id,
        account_index,
        key_index
    )
}

/// An custom key decided by the client to store something.
#[inline]
pub fn custom(wallet_id: &str, key: &str) -> Vec<u8> {
    bytes!("wallet-{}-custom-{}", wallet_id, key,)
}
