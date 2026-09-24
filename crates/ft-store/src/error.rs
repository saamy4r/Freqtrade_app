//! Store errors.

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("could not open the database at {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: rusqlite::Error,
    },

    #[error("encryption key problem: {0}")]
    Key(String),

    #[error("{0}")]
    Crypto(String),

    #[error("could not encode stored data: {0}")]
    Encode(#[from] serde_json::Error),

    #[error("no bot with id {0}")]
    UnknownBot(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;
