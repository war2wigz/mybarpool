//! `create_pool`, PROGRAM §4.3. Signer and payer: the creator. The first
//! instruction that holds money: it creates the pool, its vault (§3.4) and,
//! when absent, the creator's counter (§3.5), fixes the fee amounts (§5.1),
//! and optionally runs the creator's first purchase through the same code
//! path as `buy`.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use solana_sdk_ids::sysvar;

use crate::constants::{
    PayoutPreset, BOXES, CONFIG_SEED, COUNTER_SEED, GAME_SEED, NO_WINNING_BOX, OVERRIDE_SEED,
    POOL_SEED, QUARTERS, VAULT_SEED,
};
use crate::errors::MybarpoolError;
use crate::events::{BoxesBought, PoolCreated, PoolLocked};
use crate::instructions::buy::{purchase, Limits};
use crate::money::{fee_amounts, price_on_ladder};
use crate::slot_hashes::most_recent_slot_hash;
use crate::state::{AccessType, CreatorCounter, GameRecord, PlatformConfig, Pool, PoolStatus};
use crate::vault::{
    create_spl_vault, fund_sol_vault, transfer_in_sol, transfer_in_spl, vault_signer_seeds,
};

/// PROGRAM §4.3 `create_pool` parameters, in the §4.3 order.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatePoolParams {
    /// Client-chosen; part of the pool's seeds (PROGRAM §3.3).
    pub nonce: u64,
    /// PROGRAM §2 token index: 0 SOL, 1 SKR, 2 ORE.
    pub token: u8,
    /// Per box, base units; must be on the token's ladder.
    pub price: u64,
    /// PROGRAM §1 payout preset; a byte outside 0–2 fails to deserialise.
    pub preset: PayoutPreset,
    /// PROGRAM §3.3 access type; a byte outside 0–2 fails to deserialise. Step 4: `Public` only.
    pub access_type: AccessType,
    /// Co-signer for `Link`; must be default otherwise.
    pub gate_key: Pubkey,
    /// Merkle root for `Allowlist`; must be zero otherwise.
    pub allowlist_root: [u8; 32],
    /// 0–500, within `config.addon_budget_bps` together with `integrator_bps`.
    pub creator_addon_bps: u16,
    /// The client taking `integrator_bps`; default iff `integrator_bps == 0`.
    pub integrator: Pubkey,
    /// 0–500.
    pub integrator_bps: u16,
    /// Boxes the creator buys in the same instruction, 0–`max_own_boxes`.
    pub initial_boxes: u8,
}

#[derive(Accounts)]
#[instruction(params: CreatePoolParams)]
#[event_cpi]
pub struct CreatePool<'info> {
    /// The creator; pays the rent of the pool, the vault and (first time) the counter, plus any
    /// initial boxes.
    #[account(mut)]
    pub creator: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `paused`, the token rule, the fee bps and limits.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.2 `GameRecord`, re-derived from its own fields (the Step 3 discipline).
    #[account(
        seeds = [
            GAME_SEED,
            &game.key.season.to_le_bytes(),
            &[game.key.week],
            &[game.key.home],
            &[game.key.away],
            &game.scheduled_kickoff.to_le_bytes(),
        ],
        bump = game.bump,
    )]
    pub game: Account<'info, GameRecord>,
    /// PROGRAM §3.3 `Pool` at `["pool", game, creator, nonce]`; must not exist yet.
    #[account(
        init,
        payer = creator,
        space = Pool::SIZE,
        seeds = [POOL_SEED, game.key().as_ref(), creator.key().as_ref(), &params.nonce.to_le_bytes()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds pin it to the pool; this handler funds or initialises it and only it.
    /// PDA of the pool at `["vault", pool]` (seeds constraint); funded (SOL) or
    /// initialised as a token account (SPL) by this handler, nothing else.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump)]
    pub vault: UncheckedAccount<'info>,
    /// PROGRAM §3.5 `CreatorCounter` at `["counter", creator, game]`; created when absent.
    #[account(
        init_if_needed,
        payer = creator,
        space = CreatorCounter::SIZE,
        seeds = [COUNTER_SEED, creator.key().as_ref(), game.key().as_ref()],
        bump,
    )]
    pub counter: Account<'info, CreatorCounter>,
    /// CHECK: seeds pin it to the canonical address; decoded only when it holds this program's data.
    /// The creator's `WalletOverride` slot at `["override", creator]` (seeds
    /// constraint); read only when initialised.
    #[account(seeds = [OVERRIDE_SEED, creator.key().as_ref()], bump)]
    pub wallet_override: UncheckedAccount<'info>,
    /// The rule's mint; SPL pools only (checked against `rule.mint`).
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// The creator's token account for the mint; SPL pools with `initial_boxes > 0` only.
    #[account(mut, token::mint = mint)]
    pub creator_token_account: Option<InterfaceAccount<'info, TokenAccount>>,
    /// The rule's token program; SPL pools only (checked against `rule.token_program`).
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// CHECK: `address = sysvar::slot_hashes::ID`.
    /// The SlotHashes sysvar by address; read only when `initial_boxes > 0`.
    #[account(address = sysvar::slot_hashes::ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` names `config` and `game` here: both are read-only and the lint
/// reads a MIR temporary derived from a field read as a write (the Step 2 false positive).
/// Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without
/// this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_create_pool(ctx: Context<CreatePool>, params: CreatePoolParams) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let pool_bump = ctx.bumps.pool;
    let vault_bump = ctx.bumps.vault;
    let counter_bump = ctx.bumps.counter;
    let CreatePool {
        creator,
        config,
        game,
        pool,
        vault,
        counter,
        wallet_override,
        mint,
        creator_token_account,
        token_program,
        slot_hashes,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.3 checks, in order.
    require!(!config.paused, MybarpoolError::Paused);
    game.require_scheduled()?;
    require!(now < game.recorded_kickoff, MybarpoolError::SalesClosed);
    let rule = config
        .tokens
        .get(usize::from(params.token))
        .ok_or(MybarpoolError::TokenDisabled)?;
    require!(rule.enabled, MybarpoolError::TokenDisabled);

    // Token plumbing (Anchor's own errors): for an SPL token the mint and program are required
    // and must be the rule's; the creator's token account only when boxes are bought.
    let spl = if params.token != 0 {
        let mint = mint.as_ref().ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let token_program = token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(mint.key(), rule.mint);
        require_keys_eq!(token_program.key(), rule.token_program);
        let from = if params.initial_boxes > 0 {
            Some(
                creator_token_account
                    .as_ref()
                    .ok_or(ErrorCode::ConstraintAccountIsNone)?,
            )
        } else {
            None
        };
        Some((mint, token_program, from))
    } else {
        None
    };

    price_on_ladder(rule, params.price)?;
    // PROGRAM §4.3: gate_key != default iff Link; allowlist_root != 0 iff Allowlist.
    require!(
        (params.gate_key != Pubkey::default()) == (params.access_type == AccessType::Link),
        MybarpoolError::InvalidAccessType
    );
    require!(
        (params.allowlist_root != [0u8; 32]) == (params.access_type == AccessType::Allowlist),
        MybarpoolError::InvalidAccessType
    );
    // Step 4: Public only; gating in `buy` lands in Step 8, which removes this line.
    require!(
        params.access_type == AccessType::Public,
        MybarpoolError::InvalidAccessType
    );
    let addon_total = params
        .creator_addon_bps
        .checked_add(params.integrator_bps)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(
        addon_total <= config.addon_budget_bps,
        MybarpoolError::AddonBudgetExceeded
    );
    require!(
        (params.integrator_bps == 0) == (params.integrator == Pubkey::default()),
        MybarpoolError::IntegratorMismatch
    );
    let limits = Limits::for_creator(config, wallet_override)?;
    require!(
        counter.open_count < limits.max_open_pools,
        MybarpoolError::OpenPoolLimit
    );
    require!(
        params.initial_boxes <= limits.max_own_boxes,
        MybarpoolError::OwnBoxLimit
    );

    let fees = fee_amounts(
        params.price,
        config.platform_bps,
        config.creator_bps,
        params.creator_addon_bps,
        params.integrator_bps,
    )?;
    let rule = *rule;
    let creator_key = creator.key();
    let game_key = game.key();
    let pool_key = pool.key();
    let vault_key = vault.key();
    let slothash = if params.initial_boxes > 0 {
        Some(most_recent_slot_hash(slot_hashes)?)
    } else {
        None
    };

    // The vault: SOL funded, SPL created and initialised (PROGRAM §3.4).
    let vault_info = vault.to_account_info();
    let pool_info = pool.to_account_info();
    let system_info = system_program.to_account_info();
    match spl {
        None => {
            fund_sol_vault(creator, &vault_info, &system_info)?;
        }
        Some((mint, token_program, _)) => {
            let bump = [vault_bump];
            let seeds = vault_signer_seeds(&pool_key, &bump);
            create_spl_vault(
                creator,
                &vault_info,
                &pool_info,
                mint,
                token_program,
                &system_info,
                &seeds,
            )?;
        }
    }

    // The counter: fresh when `creator` is still the default.
    if counter.creator == Pubkey::default() {
        counter.creator = creator_key;
        counter.game = game_key;
        counter.bump = counter_bump;
    }
    counter.open_count = counter
        .open_count
        .checked_add(1)
        .ok_or(MybarpoolError::MathOverflow)?;

    // The pool: every field written (PROGRAM §3.3).
    pool.game = game_key;
    pool.creator = creator_key;
    pool.nonce = params.nonce;
    pool.token = params.token;
    pool.mint = rule.mint;
    pool.token_program = rule.token_program;
    pool.vault = vault_key;
    pool.price = params.price;
    pool.preset = params.preset;
    pool.access_type = params.access_type;
    pool.gate_key = params.gate_key;
    pool.allowlist_root = params.allowlist_root;
    pool.creator_addon_bps = params.creator_addon_bps;
    pool.integrator = params.integrator;
    pool.integrator_bps = params.integrator_bps;
    pool.platform_fee = fees.platform_fee;
    pool.creator_fee = fees.creator_fee;
    pool.integrator_fee = fees.integrator_fee;
    pool.status = PoolStatus::Open;
    pool.sold = 0;
    pool.owners = [Pubkey::default(); BOXES as usize];
    pool.creator_boxes = 0;
    pool.sponsored_total = 0;
    pool.sponsor_count = 0;
    pool.sponsorships_open = 0;
    pool.var = Pubkey::default();
    pool.var_end_at = 0;
    pool.sampled_slot = 0;
    pool.sampled_hash = [0u8; 32];
    pool.var_replacements = 0;
    pool.drawn = false;
    pool.home_axis = [0u8; 10];
    pool.away_axis = [0u8; 10];
    pool.prize_pool = 0;
    pool.quarter_prize = [0u64; QUARTERS as usize];
    pool.quarters_settled = 0;
    pool.winning_box = [NO_WINNING_BOX; QUARTERS as usize];
    pool.fees_paid = false;
    pool.unpaid_prize_pool = 0;
    pool.returned = 0;
    pool.split_amount = 0;
    pool.cancelled_by_admin = false;
    pool.abandoned = false;
    pool.created_at = now;
    pool.locked_at = 0;
    pool.bump = pool_bump;
    pool.vault_bump = vault_bump;
    pool.var_commit = [0u8; 32];
    pool.reserved = [0u8; 96];

    emit_cpi!(PoolCreated {
        time: now,
        pool: pool_key,
        game: game_key,
        creator: creator_key,
        token: params.token,
        mint: rule.mint,
        price: params.price,
        preset: params.preset,
        access_type: params.access_type,
        creator_addon_bps: params.creator_addon_bps,
        integrator: params.integrator,
        integrator_bps: params.integrator_bps,
        platform_fee: fees.platform_fee,
        creator_fee: fees.creator_fee,
        integrator_fee: fees.integrator_fee,
    });

    // The creator's initial boxes: the same path as `buy` (PROGRAM §4.3 "runs the buy logic").
    if let Some(slothash) = slothash {
        let result = purchase(
            pool,
            counter,
            &creator_key,
            params.initial_boxes,
            &slothash,
            now,
            limits,
            |amount| match spl {
                None => transfer_in_sol(creator, &vault_info, &system_info, amount),
                Some((mint, token_program, from)) => transfer_in_spl(
                    from.ok_or(ErrorCode::ConstraintAccountIsNone)?,
                    mint,
                    &vault_info,
                    creator,
                    token_program,
                    amount,
                ),
            },
        )?;
        emit_cpi!(BoxesBought {
            time: now,
            pool: pool_key,
            buyer: creator_key,
            boxes: result.boxes,
            count: params.initial_boxes,
            sold_after: pool.sold,
        });
        if result.locked {
            emit_cpi!(PoolLocked {
                time: now,
                pool: pool_key,
                locked_at: now,
            });
        }
    }
    Ok(())
}
