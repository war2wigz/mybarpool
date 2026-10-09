//! Instruction names and grouping by Anchor discriminator (the first eight bytes of the data).

use std::collections::BTreeMap;

use anchor_lang::Discriminator;

use crate::model::Context;

/// Every instruction of the program with its discriminator, from the crate the IDL is built
/// from, so the harness and the committed IDL cannot disagree.
pub fn instruction_names() -> BTreeMap<[u8; 8], &'static str> {
    use mybarpool::instruction as ix;
    let mut m = BTreeMap::new();
    macro_rules! name {
        ($($t:ident => $n:literal),* $(,)?) => { $( m.insert(<ix::$t as Discriminator>::DISCRIMINATOR.try_into().expect("8 bytes"), $n); )* };
    }
    name!(
        Initialize => "initialize", UpdateConfig => "update_config",
        SetWalletOverride => "set_wallet_override", CloseWalletOverride => "close_wallet_override",
        CreateGame => "create_game", UpdateKickoff => "update_kickoff", PostScores => "post_scores",
        MarkGame => "mark_game", CreatePool => "create_pool", Buy => "buy", Sponsor => "sponsor",
        RotateGateKey => "rotate_gate_key", CloseCounter => "close_counter", SetVar => "set_var",
        SampleVar => "sample_var", Draw => "draw", ReplaceVar => "replace_var", Settle => "settle",
        ClosePool => "close_pool", ReturnBoxes => "return_boxes",
        ReturnSponsorship => "return_sponsorship", CancelPool => "cancel_pool", Split => "split",
        Reclaim => "reclaim", ReclaimSponsorship => "reclaim_sponsorship",
        CloseSponsorship => "close_sponsorship",
    );
    m
}

/// The discriminator of a context's instruction, or `None` for a short payload.
pub fn discriminator(ctx: &Context) -> Option<[u8; 8]> {
    ctx.data.get(..8).map(|d| d.try_into().expect("8 bytes"))
}

/// The instruction's name when the context targets this program with a known discriminator.
pub fn instruction_name(ctx: &Context) -> Option<&'static str> {
    if ctx.program_id != mybarpool::ID.to_bytes() {
        return None;
    }
    discriminator(ctx).and_then(|d| instruction_names().get(&d).copied())
}

/// Contexts grouped by instruction name; contexts of other programs or with unknown data are
/// under `None` (chained fixtures carry system transfers and Entropy's own instructions).
pub fn group<'a, T>(
    items: &'a [T],
    ctx: impl Fn(&'a T) -> &'a Context,
) -> BTreeMap<Option<&'static str>, Vec<&'a T>> {
    let mut out: BTreeMap<Option<&'static str>, Vec<&T>> = BTreeMap::new();
    for item in items {
        out.entry(instruction_name(ctx(item)))
            .or_default()
            .push(item);
    }
    out
}
