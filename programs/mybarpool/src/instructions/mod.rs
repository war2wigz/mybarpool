//! Instruction handlers: PROGRAM §4.1 (administration) and §4.2 (games).

pub mod close_wallet_override;
pub mod create_game;
pub mod initialize;
pub mod mark_game;
pub mod mint_check;
pub mod post_scores;
pub mod set_wallet_override;
pub mod update_config;
pub mod update_kickoff;

pub use close_wallet_override::*;
pub use create_game::*;
pub use initialize::*;
pub use mark_game::*;
pub use post_scores::*;
pub use set_wallet_override::*;
pub use update_config::*;
pub use update_kickoff::*;
