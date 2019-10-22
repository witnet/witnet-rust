use super::*;

pub trait Handler {
    type Result;

    fn handle(self, state: &state::State) -> Self::Result;
}

impl Handler for requests::GetWalletInfos {
    type Result = api::Result<responses::WalletInfos>;

    fn handle(self, state: &state::State) -> Self::Result {
        let db = state.db.get()?;
        let infos = wallets::list(&db)?;
        let response = responses::WalletInfos { infos };

        Ok(response)
    }
}

impl Handler for requests::CreateMnemonics {
    type Result = api::Result<responses::Mnemonics>;

    fn handle(self, _state: &state::State) -> Self::Result {
        let len = self.validate()?;
        let mnemonics = crypto::gen_mnemonics(len);
        let response = responses::Mnemonics { mnemonics };

        Ok(response)
    }
}

impl Handler for requests::RunRadRequest {
    type Result = api::Result<responses::RadRequestResult>;

    fn handle(self, _state: &state::State) -> Self::Result {
        let response = radon::run_request(&self.rad_request)
            .map(responses::RadRequestResult::Value)
            .unwrap_or_else(|e| responses::RadRequestResult::Error(format!("{}", e)));

        Ok(response)
    }
}

impl Handler for requests::CreateWallet {
    type Result = api::Result<responses::WalletId>;

    fn handle(self, state: &state::State) -> Self::Result {
        let params = self.validate(&state.db_path)?;

        // Wallet master-key generation.
        let seed_password = state.wallets_config.seed_password.as_ref();
        let key_salt = state.wallets_config.master_key_salt.as_ref();
        let key_source = &params.seed_source;
        let master_key = crypto::gen_master_key(seed_password, key_salt, key_source)?;

        // Wallet default account keys.
        let account_index = constants::DEFAULT_ACCOUNT_INDEX;
        let account = account::gen_account(&state.sign_engine, account_index, &master_key)?;

        wallet::create(&params.db_url, params.password.as_ref(), &account)?;

        let wallet_id = wallets::create(&state.db.get()?, &params.name, params.caption.as_ref())?;

        let response = responses::WalletId { wallet_id };

        Ok(response)
    }
}
