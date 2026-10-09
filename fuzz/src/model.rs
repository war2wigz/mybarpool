//! A plain picture of one instruction's inputs and effects, independent of Mollusk's types so
//! the mutations and the oracle can be built and tested without it.
//!
//! `missing_mut_constraint` (anchor-lints `d8116dd`) reads every access to a field of an Anchor
//! `#[account]` struct as a write to an account of the instruction being analysed; this
//! crate has no instructions and no accounts, it decodes the program's state to judge it, so
//! the lint is allowed for the module (listed in `AUDIT-READINESS.md`).
#![cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]

/// A 32-byte key.
pub type Key = [u8; 32];

/// An account as the runtime sees it before or after the instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub key: Key,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: Key,
    pub executable: bool,
}

/// One entry of the instruction's account list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub key: Key,
    pub is_signer: bool,
    pub is_writable: bool,
}

/// Everything the instruction runs against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub program_id: Key,
    pub metas: Vec<Meta>,
    pub data: Vec<u8>,
    pub accounts: Vec<Account>,
    /// The `Clock` sysvar's `unix_timestamp`.
    pub unix_timestamp: i64,
    pub compute_unit_limit: u64,
}

impl Context {
    /// The account at `key`, if the context carries it.
    pub fn account(&self, key: &Key) -> Option<&Account> {
        self.accounts.iter().find(|a| a.key == *key)
    }
}

/// How the instruction ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Result {
    Success,
    /// `InstructionError::Custom(code)`.
    Custom(u32),
    /// A runtime check the runtime itself raised (`MissingRequiredSignature`,
    /// `NotEnoughAccountKeys`, `InvalidAccountData`, …), by name.
    Runtime(String),
    /// A panic, abort or compute exhaustion: the program did not finish.
    FailedToComplete,
}

/// What the instruction left behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub result: Result,
    pub accounts: Vec<Account>,
    pub compute_units: u64,
}

impl Outcome {
    /// The resulting account at `key`.
    pub fn account(&self, key: &Key) -> Option<&Account> {
        self.accounts.iter().find(|a| a.key == *key)
    }
}
