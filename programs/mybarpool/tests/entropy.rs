//! Entropy as deployed (PROGRAM §4.4; BUILD-PLAN Step 5 first task): the
//! dumped ELF's hash against the `verify.osec.io` report, the `Var` decoder
//! against the live ORE `Var` fetched from mainnet, and the two keccak
//! formulas against vectors the TypeScript side shares.

mod common;

use anchor_lang::prelude::Pubkey as APubkey;
use anchor_lang::solana_program::account_info::AccountInfo;
use common::*;
use mybarpool::entropy::{
    expected_value, fallback_hash, sample_instruction, Var, SAMPLE_DISCRIMINATOR,
    VAR_DISCRIMINATOR, VAR_LEN,
};
use mybarpool::MybarpoolError as E;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn code_of(error: anchor_lang::error::Error) -> u32 {
    match error {
        anchor_lang::error::Error::AnchorError(e) => e.error_code_number,
        other => panic!("not an AnchorError: {other:?}"),
    }
}

/// Decode `data` under `owner` as the program would see the account.
fn decode(owner: &APubkey, data: &mut [u8]) -> anchor_lang::Result<Var> {
    let key = APubkey::new_unique();
    let mut lamports = 1u64;
    let info = AccountInfo::new(&key, false, false, &mut lamports, data, owner, false);
    Var::try_from_account(&info)
}

#[test]
fn the_elf_fixture_is_the_verified_deployed_bytecode() {
    // verify.osec.io/status/3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X, fetched 2026-10-07:
    // repo regolith-labs/entropy, commit f26ae03c…, on_chain_hash == executable_hash below.
    assert_eq!(ENTROPY_ELF.len(), 98_929);
    // The section-header table runs to byte 98,944; the fixture's last 15 bytes of it are zero
    // and were stripped with the padding, which is why `with_entropy` pads before loading.
    let shoff = u64::from_le_bytes(ENTROPY_ELF[0x28..0x30].try_into().unwrap());
    let shentsize = u16::from_le_bytes(ENTROPY_ELF[0x3a..0x3c].try_into().unwrap());
    let shnum = u16::from_le_bytes(ENTROPY_ELF[0x3c..0x3e].try_into().unwrap());
    assert_eq!(
        shoff + u64::from(shentsize) * u64::from(shnum),
        ENTROPY_ELF_LOADABLE_LEN as u64
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hashv(&[ENTROPY_ELF]).to_bytes()),
        "b64ffdfef7bb05839fbe0d24196697cc20de6bc72081031bc0523699181b18b1"
    );
}

#[test]
fn the_live_ore_var_decodes_against_the_declared_layout() {
    // BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E at slot 454,331,258 (NOTES, first task).
    assert_eq!(LIVE_VAR.len(), VAR_LEN);
    let mut data = LIVE_VAR.to_vec();
    let v = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data).expect("decodes");
    assert_eq!(
        v.authority.to_string(),
        "BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi" // the ORE v3 Board account
    );
    assert_eq!(v.id, 0);
    assert_eq!(
        v.provider.to_string(),
        "AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF" // config.entropy_provider
    );
    assert_eq!(v.is_auto, 0);
    assert!(v.samples > 0);
    assert!(v.end_at > v.start_at);
    // As fetched: rolled by Next, committed, waiting for its window.
    assert_ne!(v.commit, [0u8; 32]);
    assert_eq!(v.seed, [0u8; 32]);
    assert_eq!(v.slot_hash, [0u8; 32]);
    assert_eq!(v.value, [0u8; 32]);
}

#[test]
fn the_decoder_refuses_a_wrong_owner_the_legacy_length_and_a_nonzero_discriminator() {
    let mut data = LIVE_VAR.to_vec();
    let err = decode(&anchor_lang::system_program::ID, &mut data).unwrap_err();
    assert_eq!(code_of(err), err_code(E::VarNotEntropy));

    // The two legacy mainnet accounts (21eRdeK7…, 2hhpsEkR…) are 232 bytes: no discriminator.
    let mut legacy = LIVE_VAR[8..].to_vec();
    assert_eq!(legacy.len(), 232);
    let err = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut legacy).unwrap_err();
    assert_eq!(code_of(err), err_code(E::VarNotEntropy));

    let mut bad = LIVE_VAR.to_vec();
    bad[0] = 1;
    let err = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut bad).unwrap_err();
    assert_eq!(code_of(err), err_code(E::VarNotEntropy));
    assert_eq!(VAR_DISCRIMINATOR, [0u8; 8]);
}

#[test]
fn the_planter_and_the_decoder_agree_on_every_offset() {
    let f = Fixture::new();
    let fields = f.revealed_var(END_AT, END_HASH, &SEED);
    let mut data = var_bytes(&fields).to_vec();
    let v = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data).expect("decodes");
    assert_eq!(v.authority, to_a(&fields.authority));
    assert_eq!(v.id, fields.id);
    assert_eq!(v.provider, to_a(&fields.provider));
    assert_eq!(v.commit, fields.commit);
    assert_eq!(v.seed, fields.seed);
    assert_eq!(v.slot_hash, fields.slot_hash);
    assert_eq!(v.value, fields.value);
    assert_eq!(
        (v.samples, v.is_auto, v.start_at, v.end_at),
        (1, 0, fields.start_at, END_AT)
    );
}

#[test]
fn keccak_formulas_match_the_vectors_the_typescript_side_shares() {
    // packages/shared/test/entropy.test.ts carries the same three vectors (pycryptodome).
    assert_eq!(
        hex(&solana_keccak_hasher::hashv(&[&[]]).to_bytes()),
        "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    );
    assert_eq!(
        hex(&fallback_hash(1000)),
        "21c9650f3e9e5e2b94468fe9f1a4351613d2f305993af006c012c30ea53117e9"
    );
    assert_eq!(
        hex(&expected_value(&[0x11; 32], &[0x22; 32], 1)),
        "afb7ecc0aa543bf6cd7d2deac8d34b0476b669b383df70b55622236745e78744"
    );
    assert_ne!(
        expected_value(&[0x11; 32], &[0x22; 32], 2),
        expected_value(&[0x11; 32], &[0x22; 32], 1)
    );
    // And against the hasher directly, on the standard fixture values.
    let mut concat = Vec::new();
    concat.extend_from_slice(&END_HASH);
    concat.extend_from_slice(&SEED);
    concat.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(
        standard_value(),
        solana_keccak_hasher::hashv(&[&concat]).to_bytes()
    );
}

#[test]
fn the_sample_instruction_is_entropys_with_a_read_only_signer() {
    // api/src/sdk.rs at f26ae03: [signer (s), var (w), SlotHashes]; data [5].
    let signer = APubkey::new_unique();
    let var = APubkey::new_unique();
    let ix = sample_instruction(&signer, &var);
    assert_eq!(ix.program_id, mybarpool::constants::ENTROPY_PROGRAM);
    assert_eq!(ix.data, vec![SAMPLE_DISCRIMINATOR]);
    assert_eq!(ix.data, vec![5]);
    assert_eq!(ix.accounts.len(), 3);
    assert_eq!(
        (
            ix.accounts[0].pubkey,
            ix.accounts[0].is_signer,
            ix.accounts[0].is_writable
        ),
        (signer, true, false)
    );
    assert_eq!(
        (
            ix.accounts[1].pubkey,
            ix.accounts[1].is_signer,
            ix.accounts[1].is_writable
        ),
        (var, false, true)
    );
    assert_eq!(
        (
            ix.accounts[2].pubkey,
            ix.accounts[2].is_signer,
            ix.accounts[2].is_writable
        ),
        (solana_sdk_ids::sysvar::slot_hashes::ID, false, false)
    );
}

#[test]
fn mollusk_loads_the_entropy_elf_and_runs_sample_against_a_planted_var() {
    // The real bytecode writes the SlotHashes entry for end_at into a planted, fresh Var.
    let f = Fixture::new();
    let mut m = mollusk_for_draw(END_AT + 3, &standard_window());
    with_entropy(&mut m);
    let var = f.var_key();
    let accounts = vec![
        (f.stranger, system_account(LAMPORTS_PER_SOL)),
        (var, var_account(&f.fresh_var(END_AT))),
        m.sysvars.keyed_account_for_slot_hashes_sysvar(),
    ];
    let ix = {
        let a = sample_instruction(&to_a(&f.stranger), &to_a(&var));
        solana_instruction::Instruction::new_with_bytes(
            to_m(&a.program_id),
            &a.data,
            a.accounts
                .iter()
                .map(|m| solana_instruction::AccountMeta {
                    pubkey: to_m(&m.pubkey),
                    is_signer: m.is_signer,
                    is_writable: m.is_writable,
                })
                .collect(),
        )
    };
    let result = m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[mollusk_svm::result::Check::success()],
    );
    let after = account_of(&result, &var);
    assert_eq!(&after.data[144..176], &END_HASH);
    println!(
        "Entropy Sample under Mollusk: {} CU",
        result.compute_units_consumed
    );
}

/// Anchor's custom code offset + the variant index.
fn err_code(e: E) -> u32 {
    err(e)
}
