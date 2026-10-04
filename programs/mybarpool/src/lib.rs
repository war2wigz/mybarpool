//! MyBarPool: NFL boxes on Solana.
//!
//! Build plan Step 2: `PlatformConfig`, `WalletOverride`, every PROGRAM §1
//! constant, the complete §8 error enum, and the four §4.1 admin
//! instructions. Nothing here moves money or touches a pool. Later steps add
//! games, pools, the draw, settlement and returns, each built and audited
//! against `docs/PROGRAM.md`.
#![allow(unexpected_cfgs)]

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use errors::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("3jk4YM9xnpoMYK3nuaUHZ59SCDxwExfT9EHxwP2wRMBw");

#[program]
pub mod mybarpool {
    use super::*;

    /// PROGRAM §4.1 `initialize`: create `PlatformConfig`. Signer: the program's upgrade
    /// authority, which becomes `admin`. Emits `ConfigUpdated`.
    pub fn initialize(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, params)
    }

    /// PROGRAM §4.1 `update_config`: any subset of config fields; §3.1 invariants judged on the
    /// whole result. Signer: admin. Emits `ConfigUpdated`.
    pub fn update_config(ctx: Context<UpdateConfig>, params: UpdateConfigParams) -> Result<()> {
        instructions::update_config::handle_update_config(ctx, params)
    }

    /// PROGRAM §4.1 `set_wallet_override`: create or update a `WalletOverride`. Signer: admin.
    /// Emits `OverrideSet`.
    pub fn set_wallet_override(
        ctx: Context<SetWalletOverride>,
        wallet: Pubkey,
        max_open_pools: u8,
        max_own_boxes: u8,
    ) -> Result<()> {
        instructions::set_wallet_override::handle_set_wallet_override(
            ctx,
            wallet,
            max_open_pools,
            max_own_boxes,
        )
    }

    /// PROGRAM §4.1 `close_wallet_override`: close it, rent to admin. Signer: admin.
    /// Emits `OverrideClosed`.
    pub fn close_wallet_override(ctx: Context<CloseWalletOverride>, wallet: Pubkey) -> Result<()> {
        instructions::close_wallet_override::handle_close_wallet_override(ctx, wallet)
    }
}
