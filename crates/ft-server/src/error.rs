//! Mapping internal failures onto HTTP.
//!
//! The point is to preserve the distinction `ft-client` works to establish:
//! unreachable is not the same as refused. The UI shows a retry for one and a
//! password prompt for the other.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use ft_types::api::{ApiErrorBody, ErrorKind};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Client(#[from] ft_client::ClientError),

    #[error("{0}")]
    Store(#[from] ft_store::StoreError),

    #[error("no bot with id {0}")]
    UnknownBot(String),

    /// The bot is unreachable and we have nothing cached to show.
    #[error("{what} is unavailable: the bot cannot be reached and nothing is cached")]
    NoCache { what: &'static str },

    #[error("{0}")]
    BadRequest(String),
}

impl ApiError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Client(e) if e.is_auth() => ErrorKind::Auth,
            Self::Client(e) if e.is_offline() => ErrorKind::Offline,
            Self::Client(_) => ErrorKind::Internal,
            Self::Store(ft_store::StoreError::UnknownBot(_)) | Self::UnknownBot(_) => {
                ErrorKind::NotFound
            }
            Self::Store(_) => ErrorKind::Internal,
            Self::NoCache { .. } => ErrorKind::Offline,
            Self::BadRequest(_) => ErrorKind::BadRequest,
        }
    }

    fn status(&self) -> StatusCode {
        match self.kind() {
            ErrorKind::Auth => StatusCode::UNAUTHORIZED,
            // 503 rather than 502: the bot is a dependency that is down, and
            // the UI should offer retry rather than treat it as a bug.
            ErrorKind::Offline => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let kind = self.kind();
        let message = self.to_string();
        if matches!(kind, ErrorKind::Internal) {
            tracing::error!(error = %message, "request failed");
        } else {
            tracing::debug!(error = %message, ?kind, "request rejected");
        }
        (self.status(), Json(ApiErrorBody { kind, message })).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
