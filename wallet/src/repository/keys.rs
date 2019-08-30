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
pub fn account_key(account_index: u32, keychain: u32) -> String {
    format!("account-{}-{}-key", account_index, keychain)
}

/// An account's total balance.
#[inline]
pub fn account_balance(account_index: u32) -> String {
    format!("account-{}-balance", account_index)
}

/// An account's UTXO set.
#[inline]
pub fn account_utxo_set(account_index: u32) -> String {
    format!("account-{}-utxo-set", account_index)
}

/// An account's next index to use for generating an address.
#[inline]
pub fn account_next_index(account_index: u32, keychain: u32) -> String {
    format!("account-{}-{}-next-index", account_index, keychain)
}

/// A wallet's account address.
#[inline]
pub fn address(account_index: u32, keychain: u32, key_index: u32) -> String {
    format!(
        "account-{}-key-{}-{}-address",
        account_index, keychain, key_index
    )
}

/// An address' path.
#[inline]
pub fn address_path(account_index: u32, keychain: u32, key_index: u32) -> String {
    format!(
        "account-{}-key-{}-{}-address-path",
        account_index, keychain, key_index
    )
}

/// An address' pkh.
#[inline]
pub fn address_pkh(account_index: u32, keychain: u32, key_index: u32) -> String {
    format!(
        "account-{}-key-{}-{}-address-pkh",
        account_index, keychain, key_index
    )
}

/// An address's label.
#[inline]
pub fn address_label(account_index: u32, keychain: u32, key_index: u32) -> String {
    format!(
        "account-{}-key-{}-{}-address-label",
        account_index, keychain, key_index
    )
}

/// Info associated to a pkh.
#[inline]
pub fn pkh(pkh: &[u8]) -> Vec<u8> {
    [pkh, b"-pkh"].concat().to_vec()
}

/// An custom key decided by the client to store something.
#[inline]
pub fn custom(key: &str) -> String {
    format!("custom-{}", key,)
}

/// A pending VTT transaction.
#[inline]
pub fn vtt(transaction_hash: &str) -> String {
    format!("vtt-{}", transaction_hash)
}

/// Next transaction id.
#[inline]
pub fn transaction_next_id(account_index: u32) -> String {
    format!("account-{}-transactions-next-id", account_index)
}

/// A transaction's value.
#[inline]
pub fn transaction_value(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-value",
        account_index, transaction_id
    )
}

/// A transaction's fee.
#[inline]
pub fn transaction_fee(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-fee",
        account_index, transaction_id
    )
}

/// A transaction's label.
#[inline]
pub fn transaction_label(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-label",
        account_index, transaction_id
    )
}

/// If a transaction is debit or credit.
#[inline]
pub fn transaction_type(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-type",
        account_index, transaction_id
    )
}

/// The block where a transaction is.
#[inline]
pub fn transaction_block(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-block",
        account_index, transaction_id
    )
}

/// Transaction timestamp.
#[inline]
pub fn transaction_timestamp(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-timestamp",
        account_index, transaction_id
    )
}

/// Transaction hash.
#[inline]
pub fn transaction_hash(account_index: u32, transaction_id: u32) -> String {
    format!(
        "account-{}-transaction-{}-hash",
        account_index, transaction_id
    )
}
