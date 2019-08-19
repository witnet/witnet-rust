/// The list of wallet ids stored in the database.
#[inline]
pub fn wallet_ids() -> &'static str {
    "wallets"
}

/// A wallet's name.
#[inline]
pub fn wallet_name() -> &'static str {
    "name"
}

/// A wallet's caption.
#[inline]
pub fn wallet_caption() -> &'static str {
    "caption"
}

/// A wallet's name.
#[inline]
pub fn wallet_id_name(id: &str) -> String {
    format!("{}name", id)
}

/// A wallet's caption.
#[inline]
pub fn wallet_id_caption(id: &str) -> String {
    format!("{}caption", id)
}

/// A wallet's encryption salt.
#[inline]
pub fn wallet_id_salt(wallet_id: &str) -> String {
    format!("{}salt", wallet_id)
}

/// A wallet's encryption iv.
#[inline]
pub fn wallet_id_iv(wallet_id: &str) -> String {
    format!("{}iv", wallet_id)
}

/// A wallet's generated account indexes.
#[inline]
pub fn wallet_accounts() -> &'static str {
    "accounts"
}

/// The default account that an unlocked wallet will use.
#[inline]
pub fn wallet_default_account() -> &'static str {
    "default-account"
}

/// An account's external key.
#[inline]
pub fn account_ek(account_index: u32) -> String {
    format!("account-{}-ek", account_index)
}

/// An account's internal key.
#[inline]
pub fn account_ik(account_index: u32) -> String {
    format!("account-{}-ik", account_index)
}

/// An account's rad key.
#[inline]
pub fn account_rk(account_index: u32) -> String {
    format!("account-{}-rk", account_index)
}

/// An account's next index to use for generating an external key.
#[inline]
pub fn account_next_ek_index(account_index: u32) -> String {
    format!("account-{}-next-ek-index", account_index)
}

/// A wallet's account address.
#[inline]
pub fn address(account_index: u32, key_index: u32) -> String {
    format!("account-{}-key-{}-address", account_index, key_index)
}

/// An address' path.
#[inline]
pub fn address_path(account_index: u32, key_index: u32) -> String {
    format!("account-{}-key-{}-address-path", account_index, key_index)
}

/// An address's label.
#[inline]
pub fn address_label(account_index: u32, key_index: u32) -> String {
    format!("account-{}-key-{}-address-label", account_index, key_index)
}

/// An custom key decided by the client to store something.
#[inline]
pub fn custom(key: &str) -> String {
    format!("custom-{}", key,)
}
