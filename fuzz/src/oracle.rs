//! The PROGRAM §10 Money oracle (build plan Step 8 › Fuzzer), checked on every mutated run.
//!
//! - O1 **No crash.** The program never fails to complete (panic, abort, compute exhaustion);
//!   every failure is a `Custom(code)` or one of the runtime's own account checks.
//! - O2 **Atomic.** On any failure every resulting account equals its input.
//! - O3 **Money, on success.** Lamports are conserved over the account set; per mint, token
//!   amounts are conserved; the accounts whose lamports or token balance increased are a subset
//!   of the destinations the instruction allows, read from the *resulting* pool and config.
//! - O4 **Vault invariant, on success.** The vault holds PROGRAM §3.4's amount for the
//!   resulting pool's status.
//! - O5 **`buy` relations, on success.** `sold` grew by `count`; exactly `count` boxes went from
//!   default to the buyer; nothing else in `owners` changed; the own-box cap with override
//!   precedence; `Locked` iff `sold == 25`, then `locked_at = now` and the counter decremented.
//!
//! `missing_mut_constraint` (anchor-lints `d8116dd`) reads every access to a field of an Anchor
//! `#[account]` struct as a write to an account of the instruction being analysed; this
//! crate has no instructions and no accounts, it decodes the program's state to judge it, so
//! the lint is allowed for the module (listed in `AUDIT-READINESS.md`).
#![cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]

use std::collections::{BTreeMap, BTreeSet};

use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountDeserialize, Discriminator};
use mybarpool::{CreatorCounter, PlatformConfig, Pool, PoolStatus, Sponsorship, WalletOverride};

use crate::model::{Account, Context, Key, Outcome, Result as Res};
use crate::seeds::instruction_name;

/// A violated oracle line, for the failure report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub line: &'static str,
    pub detail: String,
}

fn v(line: &'static str, detail: impl Into<String>) -> Violation {
    Violation {
        line,
        detail: detail.into(),
    }
}

/// The label of Mollusk's post-execution rent check: the program succeeded, an account ended
/// below its rent-exempt minimum, and the runtime would have rejected the transaction with
/// nothing changed. Mollusk reports the post-execution accounts for it, so O2 is not evaluated
/// (the runtime's rejection *is* the atomicity).
pub const RENT_CHECK: &str = "AccountNotRentExempt (post-execution rent check)";

/// The SPL Token program.
pub const TOKEN_PROGRAM: Key =
    Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").to_bytes();
/// The Token-2022 program.
pub const TOKEN_2022_PROGRAM: Key =
    Pubkey::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").to_bytes();
/// PROGRAM §1 `BOXES`.
pub const BOXES: usize = 25;

/// The rent-exempt minimum of a 0-byte account, the SOL vault's own floor (PROGRAM §3.4).
pub fn vault_rent_floor() -> u64 {
    anchor_lang::prelude::Rent::default().minimum_balance(0)
}

/// `(mint, owner wallet, amount)` when `a` is an SPL token account.
pub fn token_account(a: &Account) -> Option<(Key, Key, u64)> {
    if (a.owner != TOKEN_PROGRAM && a.owner != TOKEN_2022_PROGRAM) || a.data.len() < 165 {
        return None;
    }
    let mint: Key = a.data[0..32].try_into().ok()?;
    let owner: Key = a.data[32..64].try_into().ok()?;
    let amount = u64::from_le_bytes(a.data[64..72].try_into().ok()?);
    Some((mint, owner, amount))
}

fn decode<T: AccountDeserialize + Discriminator>(a: &Account) -> Option<T> {
    if a.owner != mybarpool::ID.to_bytes() || a.data.len() < 8 || a.data[..8] != *T::DISCRIMINATOR {
        return None;
    }
    T::try_deserialize(&mut &a.data[..]).ok()
}

fn find<T: AccountDeserialize + Discriminator>(accounts: &[Account]) -> Option<(usize, T)> {
    accounts
        .iter()
        .enumerate()
        .find_map(|(i, a)| decode::<T>(a).map(|t| (i, t)))
}

/// Every account of type `T` in the list, with its key (a context can hold several pools).
fn find_all<T: AccountDeserialize + Discriminator>(accounts: &[Account]) -> Vec<(Key, T)> {
    accounts
        .iter()
        .filter_map(|a| decode::<T>(a).map(|t| (a.key, t)))
        .collect()
}

fn short(k: &Key) -> String {
    k.iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        + "…"
}

fn popcount(bits: u32) -> u64 {
    u64::from(bits.count_ones())
}

/// Everything the oracle found wrong with one run.
pub fn check(before: &Context, after: &Outcome) -> Vec<Violation> {
    let mut out = Vec::new();
    out.extend(o1_no_crash(after));
    if after.result != Res::Success {
        if after.result != Res::Runtime(RENT_CHECK.to_string()) {
            out.extend(o2_atomic(before, after));
        }
        return out;
    }
    out.extend(o3_money(before, after));
    out.extend(o4_vault(before, after));
    if instruction_name(before) == Some("buy") {
        out.extend(o5_buy(before, after));
    }
    out
}

/// O1: the program finished.
pub fn o1_no_crash(after: &Outcome) -> Vec<Violation> {
    match &after.result {
        Res::FailedToComplete => vec![v("O1 no crash", "the program did not complete")],
        _ => Vec::new(),
    }
}

/// O2: a failure changed nothing.
pub fn o2_atomic(before: &Context, after: &Outcome) -> Vec<Violation> {
    let mut out = Vec::new();
    for a in &before.accounts {
        match after.account(&a.key) {
            None => out.push(v(
                "O2 atomic",
                format!("{} missing from the result", short(&a.key)),
            )),
            Some(b) => {
                if a.lamports != b.lamports || a.data != b.data || a.owner != b.owner {
                    out.push(v(
                        "O2 atomic",
                        format!(
                            "{} changed on failure (lamports {} → {}, data {} → {} bytes, owner {})",
                            short(&a.key),
                            a.lamports,
                            b.lamports,
                            a.data.len(),
                            b.data.len(),
                            if a.owner == b.owner { "same" } else { "changed" }
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// The wallets and accounts an instruction may pay, read from the resulting state.
pub struct Destinations {
    /// Wallets that may receive lamports directly or through their token accounts.
    pub wallets: BTreeSet<Key>,
    /// Accounts that may receive lamports or tokens by key (the vault, state accounts' rent).
    pub accounts: BTreeSet<Key>,
}

/// PROGRAM §10 Money's destination list for the instruction, from the *resulting* accounts;
/// a closing instruction empties the pool (or the config is absent), so the input's copy
/// stands in when the result has none — the wallets it names are the ones that were paid.
pub fn destinations(before: &Context, after: &Outcome) -> Destinations {
    let mut wallets = BTreeSet::new();
    let mut accounts = BTreeSet::new();
    let name = instruction_name(before);
    let mut pools = find_all::<Pool>(&after.accounts);
    for (k, p) in find_all::<Pool>(&before.accounts) {
        if !pools.iter().any(|(x, _)| *x == k) {
            pools.push((k, p)); // closed by this instruction: the input's copy names who was paid
        }
    }
    let config = find::<PlatformConfig>(&after.accounts)
        .or_else(|| find::<PlatformConfig>(&before.accounts))
        .map(|(_, c)| c);
    for (key, p) in &pools {
        // The program addresses the vault by its seeds (`["vault", pool]`, PROGRAM §3.4), not by
        // the pool's cached `vault` field, so a planted field cannot redirect anything; both are
        // permitted here so the fuzzer's plants do not trip the oracle.
        accounts.insert(p.vault.to_bytes());
        let (pda, _) = Pubkey::find_program_address(&[b"vault", key], &mybarpool::ID);
        accounts.insert(pda.to_bytes());
    }
    match name {
        Some("buy") => {
            // The vault only; nothing is created in `buy`.
        }
        Some("settle") => {
            for (_, p) in &pools {
                let q = usize::from(p.quarters_settled.saturating_sub(1));
                if let Some(&b) = p.winning_box.get(q) {
                    if let Some(w) = p.owners.get(usize::from(b)) {
                        wallets.insert(w.to_bytes());
                    }
                }
                wallets.insert(p.creator.to_bytes());
                if p.integrator != Pubkey::default() {
                    wallets.insert(p.integrator.to_bytes());
                }
            }
            if let Some(c) = &config {
                wallets.insert(c.fee_wallet.to_bytes());
            }
        }
        _ => {
            for (_, p) in &pools {
                for o in &p.owners {
                    if *o != Pubkey::default() {
                        wallets.insert(o.to_bytes());
                    }
                }
                wallets.insert(p.creator.to_bytes());
                if p.integrator != Pubkey::default() {
                    wallets.insert(p.integrator.to_bytes());
                }
            }
            if let Some(c) = &config {
                wallets.insert(c.fee_wallet.to_bytes());
                // `close_wallet_override` closes to the admin (PROGRAM §4.1).
                if name == Some("close_wallet_override") {
                    wallets.insert(c.admin.to_bytes());
                }
            }
            for a in before.accounts.iter().chain(after.accounts.iter()) {
                if let Some(s) = decode::<Sponsorship>(a) {
                    wallets.insert(s.wallet.to_bytes());
                }
            }
            // A state account created by the instruction receives its rent from the payer.
            for a in &after.accounts {
                if a.owner == mybarpool::ID.to_bytes() && !a.data.is_empty() {
                    accounts.insert(a.key);
                }
            }
        }
    }
    Destinations { wallets, accounts }
}

/// O3: conservation and destinations.
pub fn o3_money(before: &Context, after: &Outcome) -> Vec<Violation> {
    let mut out = Vec::new();
    let lamports_before: u128 = before.accounts.iter().map(|a| u128::from(a.lamports)).sum();
    let lamports_after: u128 = after.accounts.iter().map(|a| u128::from(a.lamports)).sum();
    if lamports_before != lamports_after {
        out.push(v(
            "O3 lamports conserved",
            format!("{lamports_before} before, {lamports_after} after"),
        ));
    }
    let mut by_mint_before: BTreeMap<Key, u128> = BTreeMap::new();
    let mut by_mint_after: BTreeMap<Key, u128> = BTreeMap::new();
    for a in &before.accounts {
        if let Some((mint, _, amount)) = token_account(a) {
            *by_mint_before.entry(mint).or_default() += u128::from(amount);
        }
    }
    for a in &after.accounts {
        if let Some((mint, _, amount)) = token_account(a) {
            *by_mint_after.entry(mint).or_default() += u128::from(amount);
        }
    }
    for mint in by_mint_before
        .keys()
        .chain(by_mint_after.keys())
        .collect::<BTreeSet<_>>()
    {
        let b = by_mint_before.get(mint).copied().unwrap_or(0);
        let a = by_mint_after.get(mint).copied().unwrap_or(0);
        if a != b {
            out.push(v(
                "O3 tokens conserved",
                format!("mint {}: {b} before, {a} after", short(mint)),
            ));
        }
    }

    let allowed = destinations(before, after);
    for a in &after.accounts {
        let b = before.account(&a.key);
        let lamports_up = b.is_none_or(|b| a.lamports > b.lamports) && a.lamports > 0;
        let tokens_up = match (b.and_then(token_account), token_account(a)) {
            (Some((_, _, x)), Some((_, _, y))) => y > x,
            (None, Some((_, _, y))) => y > 0,
            _ => false,
        };
        if !lamports_up && !tokens_up {
            continue;
        }
        let ok = allowed.accounts.contains(&a.key)
            || allowed.wallets.contains(&a.key)
            || token_account(a).is_some_and(|(_, owner, _)| allowed.wallets.contains(&owner));
        if !ok {
            out.push(v(
                "O3 destinations",
                format!(
                    "{} gained ({}) and is not a permitted destination",
                    short(&a.key),
                    if lamports_up { "lamports" } else { "tokens" }
                ),
            ));
        }
    }
    out
}

/// The vault's whole balance: lamports for a SOL pool, the token amount for an SPL pool
/// (zero when the slot is not a token account yet).
fn vault_total(pool: &Pool, accounts: &[Account]) -> Option<u64> {
    let key = pool.vault.to_bytes();
    let a = accounts.iter().find(|a| a.key == key)?;
    if pool.token == 0 {
        Some(a.lamports)
    } else {
        Some(token_account(a).map(|(_, _, amount)| amount).unwrap_or(0))
    }
}

/// PROGRAM §3.4's amount for `pool` given the accounts around it. In `Returned` the open
/// `Sponsorship` accounts *in the context* are summed; ones outside it are the same before
/// and after, so the discrepancy comparison below cancels them.
pub fn expected_vault(pool: &Pool, pool_key: &Key, accounts: &[Account]) -> u128 {
    // Wide arithmetic: a planted pool can carry any field value, and the oracle must report
    // the mismatch rather than overflow itself.
    let w = u128::from;
    let fees = if pool.fees_paid {
        0
    } else {
        w(pool.platform_fee) + w(pool.creator_fee) + w(pool.integrator_fee)
    };
    match pool.status {
        PoolStatus::Open | PoolStatus::Locked | PoolStatus::Drawn => {
            if pool.quarters_settled == 0 {
                u128::from(pool.sold) * w(pool.price) + w(pool.sponsored_total)
            } else {
                w(pool.unpaid_prize_pool) + fees
            }
        }
        PoolStatus::Settled => w(pool.unpaid_prize_pool) + fees,
        PoolStatus::Split => w(pool.unpaid_prize_pool),
        PoolStatus::Returned => {
            let unreturned = u64::from(pool.sold).saturating_sub(popcount(pool.returned));
            let open: u128 = accounts
                .iter()
                .filter_map(decode::<Sponsorship>)
                .filter(|s| s.pool.to_bytes() == *pool_key)
                .map(|s| w(s.amount))
                .sum();
            w(pool.price) * w(unreturned) + open
        }
    }
}

/// What §3.4 says the vault's whole balance is: the formula, plus the SOL vault's own floor.
fn expected_total(pool: &Pool, pool_key: &Key, accounts: &[Account]) -> u128 {
    let floor = if pool.token == 0 {
        vault_rent_floor()
    } else {
        0
    };
    u128::from(floor) + expected_vault(pool, pool_key, accounts)
}

/// The PROGRAM §3.3 relations every pool the program writes satisfies; a planted pool that
/// breaks one is a state the program never produces, and §3.4 says nothing about it. Returns
/// the first broken relation.
pub fn well_formed(p: &Pool) -> Result<(), &'static str> {
    let owned = p.owners.iter().filter(|o| **o != Pubkey::default()).count();
    if owned != usize::from(p.sold) || usize::from(p.sold) > BOXES {
        return Err("sold ≠ owned boxes");
    }
    let creators = p.owners.iter().filter(|o| **o == p.creator).count();
    if creators != usize::from(p.creator_boxes) {
        return Err("creator_boxes ≠ the creator's boxes");
    }
    let no_integrator = p.integrator == Pubkey::default();
    if no_integrator != (p.integrator_bps == 0) || no_integrator != (p.integrator_fee == 0) {
        return Err("integrator iff integrator_bps iff integrator_fee (§4.3)");
    }
    let pot = u128::from(BOXES as u64) * u128::from(p.price);
    let fees =
        u128::from(p.platform_fee) + u128::from(p.creator_fee) + u128::from(p.integrator_fee);
    if fees > pot {
        return Err("fees exceed the pot");
    }
    if p.quarters_settled > 4 {
        return Err("quarters_settled > 4");
    }
    if p.quarters_settled >= 1 {
        if !matches!(
            p.status,
            PoolStatus::Drawn | PoolStatus::Settled | PoolStatus::Split
        ) {
            return Err("a settled quarter outside Drawn/Settled/Split");
        }
        if u128::from(p.prize_pool) != pot - fees + u128::from(p.sponsored_total) {
            return Err("prize_pool ≠ pot − fees + sponsored_total (§4.5)");
        }
        let paid: u128 = p.quarter_prize.iter().map(|q| u128::from(*q)).sum();
        if paid > u128::from(p.prize_pool)
            || u128::from(p.unpaid_prize_pool) > u128::from(p.prize_pool)
        {
            return Err("quarter prizes or unpaid_prize_pool exceed prize_pool");
        }
    }
    if p.fees_paid && p.quarters_settled == 0 {
        return Err("fees_paid before any settlement");
    }
    if popcount(p.returned) > u64::from(p.sold) {
        return Err("more boxes returned than sold");
    }
    if p.returned != 0 && !matches!(p.status, PoolStatus::Returned | PoolStatus::Split) {
        return Err("returned bits outside Returned/Split (§4.6)");
    }
    if p.split_amount != 0 && p.status != PoolStatus::Split {
        return Err("split_amount outside Split (§4.6)");
    }
    Ok(())
}

/// O4's verdict for one pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultCheck {
    /// The input satisfied §3.4 and so does the result.
    Holds,
    /// The input satisfied §3.4 and the result does not: a violation.
    Broken(Violation),
    /// The input was already off §3.4 (a planted vault or money field) or its pool broke a
    /// §3.3 relation, so the formula says nothing about what the instruction should have
    /// moved; counted, not judged.
    NotApplicable(&'static str),
}

/// O4 for every pool in the result whose vault is in the context: PROGRAM §3.4 holds after a
/// success whenever it held before. Before a pool exists (`create_pool`) the vault owes
/// nothing, so lamports already at the address count as a discrepancy and the case is not
/// applicable. The relative form ("the discrepancy is unchanged") was tried first and is wrong
/// across status changes: `sponsored_total`, say, is in the formula while a pool sells and out
/// of it once it is `Returned`, so a planted field moves the discrepancy with the program
/// having moved exactly the right amount.
pub fn o4_vaults(before: &Context, after: &Outcome) -> Vec<VaultCheck> {
    let mut out = Vec::new();
    let pools_before = find_all::<Pool>(&before.accounts);
    for (key, p) in find_all::<Pool>(&after.accounts) {
        let Some(total_after) = vault_total(&p, &after.accounts) else {
            continue;
        };
        let d_before = match pools_before.iter().find(|(k, _)| *k == key) {
            Some((_, b)) => {
                if let Err(why) = well_formed(b) {
                    out.push(VaultCheck::NotApplicable(why));
                    continue;
                }
                let Some(total_before) = vault_total(b, &before.accounts) else {
                    continue;
                };
                i128::from(total_before)
                    - i128::try_from(expected_total(b, &key, &before.accounts)).unwrap_or(i128::MAX)
            }
            None => i128::from(vault_total(&p, &before.accounts).unwrap_or(0)),
        };
        if d_before != 0 {
            out.push(VaultCheck::NotApplicable("the input vault was off §3.4"));
            continue;
        }
        let expected_after = expected_total(&p, &key, &after.accounts);
        if u128::from(total_after) != expected_after {
            out.push(VaultCheck::Broken(v(
                "O4 vault invariant",
                format!(
                    "pool {}: status {:?}: vault holds {total_after}, PROGRAM §3.4 (with the \
                     floor) says {expected_after}",
                    short(&key),
                    p.status
                ),
            )));
        } else {
            out.push(VaultCheck::Holds);
        }
    }
    out
}

/// O4 as violations only (the counting form is [`o4_vaults`]).
pub fn o4_vault(before: &Context, after: &Outcome) -> Vec<Violation> {
    o4_vaults(before, after)
        .into_iter()
        .filter_map(|c| match c {
            VaultCheck::Broken(v) => Some(v),
            _ => None,
        })
        .collect()
}

/// The own-box limit for the pool's creator: the override when its slot is initialised, else
/// the config (PROGRAM §3.6).
fn max_own_boxes(before: &Context, after: &Outcome, pool: &Pool) -> Option<u8> {
    let creator = pool.creator.to_bytes();
    for a in after.accounts.iter().chain(before.accounts.iter()) {
        if let Some(o) = decode::<WalletOverride>(a) {
            if o.wallet.to_bytes() == creator {
                return Some(o.max_own_boxes);
            }
        }
    }
    find::<PlatformConfig>(&after.accounts).map(|(_, c)| c.max_own_boxes)
}

/// O5: the `buy` state relations.
pub fn o5_buy(before: &Context, after: &Outcome) -> Vec<Violation> {
    let mut out = Vec::new();
    // `buy`'s account list: buyer, config, game, pool, … (PROGRAM §4.3).
    let Some(pool_key) = before.metas.get(3).map(|m| m.key) else {
        return vec![v("O5 buy", "no pool meta")];
    };
    let (Some(b), Some(a)) = (
        before.account(&pool_key).and_then(decode::<Pool>),
        after.account(&pool_key).and_then(decode::<Pool>),
    ) else {
        return vec![v("O5 buy", "pool account missing before or after")];
    };
    let Some(count) = before.data.get(8).copied() else {
        return vec![v("O5 buy", "no count byte")];
    };
    let Some(buyer) = before.metas.first().map(|m| m.key) else {
        return vec![v("O5 buy", "no buyer meta")];
    };
    if u32::from(a.sold) != u32::from(b.sold) + u32::from(count) {
        out.push(v("O5 sold", format!("{} + {count} ≠ {}", b.sold, a.sold)));
    }
    let mut moved = 0usize;
    for (k, (x, y)) in b.owners.iter().zip(a.owners.iter()).enumerate() {
        if x == y {
            continue;
        }
        if *x == Pubkey::default() && y.to_bytes() == buyer {
            moved += 1;
        } else {
            out.push(v(
                "O5 owners",
                format!(
                    "box {k} changed {} → {}",
                    short(&x.to_bytes()),
                    short(&y.to_bytes())
                ),
            ));
        }
    }
    if moved != usize::from(count) {
        out.push(v(
            "O5 boxes",
            format!("{moved} boxes assigned, count {count}"),
        ));
    }
    let expected_creator = if buyer == b.creator.to_bytes() {
        u32::from(b.creator_boxes) + u32::from(count)
    } else {
        u32::from(b.creator_boxes)
    };
    if u32::from(a.creator_boxes) != expected_creator {
        out.push(v(
            "O5 creator_boxes",
            format!(
                "{} → {}, expected {expected_creator}",
                b.creator_boxes, a.creator_boxes
            ),
        ));
    }
    // The cap is absolute, so it is judged only from a well-formed input (a planted
    // `creator_boxes` above the cap on a pool the creator owns nothing in is not the program's
    // doing); the relations above hold whatever the input.
    if well_formed(&b).is_ok() {
        if let Some(limit) = max_own_boxes(before, after, &a) {
            if a.creator_boxes > limit {
                out.push(v(
                    "O5 own-box cap",
                    format!("creator_boxes {} > {limit}", a.creator_boxes),
                ));
            }
        }
    }
    let locked = usize::from(a.sold) == BOXES;
    if locked != (a.status == PoolStatus::Locked) {
        out.push(v(
            "O5 lock",
            format!("sold {} with status {:?}", a.sold, a.status),
        ));
    }
    if locked && a.locked_at != before.unix_timestamp {
        out.push(v(
            "O5 locked_at",
            format!("{} ≠ now {}", a.locked_at, before.unix_timestamp),
        ));
    }
    let counter = |accounts: &[Account]| {
        accounts
            .iter()
            .filter_map(decode::<CreatorCounter>)
            .find(|c| c.creator == a.creator && c.game == a.game)
            .map(|c| c.open_count)
    };
    match (counter(&before.accounts), counter(&after.accounts)) {
        (Some(x), Some(y)) => {
            let expected = if locked { x.saturating_sub(1) } else { x };
            if y != expected {
                out.push(v(
                    "O5 counter",
                    format!(
                        "open_count {x} → {y}, expected {expected} (locked: {locked}) for pool {}",
                        short(&pool_key)
                    ),
                ));
            }
        }
        _ => out.push(v("O5 counter", "counter account missing before or after")),
    }
    out
}
