use serde::Deserialize;

use crate::app;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    pub(crate) session_id: app::SessionId,
}

pub type CloseSessionResponse = ();
