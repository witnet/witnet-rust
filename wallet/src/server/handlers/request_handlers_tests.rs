use super::*;
use types::factories::Factory as _;

static WRONG_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon";

static PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

static XPRV: &str = "xprv1qpujxsyd4hfu0dtwa524vac84e09mjsgnh5h9crl8wrqg58z5wmsuqqcxlqmar3fjhkprndzkpnp2xlze76g4hu7g7c4r4r2m2e6y8xlvu566tn6";
static WRONG_XPRV: &str = "xprv1qxqqqqqpg0xyhjjecen2taujv52gzfvq9mfva3rd78zu4rn2qkx6k5j6w0csq0hs9lznqqr59zgleyz93w57mjpk8k837fn7xf43q7r3p37mxn095hysnx";

#[test]
fn test_create_wallet_with_unsupported_seed() {
    let state = types::State::factory();

    let request = requests::CreateWallet {
        name: "wallet".to_string(),
        caption: None,
        password: "12345678".into(),
        seed_source: "unknown source".to_string(),
        seed_data: "".into(),
    };

    match request.handle(&state).unwrap_err() {
        api::ApiError::Validation(err) => assert_eq!(
            validation::error(
                "seed_source",
                "Seed source has to be \'mnemonics\' or \'xprv\'."
            ),
            err
        ),
        err => assert!(false, format!("Expected invalid seed error: {}", err)),
    }
}

#[test]
fn test_create_wallet_with_invalid_password() {
    let state = types::State::factory();

    let request = requests::CreateWallet {
        name: "wallet".to_string(),
        caption: None,
        password: "123".into(),
        seed_source: "mnemonics".to_string(),
        seed_data: PHRASE.into(),
    };

    match request.handle(&state).unwrap_err() {
        api::ApiError::Validation(err) => assert_eq!(
            validation::error("password", "Password must have at least 8 characters"),
            err
        ),
        err => assert!(false, format!("Expected invalid password error: {}", err)),
    }
}

#[test]
fn test_create_wallet_with_wrong_mnemonic() {
    let state = types::State::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
    }

    let request = requests::CreateWallet {
        name: "wallet".to_string(),
        caption: None,
        password: "12345678".into(),
        seed_source: "mnemonics".to_string(),
        seed_data: WRONG_PHRASE.into(),
    };

    match request.handle(&state).unwrap_err() {
        api::ApiError::Validation(err) => assert_eq!(
            validation::error("seed_data", "invalid number of words in phrase: 8"),
            err
        ),
        err => assert!(
            false,
            format!("Expected seed data (mnemonic) error: {}", err)
        ),
    }
}

#[test]
fn test_create_wallet_with_mnemonic() {
    let state = types::State::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
    }

    let request = requests::CreateWallet {
        name: "wallet".to_string(),
        caption: None,
        password: "12345678".into(),
        seed_source: "mnemonics".to_string(),
        seed_data: PHRASE.into(),
    };

    let response = request.handle(&state).unwrap();

    assert_eq!(responses::WalletId { wallet_id: 1 }, response);
}

#[test]
fn test_create_wallet_with_wrong_xprv() {
    let state = types::State::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
    }

    let request = requests::CreateWallet {
        name: "wallet".to_string(),
        caption: None,
        password: "12345678".into(),
        seed_source: "xprv".to_string(),
        seed_data: WRONG_XPRV.into(),
    };

    match request.handle(&state).unwrap_err() {
        api::ApiError::Validation(err) => assert_eq!(
            validation::error(
                "seed_data",
                "Invalid seed data: Imported key is not a master key according to its path: m/1'"
            ),
            err
        ),
        err => assert!(
            false,
            format!("Expected invalid seed data (xprv) error: {}", err)
        ),
    }
}

#[test]
fn test_create_wallet_with_xprv() {
    let state = types::State::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
    }

    let request = requests::CreateWallet {
        name: "wallet".to_string(),
        caption: None,
        password: "12345678".into(),
        seed_source: "xprv".to_string(),
        seed_data: XPRV.into(),
    };

    let response = request.handle(&state).unwrap();

    assert_eq!(responses::WalletId { wallet_id: 1 }, response);
}

#[test]
fn test_unlock_wallet_that_not_exists() {
    let state = types::State::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
    }

    let request = requests::UnlockWallet {
        wallet_id: 1,
        password: Default::default(),
    };

    match request.handle(&state).unwrap_err() {
        api::ApiError::Validation(err) => {
            assert_eq!(validation::error("wallet_id", "Wallet not found"), err)
        }
        err => assert!(false, format!("Expected wallet not found error: {}", err)),
    }
}

#[test]
fn test_unlock_wallet_with_wrong_password() {
    let state = types::State::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
        wallets::create(&conn, "wallet", None).unwrap();

        let wallet_db_url = db::url(&state.db_path, "wallet");
        let password = "123";
        wallet::create(&wallet_db_url, password, &types::Account::factory()).unwrap();
    }

    let request = requests::UnlockWallet {
        wallet_id: 1,
        password: From::from("wrong password"),
    };

    match request.handle(&state).unwrap_err() {
        api::ApiError::Validation(err) => {
            assert_eq!(validation::error("password", "Invalid password"), err)
        }
        err => assert!(false, format!("Expected wallet not found error: {}", err)),
    }
}

#[test]
fn test_unlock_wallet() {
    let state = types::State::factory();
    let account = types::Account::factory();

    {
        let conn = state.db.get().unwrap();
        wallets::migrate_db(&conn).unwrap();
        wallets::create(&conn, "wallet", None).unwrap();

        let wallet_db_url = db::url(&state.db_path, "wallet");
        let password = "123";
        wallet::create(&wallet_db_url, password, &account).unwrap();
    }

    let request = requests::UnlockWallet {
        wallet_id: 1,
        password: "123".into(),
    };

    let response = request.handle(&state).unwrap();

    assert_eq!(
        vec![models::AccountInfo {
            index: account.index as i32,
            balance: 0
        }],
        response.accounts
    );
    assert_eq!(constants::DEFAULT_ACCOUNT_INDEX, response.default_account);
}
