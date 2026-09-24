//! One module per screen.

mod bots;
mod closed_trades;
pub(crate) mod loader;
mod open_trades;
mod placeholder;

pub use bots::Bots;
pub use closed_trades::ClosedTrades;
pub use open_trades::OpenTrades;
pub use placeholder::Placeholder;
