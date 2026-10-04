//! Mollusk unit tests (ARCHITECTURE › Environments). Step 0 proves the ELF
//! built by `anchor build --arch v3` loads and dispatches under Anza's SVM
//! harness; every later step adds instruction tests here.
//!
//! Run after a build: `cargo test -p mybarpool`. `SBF_OUT_DIR` is set in
//! `.cargo/config.toml` so Mollusk finds `target/deploy/mybarpool.so`.

use mollusk_svm::{result::Check, Mollusk};
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;

fn program_id() -> Pubkey {
    Pubkey::new_from_array(mybarpool::ID.to_bytes())
}

/// Anchor's `InstructionFallbackNotFound` (error code 101): an all-zero
/// discriminator matches no instruction. Reaching this error means the loader
/// accepted the SBPFv3 ELF and Anchor's dispatcher ran.
const INSTRUCTION_FALLBACK_NOT_FOUND: u32 = 101;

#[test]
fn program_loads_and_rejects_unknown_instruction() {
    let mollusk = Mollusk::new(&program_id(), "mybarpool");
    let instruction =
        Instruction::new_with_bytes(program_id(), &[0u8; 8], Vec::<AccountMeta>::new());
    mollusk.process_and_validate_instruction(
        &instruction,
        &[],
        &[Check::err(ProgramError::Custom(
            INSTRUCTION_FALLBACK_NOT_FOUND,
        ))],
    );
}
