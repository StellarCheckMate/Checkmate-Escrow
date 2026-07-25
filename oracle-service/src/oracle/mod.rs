pub mod chess_com_client;
pub mod lichess_client;
pub mod errors;

pub use chess_com_client::{ChessComClient, ChessComGameResult};
pub use errors::ChessComError;
pub use lichess_client::{LichessClient, LichessGameResult};
