use crate::types;

/// Result type for accounts-related operations that can fail.
pub type Result<T> = std::result::Result<T, failure::Error>;

/// Root KeyPath used for wallet accounts.
#[inline]
pub fn account_keypath(index: u32) -> types::KeyPath {
    types::KeyPath::default()
        .hardened(3)
        .hardened(4919)
        .hardened(index)
}

/// Generate a new account with the given index.
///
/// The account index is kind of the account id and indicates in which
/// branch the HD-Wallet derivation tree these account keys are.
pub fn gen_account(
    engine: &types::SignEngine,
    account_index: u32,
    master_key: &types::ExtendedSK,
) -> Result<types::Account> {
    let account_keypath = account_keypath(account_index);
    let account_key = master_key.derive(engine, &account_keypath)?;

    let external = {
        let keypath = types::KeyPath::default().index(0);

        account_key.derive(engine, &keypath)?
    };
    let internal = {
        let keypath = types::KeyPath::default().index(1);

        account_key.derive(engine, &keypath)?
    };
    let rad = {
        let keypath = types::KeyPath::default().index(2);

        account_key.derive(engine, &keypath)?
    };

    let account = types::Account {
        index: account_index,
        external,
        internal,
        rad,
    };

    Ok(account)
}
