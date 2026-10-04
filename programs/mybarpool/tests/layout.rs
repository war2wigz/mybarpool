//! Account layouts, PROGRAM §3 preamble, §3.1, §3.6. Every field holds a
//! distinct sentinel; each is asserted at the offset the brief's table gives.

mod common;

use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountSerialize, Discriminator};
use mybarpool::constants::*;
use mybarpool::{PlatformConfig, TokenRule, WalletOverride};

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
