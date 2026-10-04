//! Instruction handlers (PROGRAM §4.1, administration).

pub mod close_wallet_override;
pub mod initialize;
pub mod mint_check;
pub mod set_wallet_override;
pub mod update_config;

pub use close_wallet_override::*;
pub use initialize::*;
pub use set_wallet_override::*;
pub use update_config::*;
