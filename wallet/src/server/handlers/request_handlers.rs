use super::*;

impl Handler for requests::GetWalletInfos {
    type Result = api::Result<responses::WalletInfos>;

    fn handle(self, state: &types::State) -> Self::Result {
        let db = state.db.get()?;
        let infos = wallets::list(&db)?;
        let response = responses::WalletInfos { infos };

        Ok(response)
    }
}

impl Handler for requests::CreateMnemonics {
    type Result = api::Result<responses::Mnemonics>;

    fn handle(self, _state: &types::State) -> Self::Result {
        let len = self.validate()?;
        let mnemonics = crypto::gen_mnemonics(len);
        let response = responses::Mnemonics { mnemonics };

        Ok(response)
    }
}

impl Handler for requests::RunRadRequest {
    type Result = api::Result<responses::RadRequestResult>;

    fn handle(self, _state: &types::State) -> Self::Result {
        let response = radon::run_request(&self.rad_request)
            .map(responses::RadRequestResult::Value)
            .unwrap_or_else(|e| responses::RadRequestResult::Error(format!("{}", e)));

        Ok(response)
    }
}

impl Handler for requests::CreateWallet {
    type Result = api::Result<responses::WalletId>;

    fn handle(self, state: &types::State) -> Self::Result {
        let params = self.validate(&state.db_path)?;

        // Wallet master-key generation.
        let seed_password = state.wallets_config.seed_password.as_ref();
        let key_salt = state.wallets_config.master_key_salt.as_ref();
        let key_source = &params.seed_source;
        let master_key =
            crypto::gen_master_key(seed_password, key_salt, key_source).map_err(|err| {
                log::warn!("Failed to generate master key: {}", err);

                let err = validation::error("seed_data", format!("Invalid seed data: {}", err));
                api::ApiError::Validation(err)
            })?;

        // Wallet default account keys.
        let account_index = constants::DEFAULT_ACCOUNT_INDEX;
        let account = account::gen_account(&state.sign_engine, account_index, &master_key)?;

        wallet::create(&params.db_url, params.password.as_ref(), &account)?;

        let wallet_id = wallets::create(&state.db.get()?, &params.name, params.caption.as_ref())?;

        let response = responses::WalletId { wallet_id };

        Ok(response)
    }
}

impl Handler for requests::UnlockWallet {
    type Result = api::Result<responses::UnlockedWallet>;

    fn handle(self, state: &types::State) -> Self::Result {
        let wallet_id = self.wallet_id;
        let session_expiration = state.wallets_config.session_expires_in;

        // connect and unlock the database
        let info = wallets::find(&state.db.get()?, wallet_id)?.ok_or_else(|| {
            let err = validation::error("wallet_id", "Wallet not found");
            api::ApiError::Validation(err)
        })?;
        let db_url = db::url(&state.db_path, &info.name);
        let wallet_db =
            wallet::unlock_db(&db_url, self.password.as_ref()).map_err(|err| match err {
                error::Error::DbPassword => {
                    let err = validation::error("password", "Invalid password");
                    api::ApiError::Validation(err)
                }
                err => api::error::internal(err),
            })?;

        // session id generation
        let iterations = constants::ID_HASH_ITERATIONS;
        let rng = &mut *state.rng.lock()?;
        let salt = crypto::salt(rng, constants::ID_SALT_LENGTH);
        let key = crypto::key_from_password(self.password.as_ref(), &salt, iterations);
        let session_id = types::SessionId::from(crypto::gen_session_id(
            rng,
            constants::ID_HASH_FUNC,
            &key,
            &salt,
            iterations,
        ));

        // build response.
        let conn = wallet_db.get()?;
        let accounts = wallet::accounts(&conn)?;
        let response = responses::UnlockedWallet {
            accounts,
            session_id: session_id.clone(),
            default_account: constants::DEFAULT_ACCOUNT_INDEX,
            session_expiration_secs: session_expiration.as_secs(),
        };

        // create session for the unlocked wallet.
        let mut session = types::Session::new(session_expiration);
        session.wallets.insert(wallet_id, wallet_db);

        let mut sessions = state.sessions.write()?;
        sessions.insert(session_id, session);

        Ok(response)
    }
}
