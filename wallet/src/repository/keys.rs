use crate::model;
use std::{fmt, marker::PhantomData};
use witnet_crypto::key::ExtendedSK;
use witnet_data_structures::{
    chain::{CheckpointBeacon, PublicKeyHash},
    transaction::Transaction,
};

/// Type-safe database key.
///
/// `K` is the type of the key itself, and `V` is the type of the value stored in the database.
/// For example, to store a `u32` using a `&'static str` key, use: `Key<&'static str, u32>`.
pub struct Key<K: AsRef<[u8]>, V: ?Sized>(K, PhantomData<V>);

impl<K: AsRef<[u8]>, V: ?Sized> Key<K, V> {
    /// Create new key
    pub fn new(key: K) -> Key<K, V> {
        Key(key, PhantomData)
    }
}

impl<V: ?Sized> Key<&'static str, V> {
    // FIXME: remove this method and make Self::new a const fn
    // This cannot be done because of the error:
    // trait bounds other than `Sized` on const fn parameters are unstable
    // https://github.com/rust-lang/rust/issues/57563
    pub const fn new_const(key: &'static str) -> Key<&'static str, V> {
        Key(key, PhantomData)
    }
}

// Manually implement traits instead of using derive because the derive automatically inserts a
// bound for V, which is not needed. For example, `Key<K, V>: Clone` does not need `V: Clone`
impl<K, V> Clone for Key<K, V>
where
    K: AsRef<[u8]> + Clone,
    V: ?Sized,
{
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }
}

impl<K, V> Copy for Key<K, V>
where
    K: AsRef<[u8]> + Copy,
    V: ?Sized,
{
}

impl<K, V> AsRef<[u8]> for Key<K, V>
where
    K: AsRef<[u8]>,
    V: ?Sized,
{
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<K, V> fmt::Debug for Key<K, V>
where
    K: AsRef<[u8]> + fmt::Debug,
    V: ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<K, V> Default for Key<K, V>
where
    K: AsRef<[u8]> + Default,
    V: ?Sized,
{
    fn default() -> Self {
        Self::new(K::default())
    }
}

impl<K, V> PartialEq<Key<K, V>> for Key<K, V>
where
    K: AsRef<[u8]> + PartialEq<K>,
    V: ?Sized,
{
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

/// The list of wallet ids stored in the database.
#[inline]
pub fn wallet_ids() -> Key<&'static str, Vec<String>> {
    Key::new("wallets")
}

/// A wallet's name.
#[inline]
pub fn wallet_name() -> Key<&'static str, String> {
    Key::new("name")
}

/// A wallet's description.
#[inline]
pub fn wallet_description() -> Key<&'static str, String> {
    Key::new("description")
}

/// A wallet's name.
#[inline]
pub fn wallet_id_name(id: &str) -> Key<String, String> {
    Key::new(format!("{}name", id))
}

/// A wallet's encryption salt.
#[inline]
pub fn wallet_id_salt(wallet_id: &str) -> Key<String, Vec<u8>> {
    Key::new(format!("{}salt", wallet_id))
}

/// A wallet's encryption iv.
#[inline]
pub fn wallet_id_iv(wallet_id: &str) -> Key<String, Vec<u8>> {
    Key::new(format!("{}iv", wallet_id))
}

/// A wallet's generated account indexes.
#[inline]
pub fn wallet_accounts() -> Key<&'static str, Vec<u32>> {
    Key::new("accounts")
}

/// The default account that an unlocked wallet will use.
#[inline]
pub fn wallet_default_account() -> Key<&'static str, u32> {
    Key::new("default-account")
}

/// The epoch of the latest block that was processed by the wallet
#[inline]
pub fn wallet_last_sync() -> Key<&'static str, CheckpointBeacon> {
    Key::new("last-sync")
}

/// An account's external key.
#[inline]
pub fn account_key(account_index: u32, keychain: u32) -> Key<String, ExtendedSK> {
    Key::new(format!("account-{}-{}-key", account_index, keychain))
}

/// An account's total balance.
#[inline]
pub fn account_balance(account_index: u32) -> Key<String, model::BalanceInfo> {
    Key::new(format!("account-{}-balance", account_index))
}

/// An account's UTXO set.
#[inline]
pub fn account_utxo_set(account_index: u32) -> Key<String, model::UtxoSet> {
    Key::new(format!("account-{}-utxo-set", account_index))
}

/// An account's next index to use for generating an address.
#[inline]
pub fn account_next_index(account_index: u32, keychain: u32) -> Key<String, u32> {
    Key::new(format!("account-{}-{}-next-index", account_index, keychain))
}

/// A wallet's account address.
#[inline]
pub fn address(account_index: u32, keychain: u32, key_index: u32) -> Key<String, String> {
    Key::new(format!(
        "account-{}-key-{}-{}-address",
        account_index, keychain, key_index
    ))
}

/// An address' path.
#[inline]
pub fn address_path(account_index: u32, keychain: u32, key_index: u32) -> Key<String, String> {
    Key::new(format!(
        "account-{}-key-{}-{}-address-path",
        account_index, keychain, key_index
    ))
}

/// An address' pkh.
#[inline]
pub fn address_pkh(
    account_index: u32,
    keychain: u32,
    key_index: u32,
) -> Key<String, PublicKeyHash> {
    Key::new(format!(
        "account-{}-key-{}-{}-address-pkh",
        account_index, keychain, key_index
    ))
}

/// An address additional information.
#[inline]
pub fn address_info(
    account_index: u32,
    keychain: u32,
    key_index: u32,
) -> Key<String, model::AddressInfo> {
    Key::new(format!(
        "account-{}-key-{}-{}-address-info",
        account_index, keychain, key_index
    ))
}

/// Master key
#[inline]
pub fn master_key() -> Key<&'static str, ExtendedSK> {
    Key::new("master-key")
}

/// Path information associated to a pkh (account, keychain and index).
#[inline]
pub fn pkh(pkh: &PublicKeyHash) -> Key<Vec<u8>, model::Path> {
    Key::new([b"pkh-", pkh.as_ref()].concat().to_vec())
}

/// An custom key decided by the client to store something.
#[inline]
pub fn custom(key: &str) -> Key<String, String> {
    Key::new(format!("custom-{}", key))
}

/// A created transaction pending to be sent or removed.
#[inline]
pub fn transaction(transaction_hash: &str) -> Key<String, Transaction> {
    Key::new(format!("transaction-{}", transaction_hash))
}

/// An index of transaction hashes.
#[inline]
pub fn transactions_index(transaction_hash: &[u8]) -> Key<Vec<u8>, u32> {
    Key::new([b"transactions-index-{}", transaction_hash].concat())
}

/// Next transaction id.
#[inline]
pub fn transaction_next_id(account_index: u32) -> Key<String, u32> {
    Key::new(format!("account-{}-transactions-next-id", account_index))
}

/// Transaction hash.
#[inline]
pub fn transaction_hash(account_index: u32, transaction_id: u32) -> Key<String, Vec<u8>> {
    Key::new(format!(
        "account-{}-transaction-{}-hash",
        account_index, transaction_id
    ))
}

/// Transaction movement.
#[inline]
pub fn transaction_movement(
    account_index: u32,
    transaction_id: u32,
) -> Key<String, model::BalanceMovement> {
    Key::new(format!(
        "account-{}-transaction-{}-movement",
        account_index, transaction_id
    ))
}
