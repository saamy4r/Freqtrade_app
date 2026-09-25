//! Server-sent events, so the UI learns about new data instead of asking.
//!
//! The Flutter app had no polling at all: values only changed when the user
//! pulled to refresh. Polling from the client would be the obvious fix and the
//! wrong one — every screen would poll separately, and a phone would wake its
//! radio on a timer whether or not anything had changed.
//!
//! Instead the server refreshes in the background and announces what changed.
//! The announcement deliberately carries no payload: the UI re-reads the
//! endpoint it already knows, which is a warm cache hit, so there is exactly
//! one shape for each screen's data rather than one for the fetch and another
//! for the push.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

/// What changed. Kept small on purpose — see the module note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// A bot's cached data was refreshed. `stale` is true when the refresh
    /// failed because the bot was unreachable, which is what flips the
    /// offline banner without the user touching anything.
    BotUpdated { bot_id: String, stale: bool },
    /// The bot list itself changed.
    BotsChanged,
}

/// `GET /api/events`
///
/// Holding this stream open is also what tells the server someone is watching;
/// the background sync runs only while at least one client is connected.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let guard = state.subscribe();
    let receiver = state.events().subscribe();

    let stream = BroadcastStream::new(receiver).filter_map(move |result| {
        // The guard lives inside the stream, so it drops when the client
        // disconnects, decrementing the watcher count.
        let _ = &guard;
        match result {
            Ok(event) => serde_json::to_string(&event)
                .ok()
                .map(|json| Ok(Event::default().data(json))),
            // Lagged: the client missed events. Nudging it to re-read is
            // enough, since events carry no payload to lose.
            Err(_) => serde_json::to_string(&ServerEvent::BotsChanged)
                .ok()
                .map(|json| Ok(Event::default().data(json))),
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
