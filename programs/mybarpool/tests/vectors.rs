//! Cross-language checks against the shared vector files (PROGRAM §6
//! "Reference implementations live in `packages/shared` and are cross-tested
//! against the program with shared vector files"). The files are generated
//! by `packages/shared/test/vectors.test.ts` and committed; the program must
//! reproduce them byte for byte. Run after `anchor build --arch v3`.

mod common;

use std::path::PathBuf;

use anchor_lang::prelude::Pubkey as APubkey;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::{
    assignment::assign_boxes,
    money::{dust, fee_amounts, prize_pool, quarter_prizes},
    winner::winning_box,
    BoxesBought, CreatePoolParams, PayoutPreset, PoolStatus,
};
use serde::Deserialize;
use solana_pubkey::Pubkey;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared/src/vectors")
}

fn load<T: for<'de> Deserialize<'de>>(file: &str) -> T {
    let path = vectors_dir().join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn hex32(s: &str) -> [u8; 32] {
    let bytes = hex(s);
    bytes.try_into().expect("32 bytes")
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn u64s(s: &str) -> u64 {
    s.parse().expect("u64 decimal string")
}

#[derive(Deserialize)]
struct File<E> {
    entries: Vec<E>,
}

// ---------------------------------------------------------------------------
// assignment.json (24 rich entries) and assignment-bulk.json (1,000)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentEntry {
    slothash: String,
    buyer: String,
    sold: u8,
    count: u8,
    owners: Vec<String>,
    boxes: Vec<u8>,
    owners_after: Vec<String>,
}

#[test]
fn assignment_json_boxes_and_owners_after_match_for_every_entry() {
    let file: File<AssignmentEntry> = load("assignment.json");
    assert_eq!(file.entries.len(), 24);
    for (i, e) in file.entries.iter().enumerate() {
        let mut owners: [APubkey; 25] = e
            .owners
            .iter()
            .map(|h| APubkey::new_from_array(hex32(h)))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let boxes = assign_boxes(
            &hex32(&e.slothash),
            &APubkey::new_from_array(hex32(&e.buyer)),
            e.sold,
            e.count,
            &mut owners,
        )
        .unwrap_or_else(|err| panic!("entry {i}: {err}"));
        assert_eq!(boxes, e.boxes, "entry {i} boxes");
        let after: Vec<APubkey> = e
            .owners_after
            .iter()
            .map(|h| APubkey::new_from_array(hex32(h)))
            .collect();
        assert_eq!(owners.to_vec(), after, "entry {i} owners after");
    }
}

#[derive(Deserialize)]
struct BulkEntry {
    slothash: String,
    buyer: String,
    sold: u8,
    count: u8,
    owned: Vec<u8>,
    boxes: Vec<u8>,
}

fn planted_owners(owned: &[u8]) -> [APubkey; 25] {
    let mut owners = [APubkey::default(); 25];
    for b in owned {
        owners[usize::from(*b)] = to_a(&Pubkey::new_unique());
    }
    owners
}

#[test]
fn assignment_bulk_json_matches_for_all_1000_entries_and_every_50th_as_a_real_buy() {
    // Step 4 acceptance: "matches the shared reference for 1,000 random vectors".
    let file: File<BulkEntry> = load("assignment-bulk.json");
    assert_eq!(file.entries.len(), 1_000);
    let f = Fixture::new();
    for (i, e) in file.entries.iter().enumerate() {
        let slothash = hex32(&e.slothash);
        let buyer = APubkey::new_from_array(hex32(&e.buyer));
        let mut owners = planted_owners(&e.owned);
        let boxes = assign_boxes(&slothash, &buyer, e.sold, e.count, &mut owners)
            .unwrap_or_else(|err| panic!("entry {i}: {err}"));
        assert_eq!(boxes, e.boxes, "entry {i}");

        if i % 50 == 0 {
            // The instruction, with the owners planted, `sold` as given and the slot hash set.
            let m = mollusk_for_pools(T0, slothash);
            let mut pool = fresh_pool(&f, &f.expected_config(), &sol_params(0));
            pool.owners = planted_owners(&e.owned);
            pool.sold = e.sold;
            let buyer_m = to_m(&buyer);
            let mut accounts = pool_accounts(
                &f,
                &m,
                &f.expected_config(),
                &pool,
                rent_for(0) + u64::from(e.sold) * PRICE,
                1,
            );
            accounts.push((buyer_m, system_account(100 * LAMPORTS_PER_SOL)));
            let result = m.process_and_validate_instruction(
                &buy_ix(&buyer_m, &pool, e.count, TokenPath::default()),
                &accounts,
                &[Check::success()],
            );
            let event: BoxesBought = emitted_event(&result).expect("BoxesBought");
            assert_eq!(event.boxes, e.boxes, "entry {i} as a buy");
            let after = decode_pool(account_of(
                &result,
                &pool_pda(&standard_game(), &f.creator, NONCE).0,
            ));
            for b in &e.boxes {
                assert_eq!(after.owners[usize::from(*b)], buyer);
            }
            assert_eq!(after.sold, e.sold + e.count);
            assert_eq!(
                after.status,
                if after.sold == 25 {
                    PoolStatus::Locked
                } else {
                    PoolStatus::Open
                }
            );
        }
    }
}

// ---------------------------------------------------------------------------
// fees.json (24) and fees-bulk.json (200)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeeEntry {
    #[serde(default)]
    token: Option<String>,
    price: String,
    platform_bps: u16,
    creator_bps: u16,
    creator_addon_bps: u16,
    integrator_bps: u16,
    platform_fee: String,
    creator_fee: String,
    integrator_fee: String,
    // Step 6: the §5.2 fields fees.json has carried since Step 1.
    #[serde(default)]
    sponsored_total: Option<String>,
    #[serde(default)]
    preset: Option<u8>,
    #[serde(default)]
    prize_pool: Option<String>,
    #[serde(default)]
    quarters: Option<[String; 4]>,
    #[serde(default)]
    dust: Option<String>,
}

fn check_fee_file(file: &str, expected_len: usize) -> Vec<FeeEntry> {
    let file: File<FeeEntry> = load(file);
    assert_eq!(file.entries.len(), expected_len);
    for (i, e) in file.entries.iter().enumerate() {
        let fees = fee_amounts(
            u64s(&e.price),
            e.platform_bps,
            e.creator_bps,
            e.creator_addon_bps,
            e.integrator_bps,
        )
        .unwrap_or_else(|err| panic!("entry {i}: {err}"));
        assert_eq!(
            (fees.platform_fee, fees.creator_fee, fees.integrator_fee),
            (
                u64s(&e.platform_fee),
                u64s(&e.creator_fee),
                u64s(&e.integrator_fee)
            ),
            "entry {i}"
        );
    }
    file.entries
}

#[test]
fn fees_bulk_json_matches_for_all_200_entries() {
    check_fee_file("fees-bulk.json", 200);
}

#[test]
fn fees_json_prize_pool_quarters_and_dust_match_for_every_entry() {
    // PROGRAM §5.2 "≤ 3 base units on any preset": every entry's prizePool,
    // quarters and dust from the shared package equal the program's functions, and dust ≤ 3.
    let entries = check_fee_file("fees.json", 24);
    for (i, e) in entries.iter().enumerate() {
        let fees = fee_amounts(
            u64s(&e.price),
            e.platform_bps,
            e.creator_bps,
            e.creator_addon_bps,
            e.integrator_bps,
        )
        .unwrap();
        let sponsored = u64s(e.sponsored_total.as_deref().expect("sponsoredTotal"));
        let preset = PayoutPreset::try_from(e.preset.expect("preset")).unwrap();
        let pool = prize_pool(u64s(&e.price), &fees, sponsored)
            .unwrap_or_else(|err| panic!("entry {i}: {err}"));
        assert_eq!(
            pool,
            u64s(e.prize_pool.as_deref().expect("prizePool")),
            "entry {i}"
        );
        let quarters = quarter_prizes(pool, preset).unwrap();
        let expected = e.quarters.as_ref().expect("quarters");
        assert_eq!(
            quarters,
            [
                u64s(&expected[0]),
                u64s(&expected[1]),
                u64s(&expected[2]),
                u64s(&expected[3])
            ],
            "entry {i}"
        );
        let d = dust(pool, &quarters).unwrap();
        assert_eq!(d, u64s(e.dust.as_deref().expect("dust")), "entry {i}");
        assert!(d <= 3, "entry {i}: dust {d}");
    }
}

// ---------------------------------------------------------------------------
// winner.json (24) and winner-bulk.json (1,000)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WinnerEntry {
    home_axis: [u8; 10],
    away_axis: [u8; 10],
    home: u16,
    away: u16,
    r#box: u8,
}

fn check_winner_file(file: &str, expected_len: usize) {
    // PROGRAM §6.3; Step 1 vectors.
    let file: File<WinnerEntry> = load(file);
    assert_eq!(file.entries.len(), expected_len);
    for (i, e) in file.entries.iter().enumerate() {
        assert_eq!(
            winning_box(e.home, e.away, &e.home_axis, &e.away_axis).unwrap(),
            e.r#box,
            "entry {i}"
        );
    }
}

#[test]
fn winner_json_matches_for_all_24_entries() {
    check_winner_file("winner.json", 24);
}

#[test]
fn winner_bulk_json_matches_for_all_1000_entries() {
    check_winner_file("winner-bulk.json", 1_000);
}

#[test]
fn fees_json_matches_and_every_entry_stores_the_same_fees_through_create_pool() {
    // PROGRAM §5.1; Step 4 acceptance "fee amounts stored equal the shared-package math".
    // SOL and ORE entries run on those rules; SKR@6 entries on the Token-2022 rule.
    let entries = check_fee_file("fees.json", 24);
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    for (i, e) in entries.iter().enumerate() {
        let token = e.token.as_deref().expect("fees.json entries name a token");
        let mut config = f.config_with_skr();
        config.platform_bps = e.platform_bps;
        config.creator_bps = e.creator_bps;
        // Keep the §3.1 invariant platform + creator + addon_budget ≤ 1500 satisfied by the
        // fixture config (500 + 500 + 500); entries have each bps ≤ 500 so the sum holds.
        let (index, path) = match token {
            "SOL" => (0u8, TokenPath::default()),
            "SKR@6" => (
                1,
                TokenPath {
                    mint: Some(f.skr_mint),
                    token_account: None,
                    token_program: Some(token_2022_program_id()),
                },
            ),
            "ORE" => (
                2,
                TokenPath {
                    mint: Some(f.ore_mint),
                    token_account: None,
                    token_program: Some(token_program_id()),
                },
            ),
            other => panic!("entry {i}: unknown token {other}"),
        };
        let params = CreatePoolParams {
            token: index,
            price: u64s(&e.price),
            creator_addon_bps: e.creator_addon_bps,
            integrator: if e.integrator_bps == 0 {
                APubkey::default()
            } else {
                to_a(&f.integrator)
            },
            integrator_bps: e.integrator_bps,
            ..sol_params(0)
        };
        let result = m.process_and_validate_instruction(
            &create_pool_ix(&f.creator, &standard_game(), params, path),
            &pool_base(&f, &m, &config),
            &[Check::success()],
        );
        let pool = decode_pool(account_of(
            &result,
            &pool_pda(&standard_game(), &f.creator, NONCE).0,
        ));
        assert_eq!(
            (pool.platform_fee, pool.creator_fee, pool.integrator_fee),
            (
                u64s(&e.platform_fee),
                u64s(&e.creator_fee),
                u64s(&e.integrator_fee)
            ),
            "entry {i} ({token}) through create_pool"
        );
    }
}

// ---------------------------------------------------------------------------
// PROGRAM §6.4 allowlist (Step 8): `allowlist.json`
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllowlistProofEntry {
    wallet: String,
    proof: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllowlistEntry {
    wallets: Vec<String>,
    root: String,
    proofs: Vec<AllowlistProofEntry>,
    non_members: Vec<String>,
}

#[derive(Deserialize)]
struct AllowlistFile {
    entries: Vec<AllowlistEntry>,
}

#[test]
fn allowlist_single_wallet_root_is_the_leaf_and_the_proof_is_empty() {
    // PROGRAM §6.4: leaf(w) = sha256(0x00 || w); one wallet → root = leaf(w); proof = [].
    // Asserted by hand before the generated vectors, in both languages.
    use mybarpool::allowlist::{leaf, node, verify, MAX_PROOF_LEN};
    let w = APubkey::new_from_array([7u8; 32]);
    let mut pre = vec![0x00u8];
    pre.extend_from_slice(&[7u8; 32]);
    let expected = solana_sha256_hasher::hashv(&[&pre]).to_bytes();
    assert_eq!(leaf(&w), expected);
    assert!(verify(&expected, &w, &[]));
    assert!(!verify(&expected, &APubkey::new_from_array([8u8; 32]), &[]));
    // node(a, b) = sha256(0x01 || min || max): symmetric.
    let (a, b) = ([1u8; 32], [2u8; 32]);
    let mut pre = vec![0x01u8];
    pre.extend_from_slice(&a);
    pre.extend_from_slice(&b);
    assert_eq!(
        node(&a, &b),
        solana_sha256_hasher::hashv(&[&pre]).to_bytes()
    );
    assert_eq!(node(&b, &a), node(&a, &b));
    assert_eq!(MAX_PROOF_LEN, 32);
}

#[test]
fn allowlist_json_every_proof_verifies_and_every_non_member_fails() {
    // PROGRAM §6.4, cross-tested: the TypeScript builder's roots and proofs verify in the
    // program's `verify`, and the Rust fold over the first list's leaves equals its root.
    use mybarpool::allowlist::verify;
    let file: AllowlistFile = load("allowlist.json");
    assert_eq!(
        file.entries
            .iter()
            .map(|e| e.wallets.len())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 8, 25, 100, 1_000]
    );
    for entry in &file.entries {
        let root = hex32(&entry.root);
        let n = entry.wallets.len();
        assert_eq!(entry.proofs.len(), if n <= 25 { n } else { 10 }, "list {n}");
        for p in &entry.proofs {
            let wallet = APubkey::new_from_array(hex32(&p.wallet));
            let proof: Vec<[u8; 32]> = p.proof.iter().map(|h| hex32(h)).collect();
            assert!(verify(&root, &wallet, &proof), "list {n}: {}", p.wallet);
            // A wrong wallet with the right proof fails.
            let other = APubkey::new_from_array(hex32(&entry.non_members[0]));
            assert!(!verify(&root, &other, &proof), "list {n}: non-member");
        }
        assert_eq!(entry.non_members.len(), 3);
        for nm in &entry.non_members {
            let wallet = APubkey::new_from_array(hex32(nm));
            // With any member's proof, and with an empty proof.
            let any = &entry.proofs[0];
            let proof: Vec<[u8; 32]> = any.proof.iter().map(|h| hex32(h)).collect();
            assert!(!verify(&root, &wallet, &proof), "list {n}: {nm}");
            assert!(!verify(&root, &wallet, &[]), "list {n}: {nm} empty");
        }
        // The host-side fold (tests/common `tree`) over the list equals the recorded root.
        let wallets: Vec<Pubkey> = entry
            .wallets
            .iter()
            .map(|h| Pubkey::new_from_array(hex32(h)))
            .collect();
        assert_eq!(tree::root(&wallets), root, "list {n}: root");
        for p in &entry.proofs {
            let w = Pubkey::new_from_array(hex32(&p.wallet));
            let proof: Vec<[u8; 32]> = p.proof.iter().map(|h| hex32(h)).collect();
            assert_eq!(tree::proof(&wallets, &w), proof, "list {n}: proof shape");
        }
    }
}

#[test]
fn allowlist_json_members_buy_through_the_program() {
    // PROGRAM §4.3 with the 8-wallet vector list planted as a pool's root: every member buys one
    // box with the file's proof; every non-member is AllowlistProofInvalid.
    let file: AllowlistFile = load("allowlist.json");
    let entry = &file.entries[5];
    assert_eq!(entry.wallets.len(), 8);
    let wallets: Vec<Pubkey> = entry
        .wallets
        .iter()
        .map(|h| Pubkey::new_from_array(hex32(h)))
        .collect();
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut pool = fresh_pool(&f, &f.expected_config(), &sol_params(0));
    pool.access_type = mybarpool::AccessType::Allowlist;
    pool.allowlist_root = hex32(&entry.root);
    let accounts = pool_accounts_with_wallets(&f, &m, &pool, rent_for(0), 1, &wallets);
    for p in &entry.proofs {
        let w = Pubkey::new_from_array(hex32(&p.wallet));
        let proof: Vec<[u8; 32]> = p.proof.iter().map(|h| hex32(h)).collect();
        m.process_and_validate_instruction(
            &buy_ix_gated(&w, &pool, 1, TokenPath::default(), None, proof),
            &accounts,
            &[Check::success()],
        );
    }
    let stranger = Pubkey::new_from_array(hex32(&entry.non_members[1]));
    let mut with_stranger = accounts.clone();
    with_stranger.push((stranger, system_account(LAMPORTS_PER_SOL)));
    let proof: Vec<[u8; 32]> = entry.proofs[2].proof.iter().map(|h| hex32(h)).collect();
    m.process_and_validate_instruction(
        &buy_ix_gated(&stranger, &pool, 1, TokenPath::default(), None, proof),
        &with_stranger,
        &[Check::err(solana_program_error::ProgramError::Custom(err(
            mybarpool::MybarpoolError::AllowlistProofInvalid,
        )))],
    );
}
