#[inline]
pub fn wallets() -> &'static str {
    "wallets"
}

#[inline]
pub fn wallet_info(id: &str) -> String {
    format!("{}-wallet-info", id)
}

#[inline]
pub fn wallet(id: &str) -> String {
    format!("{}-wallet", id)
}
