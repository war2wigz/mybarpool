//! MyBarPool: NFL boxes on Solana.
//!
//! Step 0 of the build plan: an empty program that compiles, builds as
//! SBPFv3 and runs under Mollusk and Surfpool. Accounts and instructions
//! arrive in later steps, each one built and audited against
//! `docs/PROGRAM.md`.
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

declare_id!("3jk4YM9xnpoMYK3nuaUHZ59SCDxwExfT9EHxwP2wRMBw");

#[program]
pub mod mybarpool {}
