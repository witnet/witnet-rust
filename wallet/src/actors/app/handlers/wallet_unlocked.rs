use actix::prelude::*;

use crate::actors::App;
use crate::wallet;

pub struct WalletUnlocked {
    pub session_id: String,
    pub unlocked_wallet: wallet::UnlockedWallet,
}

impl Message for WalletUnlocked {
    type Result = ();
}

impl Handler<WalletUnlocked> for App {
    type Result = ();

    fn handle(&mut self, msg: WalletUnlocked, _ctx: &mut Self::Context) -> Self::Result {
        self.assoc_wallet_to_session(msg.unlocked_wallet, msg.session_id)
    }
}
