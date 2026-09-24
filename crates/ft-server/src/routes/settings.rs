//! Small key/value settings: which bot is active, which theme.
//!
//! Server-side rather than in browser storage so the choice survives a
//! reinstall and is the same wherever the UI runs. The Flutter app kept these
//! in SharedPreferences under `active_bot_id` and `theme_mode`.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiResult;
use crate::state::AppState;

/// Key for the bot the UI last selected.
pub const ACTIVE_BOT: &str = "active_bot";

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingValue {
    pub value: Option<String>,
}

pub async fn get(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<Json<SettingValue>> {
    Ok(Json(SettingValue {
        value: state.store().setting(&key)?,
    }))
}

pub async fn put(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<SettingValue>,
) -> ApiResult<Json<SettingValue>> {
    match &body.value {
        Some(value) => state.store().set_setting(&key, value)?,
        // Writing null clears it, which is what deleting the active bot needs.
        None => state.store().set_setting(&key, "")?,
    }
    Ok(Json(body))
}
