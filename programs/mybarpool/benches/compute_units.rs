//! Compute-unit table (ARCHITECTURE › Environments, build plan Step 0).
//!
//! `cargo bench -p mybarpool` after `anchor build --arch v3` rewrites
//! `programs/mybarpool/compute_units.md`, which is committed. CI regenerates
//! it and shows the diff as a review item; a changed row is not a failure.
//! Later steps add one row per instruction; the DESIGN §10.4 headroom rule
//! (+20% and +30k units) is checked against this table.

use mollusk_svm::Mollusk;
use mollusk_svm_bencher::MolluskComputeUnitBencher;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

fn main() {
    let program_id = Pubkey::new_from_array(mybarpool::ID.to_bytes());
    let mollusk = Mollusk::new(&program_id, "mybarpool");

    // No instructions exist yet; this row measures Anchor's dispatch cost
    // on an unknown discriminator and is replaced in Step 2.
    let unknown = Instruction::new_with_bytes(program_id, &[0u8; 8], Vec::<AccountMeta>::new());

    MolluskComputeUnitBencher::new(mollusk)
        .bench(("dispatch_unknown_instruction", &unknown, &[]))
        .must_pass(false)
        .out_dir(env!("CARGO_MANIFEST_DIR"))
        .execute();
}
