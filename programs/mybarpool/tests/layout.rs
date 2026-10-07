//! Account layouts, PROGRAM §3 preamble, §3.1–§3.3, §3.5–§3.7. Every field holds a
//! distinct sentinel; each is asserted at the offset the brief's table gives.

mod common;

use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountSerialize, Discriminator};
use mybarpool::constants::*;
use mybarpool::{
    AccessType, CreatorCounter, GameKey, GameRecord, GameStatus, PlatformConfig, Pool, PoolStatus,
    Sponsorship, TokenRule, WalletOverride,
};

fn key(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

fn rule(seed: u8) -> TokenRule {
    TokenRule {
        enabled: seed % 2 == 1,
        mint: key(seed),
        token_program: key(seed.wrapping_add(1)),
        decimals: seed.wrapping_add(2),
        min_price: 0x0101_0101_0101_0100 | u64::from(seed),
        step: 0x0202_0202_0202_0200 | u64::from(seed),
        max_price: 0x0303_0303_0303_0300 | u64::from(seed),
        max_sponsorship: 0x0404_0404_0404_0400 | u64::from(seed),
    }
}

fn serialize<T: AccountSerialize>(value: &T) -> Vec<u8> {
    let mut data = Vec::new();
    value.try_serialize(&mut data).expect("serialise");
    data
}

#[test]
fn token_rule_is_98_bytes() {
    assert_eq!(TokenRule::SIZE, 98); // 1 + 32 + 32 + 1 + 4 × 8
    let mut data = Vec::new();
    anchor_lang::AnchorSerialize::serialize(&rule(7), &mut data).unwrap();
    assert_eq!(data.len(), 98);
    assert_eq!(data[0], 1); // enabled
    assert_eq!(&data[1..33], &[7u8; 32]); // mint
    assert_eq!(&data[33..65], &[8u8; 32]); // token_program
    assert_eq!(data[65], 9); // decimals
    assert_eq!(&data[66..74], &(0x0101_0101_0101_0107u64).to_le_bytes()); // min_price
    assert_eq!(&data[74..82], &(0x0202_0202_0202_0207u64).to_le_bytes()); // step
    assert_eq!(&data[82..90], &(0x0303_0303_0303_0307u64).to_le_bytes()); // max_price
    assert_eq!(&data[90..98], &(0x0404_0404_0404_0407u64).to_le_bytes()); // max_sponsorship
}

#[test]
fn platform_config_is_698_bytes_at_the_documented_offsets() {
    assert_eq!(PlatformConfig::SIZE, 698); // 8 + 690
    let config = PlatformConfig {
        admin: key(0xA1),
        score_authority: key(0xA2),
        entropy_provider: key(0xA3),
        fee_wallet: key(0xA4),
        platform_bps: 0x1111,
        creator_bps: 0x2222,
        addon_budget_bps: 0x3333,
        default_preset: 0x44,
        max_open_pools: 0x55,
        max_own_boxes: 0x66,
        preseason_enabled: true,
        paused: false,
        tokens: [rule(0x10), rule(0x20), rule(0x30)],
        bump: 0xBB,
        reserved: [0xEE; 256],
    };
    let data = serialize(&config);
    assert_eq!(data.len(), 698);
    assert_eq!(&data[0..8], PlatformConfig::DISCRIMINATOR);
    assert_eq!(&data[8..40], &[0xA1u8; 32]); // admin
    assert_eq!(&data[40..72], &[0xA2u8; 32]); // score_authority
    assert_eq!(&data[72..104], &[0xA3u8; 32]); // entropy_provider
    assert_eq!(&data[104..136], &[0xA4u8; 32]); // fee_wallet
    assert_eq!(&data[136..138], &0x1111u16.to_le_bytes()); // platform_bps
    assert_eq!(&data[138..140], &0x2222u16.to_le_bytes()); // creator_bps
    assert_eq!(&data[140..142], &0x3333u16.to_le_bytes()); // addon_budget_bps
    assert_eq!(data[142], 0x44); // default_preset
    assert_eq!(data[143], 0x55); // max_open_pools
    assert_eq!(data[144], 0x66); // max_own_boxes
    assert_eq!(data[145], 1); // preseason_enabled
    assert_eq!(data[146], 0); // paused
    assert_eq!(PlatformConfig::TOKENS_OFFSET, 147);
    for (i, seed) in [0x10u8, 0x20, 0x30].iter().enumerate() {
        let at = 147 + i * 98;
        assert_eq!(
            &data[at + 1..at + 33],
            &[*seed; 32],
            "tokens[{i}].mint at {}",
            at + 1
        );
    }
    assert_eq!(data[441], 0xBB); // bump
    assert_eq!(&data[442..698], &[0xEEu8; 256]); // reserved
}

#[test]
fn wallet_override_is_43_bytes_at_the_documented_offsets() {
    assert_eq!(WalletOverride::SIZE, 43); // 8 + 35
    let value = WalletOverride {
        wallet: key(0xC1),
        max_open_pools: 0x77,
        max_own_boxes: 0x88,
        bump: 0x99,
    };
    let data = serialize(&value);
    assert_eq!(data.len(), 43);
    assert_eq!(&data[0..8], WalletOverride::DISCRIMINATOR);
    assert_eq!(&data[8..40], &[0xC1u8; 32]); // wallet
    assert_eq!(data[40], 0x77); // max_open_pools
    assert_eq!(data[41], 0x88); // max_own_boxes
    assert_eq!(data[42], 0x99); // bump
}

#[test]
fn game_key_is_5_bytes_matching_mybarpool_shared() {
    // PROGRAM §2; `encodeGameKey({2026, 1, 15, 8})` in @mybarpool/shared is ea 07 01 0f 08
    // (2026 = 0x07EA little-endian; the brief's "e2 07" was a typo, see NOTES).
    assert_eq!(GameKey::SIZE, 5);
    let key = GameKey {
        season: 2026,
        week: 1,
        home: 15,
        away: 8,
    };
    let mut data = Vec::new();
    anchor_lang::AnchorSerialize::serialize(&key, &mut data).unwrap();
    assert_eq!(data, [0xea, 0x07, 0x01, 0x0f, 0x08]);
}

#[test]
fn game_status_is_one_byte_with_the_section_3_2_discriminants() {
    // PROGRAM §3.2: Scheduled 0, Postponed 1, Cancelled 2, Suspended 3, Final 4.
    for (status, byte) in [
        (GameStatus::Scheduled, 0u8),
        (GameStatus::Postponed, 1),
        (GameStatus::Cancelled, 2),
        (GameStatus::Suspended, 3),
        (GameStatus::Final, 4),
    ] {
        let mut data = Vec::new();
        anchor_lang::AnchorSerialize::serialize(&status, &mut data).unwrap();
        assert_eq!(data, [byte], "{status:?}");
        let back: GameStatus = anchor_lang::AnchorDeserialize::deserialize(&mut &data[..]).unwrap();
        assert_eq!(back, status);
    }
    assert!(<GameStatus as anchor_lang::AnchorDeserialize>::deserialize(&mut &[5u8][..]).is_err());
}

#[test]
fn game_record_is_153_bytes_at_the_documented_offsets() {
    assert_eq!(GameRecord::SIZE, 153); // PROGRAM §3.2: 8 + 145
    let record = GameRecord {
        key: GameKey {
            season: 0x1234,
            week: 0x56,
            home: 0x78,
            away: 0x9A,
        },
        scheduled_kickoff: 0x0101_0101_0101_0101,
        recorded_kickoff: 0x0202_0202_0202_0202,
        status: GameStatus::Suspended,
        quarters_posted: 0xBC,
        home_score: [0x1111, 0x2222, 0x3333, 0x4444],
        away_score: [0x5555, 0x6666, 0x7777, 0x8888],
        posted_at: [
            0x0303_0303_0303_0303,
            0x0404_0404_0404_0404,
            0x0505_0505_0505_0505,
            0x0606_0606_0606_0606,
        ],
        final_had_overtime: true,
        marked_at: 0x0707_0707_0707_0707,
        bump: 0xDE,
        reserved: [0xEE; 64],
    };
    let data = serialize(&record);
    assert_eq!(data.len(), 153);
    assert_eq!(&data[0..8], GameRecord::DISCRIMINATOR);
    assert_eq!(&data[8..10], &0x1234u16.to_le_bytes()); // key.season
    assert_eq!(data[10], 0x56); // key.week
    assert_eq!(data[11], 0x78); // key.home
    assert_eq!(data[12], 0x9A); // key.away
    assert_eq!(&data[13..21], &0x0101_0101_0101_0101i64.to_le_bytes()); // scheduled_kickoff
    assert_eq!(&data[21..29], &0x0202_0202_0202_0202i64.to_le_bytes()); // recorded_kickoff
    assert_eq!(data[29], 3); // status = Suspended
    assert_eq!(data[30], 0xBC); // quarters_posted
    for (i, score) in [0x1111u16, 0x2222, 0x3333, 0x4444].iter().enumerate() {
        let at = 31 + 2 * i;
        assert_eq!(
            &data[at..at + 2],
            &score.to_le_bytes(),
            "home_score[{i}] at {at}"
        );
    }
    for (i, score) in [0x5555u16, 0x6666, 0x7777, 0x8888].iter().enumerate() {
        let at = 39 + 2 * i;
        assert_eq!(
            &data[at..at + 2],
            &score.to_le_bytes(),
            "away_score[{i}] at {at}"
        );
    }
    for (i, at_time) in [
        0x0303_0303_0303_0303i64,
        0x0404_0404_0404_0404,
        0x0505_0505_0505_0505,
        0x0606_0606_0606_0606,
    ]
    .iter()
    .enumerate()
    {
        let at = 47 + 8 * i;
        assert_eq!(
            &data[at..at + 8],
            &at_time.to_le_bytes(),
            "posted_at[{i}] at {at}"
        );
    }
    assert_eq!(data[79], 1); // final_had_overtime
    assert_eq!(&data[80..88], &0x0707_0707_0707_0707i64.to_le_bytes()); // marked_at
    assert_eq!(data[88], 0xDE); // bump
    assert_eq!(&data[89..153], &[0xEEu8; 64]); // reserved
}

#[test]
fn game_record_seed_bytes_are_the_section_3_2_layout() {
    // PROGRAM §3.2 seeds as bytes: "game", season u16 LE, week, home, away, kickoff i64 LE.
    // This freezes the Rust seed layout against hand-written bytes; the cross-language check
    // (the address `@mybarpool/shared`'s gameRecordSeeds derives equals the record's) is in the
    // localnet suite, games.test.ts test 2 (Step 3 audit L3).
    let key = GameKey {
        season: 2026,
        week: 1,
        home: 15,
        away: 8,
    };
    let kickoff: i64 = 1_800_086_400;
    // The bytes `gameRecordSeeds` yields for these inputs: "game", ea 07, 01, 0f, 08, kickoff LE.
    let mut seeds: Vec<Vec<u8>> = vec![b"game".to_vec()];
    seeds.push(vec![0xea, 0x07]);
    seeds.push(vec![1]);
    seeds.push(vec![15]);
    seeds.push(vec![8]);
    seeds.push(vec![0x80, 0x23, 0x4b, 0x6b, 0, 0, 0, 0]);
    assert_eq!(seeds[5], kickoff.to_le_bytes().to_vec());
    let seed_refs: Vec<&[u8]> = seeds.iter().map(|s| s.as_slice()).collect();
    let expected = Pubkey::find_program_address(&seed_refs, &mybarpool::ID);
    let from_fields = Pubkey::find_program_address(
        &[
            GAME_SEED,
            &key.season.to_le_bytes(),
            &[key.week],
            &[key.home],
            &[key.away],
            &kickoff.to_le_bytes(),
        ],
        &mybarpool::ID,
    );
    assert_eq!(expected, from_fields);
}

#[test]
fn constants_match_program_section_1() {
    assert_eq!(BOXES, 25);
    assert_eq!(LANES, 5);
    assert_eq!(QUARTERS, 4);
    assert_eq!(PLATFORM_BPS_MAX, 500);
    assert_eq!(CREATOR_BPS_MAX, 500);
    assert_eq!(ADDON_BUDGET_BPS_MAX, 500);
    assert_eq!(TOTAL_BPS_MAX, 1500);
    assert_eq!(MIN_QUARTER_SECONDS, 900);
    assert_eq!(KICKOFF_UPDATE_BOUND, 259_200); // 72 h
    assert_eq!(RECLAIM_DELAY, 2_592_000); // 30 d
    assert_eq!(MAX_OWN_BOXES_ABSOLUTE, 25);
    assert_eq!(
        ENTROPY_PROGRAM.to_string(),
        "3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X"
    );
    assert_eq!(CONFIG_SEED, b"config");
    assert_eq!(OVERRIDE_SEED, b"override");
    assert_eq!(GAME_SEED, b"game");
    assert_eq!(SOL_DECIMALS, 9);
    for split in PRESET_SPLITS {
        assert_eq!(split.iter().map(|&p| u32::from(p)).sum::<u32>(), 100);
    }
    assert_eq!(
        PRESET_SPLITS[PayoutPreset::Standard as usize],
        [20, 20, 20, 40]
    );
    assert_eq!(PRESET_SPLITS[PayoutPreset::Even as usize], [25, 25, 25, 25]);
    assert_eq!(
        PRESET_SPLITS[PayoutPreset::FinalOnly as usize],
        [0, 0, 0, 100]
    );
    assert_eq!(PayoutPreset::try_from(0).unwrap(), PayoutPreset::Standard);
    assert_eq!(PayoutPreset::try_from(2).unwrap(), PayoutPreset::FinalOnly);
    for bad in [3u8, 255] {
        let error = PayoutPreset::try_from(bad).unwrap_err();
        assert!(
            matches!(error, anchor_lang::error::Error::AnchorError(ref e) if e.error_code_number == 6014),
            "preset {bad} must be InvalidPreset (6014), got {error:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Step 4: Pool, CreatorCounter, Sponsorship, and the three enums
// ---------------------------------------------------------------------------

fn round_trip_one_byte<T>(cases: &[(T, u8)], next: u8)
where
    T: anchor_lang::AnchorSerialize + anchor_lang::AnchorDeserialize + PartialEq + std::fmt::Debug,
{
    for (value, byte) in cases {
        let mut data = Vec::new();
        anchor_lang::AnchorSerialize::serialize(value, &mut data).unwrap();
        assert_eq!(data, [*byte], "{value:?}");
        let back: T = anchor_lang::AnchorDeserialize::deserialize(&mut &data[..]).unwrap();
        assert_eq!(&back, value);
    }
    assert!(<T as anchor_lang::AnchorDeserialize>::deserialize(&mut &[next][..]).is_err());
}

#[test]
fn pool_status_access_type_and_payout_preset_are_one_byte_each() {
    // PROGRAM §3.3: Open 0 … Split 5; Public 0, Link 1, Allowlist 2; §1: Standard 0, Even 1, FinalOnly 2.
    round_trip_one_byte(
        &[
            (PoolStatus::Open, 0u8),
            (PoolStatus::Locked, 1),
            (PoolStatus::Drawn, 2),
            (PoolStatus::Settled, 3),
            (PoolStatus::Returned, 4),
            (PoolStatus::Split, 5),
        ],
        6,
    );
    round_trip_one_byte(
        &[
            (AccessType::Public, 0u8),
            (AccessType::Link, 1),
            (AccessType::Allowlist, 2),
        ],
        3,
    );
    round_trip_one_byte(
        &[
            (PayoutPreset::Standard, 0u8),
            (PayoutPreset::Even, 1),
            (PayoutPreset::FinalOnly, 2),
        ],
        3,
    );
}

#[test]
fn pool_is_1442_bytes_at_the_documented_offsets() {
    assert_eq!(Pool::SIZE, 1442); // PROGRAM §3.3: 8 + 1,434
    let mut owners = [Pubkey::default(); 25];
    for (i, o) in owners.iter_mut().enumerate() {
        *o = key(0x40 + i as u8);
    }
    let pool = Pool {
        game: key(0x01),
        creator: key(0x02),
        nonce: 0x0303_0303_0303_0303,
        token: 0x04,
        mint: key(0x05),
        token_program: key(0x06),
        vault: key(0x07),
        price: 0x0808_0808_0808_0808,
        preset: PayoutPreset::FinalOnly,
        access_type: AccessType::Link,
        gate_key: key(0x09),
        allowlist_root: [0x0A; 32],
        creator_addon_bps: 0x0B0B,
        integrator: key(0x0C),
        integrator_bps: 0x0D0D,
        platform_fee: 0x0E0E_0E0E_0E0E_0E0E,
        creator_fee: 0x0F0F_0F0F_0F0F_0F0F,
        integrator_fee: 0x1010_1010_1010_1010,
        status: PoolStatus::Split,
        sold: 0x11,
        owners,
        creator_boxes: 0x12,
        sponsored_total: 0x1313_1313_1313_1313,
        sponsor_count: 0x1414,
        sponsorships_open: 0x1515,
        var: key(0x16),
        var_end_at: 0x1717_1717_1717_1717,
        sampled_slot: 0x1818_1818_1818_1818,
        sampled_hash: [0x19; 32],
        var_replacements: 0x1A,
        drawn: true,
        home_axis: [0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24],
        away_axis: [0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E],
        prize_pool: 0x2F2F_2F2F_2F2F_2F2F,
        quarter_prize: [
            0x3030_3030_3030_3030,
            0x3131_3131_3131_3131,
            0x3232_3232_3232_3232,
            0x3333_3333_3333_3333,
        ],
        quarters_settled: 0x34,
        winning_box: [0x35, 0x36, 0x37, 0x38],
        fees_paid: true,
        unpaid_prize_pool: 0x3939_3939_3939_3939,
        returned: 0x3A3A_3A3A,
        split_amount: 0x3B3B_3B3B_3B3B_3B3B,
        cancelled_by_admin: true,
        abandoned: true,
        created_at: 0x3C3C_3C3C_3C3C_3C3C,
        locked_at: 0x3D3D_3D3D_3D3D_3D3D,
        bump: 0x3E,
        vault_bump: 0x3F,
        reserved: [0xEE; 128],
    };
    let data = serialize(&pool);
    assert_eq!(data.len(), 1442);
    assert_eq!(&data[0..8], Pool::DISCRIMINATOR);
    assert_eq!(&data[8..40], &[0x01u8; 32]); // game
    assert_eq!(&data[40..72], &[0x02u8; 32]); // creator
    assert_eq!(&data[72..80], &0x0303_0303_0303_0303u64.to_le_bytes()); // nonce
    assert_eq!(data[80], 0x04); // token
    assert_eq!(&data[81..113], &[0x05u8; 32]); // mint
    assert_eq!(&data[113..145], &[0x06u8; 32]); // token_program
    assert_eq!(&data[145..177], &[0x07u8; 32]); // vault
    assert_eq!(&data[177..185], &0x0808_0808_0808_0808u64.to_le_bytes()); // price
    assert_eq!(data[185], 2); // preset = FinalOnly
    assert_eq!(data[186], 1); // access_type = Link
    assert_eq!(&data[187..219], &[0x09u8; 32]); // gate_key
    assert_eq!(&data[219..251], &[0x0Au8; 32]); // allowlist_root
    assert_eq!(&data[251..253], &0x0B0Bu16.to_le_bytes()); // creator_addon_bps
    assert_eq!(&data[253..285], &[0x0Cu8; 32]); // integrator
    assert_eq!(&data[285..287], &0x0D0Du16.to_le_bytes()); // integrator_bps
    assert_eq!(&data[287..295], &0x0E0E_0E0E_0E0E_0E0Eu64.to_le_bytes()); // platform_fee
    assert_eq!(&data[295..303], &0x0F0F_0F0F_0F0F_0F0Fu64.to_le_bytes()); // creator_fee
    assert_eq!(&data[303..311], &0x1010_1010_1010_1010u64.to_le_bytes()); // integrator_fee
    assert_eq!(data[311], 5); // status = Split
    assert_eq!(data[312], 0x11); // sold
    for i in 0..25 {
        let at = 313 + 32 * i;
        assert_eq!(
            &data[at..at + 32],
            &[0x40 + i as u8; 32],
            "owners[{i}] at {at}"
        );
    }
    assert_eq!(data[1113], 0x12); // creator_boxes
    assert_eq!(&data[1114..1122], &0x1313_1313_1313_1313u64.to_le_bytes()); // sponsored_total
    assert_eq!(&data[1122..1124], &0x1414u16.to_le_bytes()); // sponsor_count
    assert_eq!(&data[1124..1126], &0x1515u16.to_le_bytes()); // sponsorships_open
    assert_eq!(&data[1126..1158], &[0x16u8; 32]); // var
    assert_eq!(&data[1158..1166], &0x1717_1717_1717_1717u64.to_le_bytes()); // var_end_at
    assert_eq!(&data[1166..1174], &0x1818_1818_1818_1818u64.to_le_bytes()); // sampled_slot
    assert_eq!(&data[1174..1206], &[0x19u8; 32]); // sampled_hash
    assert_eq!(data[1206], 0x1A); // var_replacements
    assert_eq!(data[1207], 1); // drawn
    assert_eq!(&data[1208..1218], &pool.home_axis); // home_axis
    assert_eq!(&data[1218..1228], &pool.away_axis); // away_axis
    assert_eq!(&data[1228..1236], &0x2F2F_2F2F_2F2F_2F2Fu64.to_le_bytes()); // prize_pool
    for (i, q) in pool.quarter_prize.iter().enumerate() {
        let at = 1236 + 8 * i;
        assert_eq!(
            &data[at..at + 8],
            &q.to_le_bytes(),
            "quarter_prize[{i}] at {at}"
        );
    }
    assert_eq!(data[1268], 0x34); // quarters_settled
    assert_eq!(&data[1269..1273], &[0x35, 0x36, 0x37, 0x38]); // winning_box
    assert_eq!(data[1273], 1); // fees_paid
    assert_eq!(&data[1274..1282], &0x3939_3939_3939_3939u64.to_le_bytes()); // unpaid_prize_pool
    assert_eq!(&data[1282..1286], &0x3A3A_3A3Au32.to_le_bytes()); // returned
    assert_eq!(&data[1286..1294], &0x3B3B_3B3B_3B3B_3B3Bu64.to_le_bytes()); // split_amount
    assert_eq!(data[1294], 1); // cancelled_by_admin
    assert_eq!(data[1295], 1); // abandoned
    assert_eq!(&data[1296..1304], &0x3C3C_3C3C_3C3C_3C3Ci64.to_le_bytes()); // created_at
    assert_eq!(&data[1304..1312], &0x3D3D_3D3D_3D3D_3D3Di64.to_le_bytes()); // locked_at
    assert_eq!(data[1312], 0x3E); // bump
    assert_eq!(data[1313], 0x3F); // vault_bump
    assert_eq!(&data[1314..1442], &[0xEEu8; 128]); // reserved
}

#[test]
fn creator_counter_is_74_bytes_at_the_documented_offsets() {
    assert_eq!(CreatorCounter::SIZE, 74); // PROGRAM §3.5: 8 + 66
    let value = CreatorCounter {
        creator: key(0xC1),
        game: key(0xC2),
        open_count: 0xC3,
        bump: 0xC4,
    };
    let data = serialize(&value);
    assert_eq!(data.len(), 74);
    assert_eq!(&data[0..8], CreatorCounter::DISCRIMINATOR);
    assert_eq!(&data[8..40], &[0xC1u8; 32]); // creator
    assert_eq!(&data[40..72], &[0xC2u8; 32]); // game
    assert_eq!(data[72], 0xC3); // open_count
    assert_eq!(data[73], 0xC4); // bump
}

#[test]
fn sponsorship_is_81_bytes_at_the_documented_offsets() {
    assert_eq!(Sponsorship::SIZE, 81); // PROGRAM §3.7: 8 + 73
    let value = Sponsorship {
        pool: key(0xD1),
        wallet: key(0xD2),
        amount: 0xD3D3_D3D3_D3D3_D3D3,
        bump: 0xD4,
    };
    let data = serialize(&value);
    assert_eq!(data.len(), 81);
    assert_eq!(&data[0..8], Sponsorship::DISCRIMINATOR);
    assert_eq!(&data[8..40], &[0xD1u8; 32]); // pool
    assert_eq!(&data[40..72], &[0xD2u8; 32]); // wallet
    assert_eq!(&data[72..80], &0xD3D3_D3D3_D3D3_D3D3u64.to_le_bytes()); // amount
    assert_eq!(data[80], 0xD4); // bump
}

#[test]
fn step_4_seed_prefixes_match_program_section_3() {
    // PROGRAM §3.3–§3.7; the byte layouts of the four PDAs' seeds, in the order
    // poolSeeds / vaultSeeds / counterSeeds / sponsorshipSeeds in @mybarpool/shared produce them
    // (the address-level cross-check is in the localnet suite, pools.test.ts).
    assert_eq!(POOL_SEED, b"pool");
    assert_eq!(VAULT_SEED, b"vault");
    assert_eq!(COUNTER_SEED, b"counter");
    assert_eq!(SPONSORSHIP_SEED, b"sponsorship");
    assert_eq!(BPS_DENOMINATOR, 10_000);
    assert_eq!(NO_WINNING_BOX, 255);
    let game = key(0xAA);
    let creator = key(0xBB);
    let nonce: u64 = 7;
    let pool = Pubkey::find_program_address(
        &[
            POOL_SEED,
            game.as_ref(),
            creator.as_ref(),
            &nonce.to_le_bytes(),
        ],
        &mybarpool::ID,
    );
    let by_bytes = Pubkey::find_program_address(
        &[b"pool", &[0xAA; 32], &[0xBB; 32], &[7, 0, 0, 0, 0, 0, 0, 0]],
        &mybarpool::ID,
    );
    assert_eq!(pool, by_bytes);
}

#[test]
fn entropy_var_is_240_bytes_at_the_brief_offsets() {
    // this brief, the `Var` offset table: disc 0..8, authority 8..40, id 40..48,
    // provider 48..80, commit 80..112, seed 112..144, slot_hash 144..176, value 176..208,
    // samples 208..216, is_auto 216..224, start_at 224..232, end_at 232..240.
    use mybarpool::entropy::{Var, VAR_LEN, VAR_SEED};
    assert_eq!(VAR_LEN, 240);
    assert_eq!(VAR_SEED, b"var");
    let fields = common::VarFields {
        authority: common::to_m(&key(0x01)),
        id: 0x0202_0202_0202_0202,
        provider: common::to_m(&key(0x03)),
        commit: [0x04; 32],
        seed: [0x05; 32],
        slot_hash: [0x06; 32],
        value: [0x07; 32],
        samples: 0x0808_0808_0808_0808,
        is_auto: 0x0909_0909_0909_0909,
        start_at: 0x0A0A_0A0A_0A0A_0A0A,
        end_at: 0x0B0B_0B0B_0B0B_0B0B,
    };
    let data = common::var_bytes(&fields);
    assert_eq!(&data[0..8], &[0u8; 8]);
    assert_eq!(&data[8..40], key(0x01).as_ref());
    assert_eq!(&data[40..48], &0x0202_0202_0202_0202u64.to_le_bytes());
    assert_eq!(&data[48..80], key(0x03).as_ref());
    assert_eq!(&data[80..112], &[0x04; 32]);
    assert_eq!(&data[112..144], &[0x05; 32]);
    assert_eq!(&data[144..176], &[0x06; 32]);
    assert_eq!(&data[176..208], &[0x07; 32]);
    assert_eq!(&data[208..216], &0x0808_0808_0808_0808u64.to_le_bytes());
    assert_eq!(&data[216..224], &0x0909_0909_0909_0909u64.to_le_bytes());
    assert_eq!(&data[224..232], &0x0A0A_0A0A_0A0A_0A0Au64.to_le_bytes());
    assert_eq!(&data[232..240], &0x0B0B_0B0B_0B0B_0B0Bu64.to_le_bytes());

    // The program's decoder reads the same offsets.
    let account = common::var_account(&fields);
    let key_ = key(0x10);
    let mut lamports = account.lamports;
    let mut bytes = account.data.clone();
    let owner = common::to_a(&account.owner);
    let info = anchor_lang::prelude::AccountInfo::new(
        &key_,
        false,
        false,
        &mut lamports,
        &mut bytes,
        &owner,
        false,
    );
    let v = Var::try_from_account(&info).expect("decode");
    assert_eq!(v.authority, key(0x01));
    assert_eq!(v.id, fields.id);
    assert_eq!(v.provider, key(0x03));
    assert_eq!(
        (v.commit, v.seed, v.slot_hash, v.value),
        ([0x04; 32], [0x05; 32], [0x06; 32], [0x07; 32])
    );
    assert_eq!(
        (v.samples, v.is_auto, v.start_at, v.end_at),
        (
            fields.samples,
            fields.is_auto,
            fields.start_at,
            fields.end_at
        )
    );
}
