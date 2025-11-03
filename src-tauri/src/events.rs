use serde::Serialize;

use crate::app_state::StatusSnapshot;

pub const STATUS_EVENT: &str = "touchgrass://status";
pub const REMINDER_EVENT: &str = "touchgrass://reminder";
pub const LOG_EVENT: &str = "touchgrass://log";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub status: StatusSnapshot,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPayload {
    pub level: String,
    pub message: String,
}
