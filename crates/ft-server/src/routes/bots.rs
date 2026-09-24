//! Bot management: list, add, delete, reorder, ping.

use axum::extract::{Path, State};
use axum::Json;

use ft_client::FreqtradeClient;
use ft_store::NewBot;
use ft_types::api::{ActionResult, AddBotRequest, BotSummary, PingResult, ReorderRequest};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

fn summarize(bot: ft_store::BotRecord) -> BotSummary {
    BotSummary {
        id: bot.id,
        name: bot.name,
        url: bot.url,
        username: bot.username,
    }
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<BotSummary>>> {
    Ok(Json(
        state
            .store()
            .list_bots()?
            .into_iter()
            .map(summarize)
            .collect(),
    ))
}

/// Adds a bot, but only after proving the credentials work.
///
/// This mirrors `bots_screen.dart:227`, which performed a real login before
/// saving. Validating here rather than in the UI means the check cannot be
/// skipped, and the error distinguishes a wrong password from an unreachable
/// host instead of showing one generic failure.
pub async fn add(
    State(state): State<AppState>,
    Json(body): Json<AddBotRequest>,
) -> ApiResult<Json<BotSummary>> {
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("the bot needs a name".into()));
    }

    // Normalizes the URL and rejects an unusable one before any network call.
    let client = FreqtradeClient::new(&body.url, &body.username, &body.password)?;
    client.login().await?;

    let bot = state.store().add_bot(NewBot {
        name: body.name.trim().to_owned(),
        url: client.base_url().to_owned(),
        username: body.username,
        password: body.password,
    })?;
    tracing::info!(id = %bot.id, name = %bot.name, "bot added");
    Ok(Json(summarize(bot)))
}

/// Deletes a bot and everything cached for it.
pub async fn delete(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<Json<ActionResult>> {
    if !state.store().delete_bot(&bot_id)? {
        return Err(ApiError::UnknownBot(bot_id));
    }
    state.forget_client(&bot_id).await;
    Ok(Json(ActionResult {
        message: "bot removed".to_owned(),
    }))
}

pub async fn reorder(
    State(state): State<AppState>,
    Json(body): Json<ReorderRequest>,
) -> ApiResult<Json<Vec<BotSummary>>> {
    state.store().reorder_bots(&body.ids)?;
    list(State(state)).await
}

/// Unauthenticated liveness check, for the per-bot dot on the Bots screen.
///
/// Unreachable is reported as `online: false` rather than an error, because the
/// screen pings every bot at once and one being down is expected, not
/// exceptional.
pub async fn ping(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<Json<PingResult>> {
    let bot = state.bot(&bot_id)?;
    let online = FreqtradeClient::ping(&bot.url).await.unwrap_or(false);
    Ok(Json(PingResult { online }))
}
