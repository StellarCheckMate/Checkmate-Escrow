pub mod chess_com_client;
pub mod errors;
pub mod lichess_client;

pub use chess_com_client::{ChessComClient, ChessComGameResult};
pub use errors::{ChessComError, LichessError, OracleServiceError};
pub use lichess_client::{LichessClient, LichessGameResult};

/// Local re-export of the on-chain `Winner` enum so callers don't need to
/// depend on `contracts_oracle` directly.
pub use contracts_oracle::types::Winner;
