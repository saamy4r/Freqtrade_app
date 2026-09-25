//! One module per screen.

mod bots;
mod chart;
mod closed_trades;
mod dashboard;
pub(crate) mod loader;
mod logs;
mod open_trades;

pub use bots::Bots;
pub use chart::Chart;
pub use closed_trades::ClosedTrades;
pub use dashboard::Dashboard;
pub use logs::Logs;
pub use open_trades::OpenTrades;
