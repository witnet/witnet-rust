use serde::Deserialize;

use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    pub(crate) session_id: types::SessionId,
}

pub type CloseSessionResponse = ();
