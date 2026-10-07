//! Regolith Entropy as this program reads it (PROGRAM §4.4): the `Var` layout,
//! the two keccak formulas and the `Sample` instruction, declared here rather
//! than imported (the `entropy-api` crate sits on an older `solana-program`
//! major than Anchor 1.2). Everything is taken from `regolith-labs/entropy`
//! at `f26ae03cccab6188effb0a170b8123cf4bb54c94`, the commit `verify.osec.io`
//! reports as the deployed `3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X`
//! (`api/src/state/var.rs`, `api/src/instruction.rs`, `program/src/sample.rs`,
//! `program/src/reveal.rs`); a test decodes a `Var` fetched from mainnet and
//! another checks the dumped ELF's hash against that report.
//!
//! The program never opens, reveals, rolls or closes a `Var`: it reads one and
//! CPIs `Sample`. Nothing here depends on the provider's auto mode.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use solana_keccak_hasher::hashv as keccak;
use solana_sdk_ids::sysvar;

use crate::constants::ENTROPY_PROGRAM;
use crate::errors::MybarpoolError;

/// A `Var` account is steel's 8-byte discriminator followed by the 232-byte `#[repr(C)]` struct.
pub const VAR_LEN: usize = 240;
/// steel writes `EntropyAccount::Var = 0` into the first byte and pads: all zero.
pub const VAR_DISCRIMINATOR: [u8; 8] = [0; 8];
/// PDA seed prefix: `["var", authority, id.to_le_bytes()]` under `ENTROPY_PROGRAM`. The program
/// does not derive or check it (it binds to an address); tests plant at the real PDA.
pub const VAR_SEED: &[u8] = b"var";
/// `EntropyInstruction::Sample` at `f26ae03`.
pub const SAMPLE_DISCRIMINATOR: u8 = 5;

/// `api/src/state/var.rs` at `f26ae03`. Offsets from the start of the account data, the
/// discriminator included: `authority` 8, `id` 40, `provider` 48, `commit` 80, `seed` 112,
/// `slot_hash` 144, `value` 176, `samples` 208, `is_auto` 216, `start_at` 224, `end_at` 232.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Var {
    /// The wallet that opened it; the only signer `Next` and `Close` accept.
    pub authority: Pubkey,
    /// Part of the PDA seeds.
    pub id: u64,
    /// The label the opener set; `set_var` requires `config.entropy_provider`.
    pub provider: Pubkey,
    /// `keccak(seed)`, set at `Open`; `Next` rolls `seed` into it.
    pub commit: [u8; 32],
    /// Zero until `Reveal`.
    pub seed: [u8; 32],
    /// Zero until `Sample`; the SlotHashes entry for `end_at`, or `keccak(end_at)` if gone.
    pub slot_hash: [u8; 32],
    /// Zero until `Reveal`; `keccak(slot_hash ‖ seed ‖ samples)`.
    pub value: [u8; 32],
    /// Values left in the `Var`; `Next` decrements it. A MyBarPool `Var` has exactly one.
    pub samples: u64,
    /// Provider auto mode; must be 0 for us and is never read again.
    pub is_auto: u64,
    /// Slot of `Open` / the last `Next`.
    pub start_at: u64,
    /// The slot whose hash is sampled; `Sample` needs `Clock.slot ≥ end_at`.
    pub end_at: u64,
}

impl Var {
    /// Decode a `Var` from an account: owner `ENTROPY_PROGRAM`, exactly `VAR_LEN` bytes, zero
    /// discriminator, else `VarNotEntropy` (this refuses the two legacy 232-byte accounts on
    /// mainnet by length). Fields are read at their offsets through `.get()`, never by casting.
    pub fn try_from_account(info: &AccountInfo) -> Result<Var> {
        require_keys_eq!(*info.owner, ENTROPY_PROGRAM, MybarpoolError::VarNotEntropy);
        let data = info.try_borrow_data()?;
        require!(data.len() == VAR_LEN, MybarpoolError::VarNotEntropy);
        require!(
            data.get(0..8).ok_or(MybarpoolError::VarNotEntropy)? == VAR_DISCRIMINATOR,
            MybarpoolError::VarNotEntropy
        );
        Ok(Var {
            authority: pubkey_at(&data, 8)?,
            id: u64_at(&data, 40)?,
            provider: pubkey_at(&data, 48)?,
            commit: bytes32_at(&data, 80)?,
            seed: bytes32_at(&data, 112)?,
            slot_hash: bytes32_at(&data, 144)?,
            value: bytes32_at(&data, 176)?,
            samples: u64_at(&data, 208)?,
            is_auto: u64_at(&data, 216)?,
            start_at: u64_at(&data, 224)?,
            end_at: u64_at(&data, 232)?,
        })
    }
}

fn bytes32_at(data: &[u8], at: usize) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    out.copy_from_slice(data.get(at..at + 32).ok_or(MybarpoolError::VarNotEntropy)?);
    Ok(out)
}

fn pubkey_at(data: &[u8], at: usize) -> Result<Pubkey> {
    Ok(Pubkey::new_from_array(bytes32_at(data, at)?))
}

fn u64_at(data: &[u8], at: usize) -> Result<u64> {
    let mut out = [0u8; 8];
    out.copy_from_slice(data.get(at..at + 8).ok_or(MybarpoolError::VarNotEntropy)?);
    Ok(u64::from_le_bytes(out))
}

/// `keccak(end_at.to_le_bytes())`: what `Sample` writes when `end_at` has left SlotHashes
/// (`program/src/sample.rs`). Predictable at `Open`; a `Var` carrying it is never drawn on.
pub fn fallback_hash(end_at: u64) -> [u8; 32] {
    keccak(&[&end_at.to_le_bytes()]).to_bytes()
}

/// `keccak(slot_hash ‖ seed ‖ samples.to_le_bytes())`: what `Reveal` writes as `value`
/// (`program/src/reveal.rs`). `draw` recomputes it from the `Var`'s own fields.
pub fn expected_value(slot_hash: &[u8; 32], seed: &[u8; 32], samples: u64) -> [u8; 32] {
    keccak(&[slot_hash, seed, &samples.to_le_bytes()]).to_bytes()
}

/// Entropy `Sample` (`api/src/sdk.rs`): program `ENTROPY_PROGRAM`, accounts `[signer (readonly
/// signer), var (writable), SlotHashes (readonly)]`, data `[5]`. The signer carries no other
/// privilege into the CPI.
pub fn sample_instruction(signer: &Pubkey, var: &Pubkey) -> Instruction {
    Instruction {
        program_id: ENTROPY_PROGRAM,
        accounts: vec![
            AccountMeta::new_readonly(*signer, true),
            AccountMeta::new(*var, false),
            AccountMeta::new_readonly(sysvar::slot_hashes::ID, false),
        ],
        data: vec![SAMPLE_DISCRIMINATOR],
    }
}
