//! `buy`, PROGRAM §4.3, and the purchase step it shares with `create_pool`
//! (so the creator's initial boxes run the identical code path: cap, pot,
//! assignment, lock, counter).

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use solana_sdk_ids::sysvar;

use crate::constants::{BOXES, CONFIG_SEED, COUNTER_SEED, OVERRIDE_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::{BoxesBought, PoolLocked};
use crate::slot_hashes::most_recent_slot_hash;
use crate::state::{CreatorCounter, GameRecord, PlatformConfig, Pool, PoolStatus, WalletOverride};
use crate::vault::{transfer_in_sol, transfer_in_spl};

/// The two creator limits in force for one wallet: the `WalletOverride` when the slot at
/// `["override", creator]` is initialised, else the config (PROGRAM §3.6, ARCHITECTURE › Limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_open_pools: u8,
    pub max_own_boxes: u8,
}

impl Limits {
    /// Read the override if the account at the canonical address carries this program's data
    /// (owner is this program and it deserialises), else the config's two values. The slot is a
    /// required account so a client cannot omit an existing override to escape a lower limit.
    pub fn for_creator(config: &PlatformConfig, wallet_override: &AccountInfo) -> Result<Self> {
        if *wallet_override.owner == crate::ID && !wallet_override.data_is_empty() {
            let data = wallet_override.try_borrow_data()?;
            let mut slice: &[u8] = &data;
            let o = WalletOverride::try_deserialize(&mut slice)?;
            return Ok(Self {
                max_open_pools: o.max_open_pools,
                max_own_boxes: o.max_own_boxes,
            });
        }
        Ok(Self {
            max_open_pools: config.max_open_pools,
            max_own_boxes: config.max_own_boxes,
        })
    }
}

/// What a purchase did, for the caller's events.
pub struct Purchase {
    /// 0-based indices in assignment order.
    pub boxes: Vec<u8>,
    /// The 25th box sold in this purchase.
    pub locked: bool,
}

/// PROGRAM §4.3 `buy` effects, shared with `create_pool`. Checks `count` and the creator cap,
/// runs `transfer(amount)` for `count × price`, assigns boxes (§6.1), advances `sold` and
/// `creator_boxes`, and on the 25th box sets `Locked`, `locked_at`, and decrements the counter.
/// The caller has already checked `paused`, the pool status, the game and the sales window.
#[allow(clippy::too_many_arguments)]
pub fn purchase(
    pool: &mut Pool,
    counter: &mut CreatorCounter,
    buyer: &Pubkey,
    count: u8,
    slothash: &[u8; 32],
    now: i64,
    limits: Limits,
    transfer: impl FnOnce(u64) -> Result<()>,
) -> Result<Purchase> {
    require!(count >= 1, MybarpoolError::NothingToBuy);
    let remaining = BOXES
        .checked_sub(pool.sold)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(count <= remaining, MybarpoolError::TooManyBoxes);
    let is_creator = *buyer == pool.creator;
    if is_creator {
        let after = pool
            .creator_boxes
            .checked_add(count)
            .ok_or(MybarpoolError::MathOverflow)?;
        require!(after <= limits.max_own_boxes, MybarpoolError::OwnBoxLimit);
    }

    let amount = pool
        .price
        .checked_mul(u64::from(count))
        .ok_or(MybarpoolError::MathOverflow)?;
    transfer(amount)?;

    let boxes = pool.assign_boxes(buyer, count, slothash)?;
    pool.sold = pool
        .sold
        .checked_add(count)
        .ok_or(MybarpoolError::MathOverflow)?;
    if is_creator {
        pool.creator_boxes = pool
            .creator_boxes
            .checked_add(count)
            .ok_or(MybarpoolError::MathOverflow)?;
    }
    let locked = pool.sold == BOXES;
    if locked {
        pool.status = PoolStatus::Locked;
        pool.locked_at = now;
        counter.open_count = counter
            .open_count
            .checked_sub(1)
            .ok_or(MybarpoolError::MathOverflow)?;
    }
    Ok(Purchase { boxes, locked })
}

#[derive(Accounts)]
#[event_cpi]
pub struct Buy<'info> {
    /// The buyer; pays `count × price` and the transaction fee.
    #[account(mut)]
    pub buyer: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `paused` and the limits.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); read for status and kickoff.
    pub game: Account<'info, GameRecord>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
        has_one = game,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds + the stored `vault_bump` pin it to `pool.vault`.
    /// PROGRAM §3.4 vault at `["vault", pool]` with the stored bump; `pool.vault` is the
    /// only address `buy` accepts for it. SOL: system-owned, lamports only. SPL: a token account
    /// owned by the pool PDA, which `transfer_checked` into it verifies.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// PROGRAM §3.5 the creator's counter for this game; decremented when this buy locks.
    #[account(
        mut,
        seeds = [COUNTER_SEED, pool.creator.as_ref(), pool.game.as_ref()],
        bump = counter.bump,
    )]
    pub counter: Account<'info, CreatorCounter>,
    /// CHECK: seeds pin it to the canonical address; decoded only when it holds this program's data.
    /// The creator's `WalletOverride` slot at `["override", pool.creator]` (seeds
    /// constraint); read only when initialised and only when the buyer is the creator.
    #[account(seeds = [OVERRIDE_SEED, pool.creator.as_ref()], bump)]
    pub wallet_override: UncheckedAccount<'info>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// The buyer's token account for the mint; SPL pools only.
    #[account(mut, token::mint = mint)]
    pub buyer_token_account: Option<InterfaceAccount<'info, TokenAccount>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// CHECK: `address = sysvar::slot_hashes::ID`.
    /// The SlotHashes sysvar by address; PROGRAM §6.1's `slothash` comes from it and
    /// never from instruction data.
    #[account(address = sysvar::slot_hashes::ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` names `config` and `game` here: both are read-only and the lint
/// reads a MIR temporary derived from a field read as a write (the Step 2 false positive).
/// Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without
/// this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_buy(ctx: Context<Buy>, count: u8) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let Buy {
        buyer,
        config,
        game,
        pool,
        vault,
        counter,
        wallet_override,
        mint,
        buyer_token_account,
        token_program,
        slot_hashes,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.3 checks, in order.
    require!(!config.paused, MybarpoolError::Paused);
    pool.require_open()?;
    require!(now < game.recorded_kickoff, MybarpoolError::SalesClosed);
    game.require_scheduled()?;

    let slothash = most_recent_slot_hash(slot_hashes)?;
    let limits = Limits::for_creator(config, wallet_override)?;
    let pool_key = pool.key();
    let buyer_key = buyer.key();

    // Token plumbing: for an SPL pool the three optional accounts are required and the program
    // must be the one the rule named at creation (Anchor's own errors, not §8's).
    let spl = if pool.token != 0 {
        let mint = mint.as_ref().ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let from = buyer_token_account
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let token_program = token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(token_program.key(), pool.token_program);
        Some((mint, from, token_program))
    } else {
        None
    };

    let vault_info = vault.to_account_info();
    let system_info = system_program.to_account_info();
    let result = purchase(
        pool,
        counter,
        &buyer_key,
        count,
        &slothash,
        now,
        limits,
        |amount| match spl {
            None => transfer_in_sol(buyer, &vault_info, &system_info, amount),
            Some((mint, from, token_program)) => {
                transfer_in_spl(from, mint, &vault_info, buyer, token_program, amount)
            }
        },
    )?;

    emit_cpi!(BoxesBought {
        time: now,
        pool: pool_key,
        buyer: buyer_key,
        boxes: result.boxes,
        count,
        sold_after: pool.sold,
    });
    if result.locked {
        emit_cpi!(PoolLocked {
            time: now,
            pool: pool_key,
            locked_at: now,
        });
    }
    Ok(())
}
