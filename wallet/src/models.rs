use diesel::Queryable;
use serde::Serialize;

#[derive(Debug, Clone, Queryable, Serialize)]
pub struct WalletInfo {
    pub id: i32,
    pub name: String,
    pub caption: Option<String>,
}
