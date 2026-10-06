//! Shared Mollusk fixtures for the program's unit tests (build plan Steps 2 and 3).
//!
//! Mollusk and Anchor sit on different `solana-pubkey` majors, so keys cross
//! the boundary by bytes (`to_m` / `to_a`), as Step 0's `mollusk.rs` did.
#![allow(dead_code)]

use anchor_lang::prelude::Pubkey as APubkey;
use anchor_lang::solana_program::bpf_loader_upgradeable::UpgradeableLoaderState;
use anchor_lang::{AnchorDeserialize, Discriminator, InstructionData, ToAccountMetas};
use mollusk_svm::program::{create_program_account_loader_v3, keyed_account_for_system_program};
use mollusk_svm::result::InstructionResult;
use mollusk_svm::Mollusk;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

// The SPL interface crates at the versions anchor-spl 1.2.0 resolves (re-exported), so the
// fixtures and the program agree on one version of each (Step 2 audit M1).
use anchor_spl::token::spl_token::state::Mint as MintLegacy;
use anchor_spl::token_2022::spl_token_2022::extension::{
    metadata_pointer::MetadataPointer, transfer_fee::TransferFeeConfig,
    transfer_hook::TransferHook, BaseStateWithExtensionsMut, ExtensionType, StateWithExtensionsMut,
};
use anchor_spl::token_2022::spl_token_2022::state::Mint as Mint2022;

use mybarpool::{
    constants::{CONFIG_SEED, GAME_SEED, OVERRIDE_SEED},
    GameKey, GameRecord, GameStatus, PlatformConfig, TokenRule, WalletOverride,
};

pub const PROGRAM_NAME: &str = "mybarpool";
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

pub fn to_m(k: &APubkey) -> Pubkey {
    Pubkey::new_from_array(k.to_bytes())
}
pub fn to_a(k: &Pubkey) -> APubkey {
    APubkey::new_from_array(k.to_bytes())
}

pub fn program_id() -> Pubkey {
    to_m(&mybarpool::ID)
}

pub fn bpf_loader_upgradeable_id() -> Pubkey {
    to_m(&anchor_lang::solana_program::bpf_loader_upgradeable::ID)
}

pub fn token_program_id() -> Pubkey {
    to_m(&anchor_spl::token::ID)
}

pub fn token_2022_program_id() -> Pubkey {
    to_m(&anchor_spl::token_2022::ID)
}

/// ORE mint, ARCHITECTURE › Solana program (classic Token, 11 decimals).
pub fn ore_mint() -> Pubkey {
    Pubkey::from_str_const("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp")
}

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG_SEED], &program_id())
}

pub fn override_pda(wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[OVERRIDE_SEED, wallet.as_ref()], &program_id())
}

pub fn program_data_pda() -> Pubkey {
    Pubkey::find_program_address(&[program_id().as_ref()], &bpf_loader_upgradeable_id()).0
}

pub fn event_authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"__event_authority"], &program_id()).0
}

pub fn mollusk() -> Mollusk {
    Mollusk::new(&program_id(), PROGRAM_NAME)
}

// ---------------------------------------------------------------------------
// Clock (Step 3): every test states its `now`; nothing reads the host clock.
// ---------------------------------------------------------------------------

/// Fixed "now" for the game tests: 2027-01-15T08:00:00Z. Every other time derives from it.
pub const T0: i64 = 1_800_000_000;

/// A Mollusk whose Clock sysvar reads `unix_timestamp` (and a plausible slot: 400 ms slots
/// from the Unix epoch, which only has to be monotonic with the timestamp).
pub fn mollusk_at(unix_timestamp: i64) -> Mollusk {
    let mut m = mollusk();
    m.sysvars.clock.unix_timestamp = unix_timestamp;
    m.sysvars.clock.slot = u64::try_from(unix_timestamp).unwrap_or(0) * 5 / 2;
    m
}

// ---------------------------------------------------------------------------
// Game record fixtures (Step 3)
// ---------------------------------------------------------------------------

/// The standard game: PROGRAM §2 team table, KC (15) hosting DAL (8), week 1 of 2026.
pub fn standard_key() -> GameKey {
    GameKey {
        season: 2026,
        week: 1,
        home: 15,
        away: 8,
    }
}

/// The standard scheduled kickoff: a day after `T0`.
pub const SCHEDULED: i64 = T0 + 86_400;

pub fn game_pda(key: &GameKey, scheduled_kickoff: i64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            GAME_SEED,
            &key.season.to_le_bytes(),
            &[key.week],
            &[key.home],
            &[key.away],
            &scheduled_kickoff.to_le_bytes(),
        ],
        &program_id(),
    )
}

/// A fresh `Scheduled` record for `key` at `scheduled_kickoff`, as `create_game` writes it.
pub fn fresh_record(key: GameKey, scheduled_kickoff: i64) -> GameRecord {
    GameRecord {
        key,
        scheduled_kickoff,
        recorded_kickoff: scheduled_kickoff,
        status: GameStatus::Scheduled,
        quarters_posted: 0,
        home_score: [0; 4],
        away_score: [0; 4],
        posted_at: [0; 4],
        final_had_overtime: false,
        marked_at: 0,
        bump: game_pda(&key, scheduled_kickoff).1,
        reserved: [0; 64],
    }
}

/// The standard record, `Scheduled`, nothing posted.
pub fn standard_record() -> GameRecord {
    fresh_record(standard_key(), SCHEDULED)
}

/// The standard record with `n` quarters posted at 45-minute spacing from kickoff, with the
/// brief's scores 7–3, 14–10, 17–17, 24–20 (the fourth sets `Final`).
pub fn record_with_quarters(n: u8) -> GameRecord {
    let mut r = standard_record();
    let scores = [(7u16, 3u16), (14, 10), (17, 17), (24, 20)];
    for q in 0..usize::from(n) {
        r.home_score[q] = scores[q].0;
        r.away_score[q] = scores[q].1;
        r.posted_at[q] = r.recorded_kickoff + 2_700 * (q as i64 + 1);
    }
    r.quarters_posted = n;
    if n == 4 {
        r.status = GameStatus::Final;
        r.final_had_overtime = true;
    }
    r
}

pub fn game_record_account(record: &GameRecord) -> Account {
    account_for(record, &program_id(), GameRecord::SIZE)
}

pub fn decode_game(account: &Account) -> GameRecord {
    let mut slice: &[u8] = &account.data;
    <GameRecord as anchor_lang::AccountDeserialize>::try_deserialize(&mut slice)
        .expect("decode game record")
}

pub fn create_game_ix(signer: &Pubkey, key: GameKey, scheduled_kickoff: i64) -> Instruction {
    instruction(
        mybarpool::accounts::CreateGame {
            score_authority: to_a(signer),
            config: to_a(&config_pda().0),
            game: to_a(&game_pda(&key, scheduled_kickoff).0),
            system_program: anchor_lang::system_program::ID,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::CreateGame {
            key,
            scheduled_kickoff,
        },
    )
}

pub fn update_kickoff_ix(signer: &Pubkey, game: &Pubkey, new_time: i64) -> Instruction {
    instruction(
        mybarpool::accounts::UpdateKickoff {
            score_authority: to_a(signer),
            config: to_a(&config_pda().0),
            game: to_a(game),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::UpdateKickoff { new_time },
    )
}

pub fn post_scores_ix(
    signer: &Pubkey,
    game: &Pubkey,
    quarter: u8,
    home: u16,
    away: u16,
    is_final: bool,
    had_overtime: bool,
) -> Instruction {
    instruction(
        mybarpool::accounts::PostScores {
            score_authority: to_a(signer),
            config: to_a(&config_pda().0),
            game: to_a(game),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::PostScores {
            quarter,
            home,
            away,
            is_final,
            had_overtime,
        },
    )
}

pub fn mark_game_ix(signer: &Pubkey, game: &Pubkey, new_status: GameStatus) -> Instruction {
    instruction(
        mybarpool::accounts::MarkGame {
            admin: to_a(signer),
            config: to_a(&config_pda().0),
            game: to_a(game),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::MarkGame { new_status },
    )
}

/// `base_accounts` with the initial config plus a game record (or an empty slot at its PDA).
pub fn game_accounts(f: &Fixture, record: Option<&GameRecord>) -> Vec<(Pubkey, Account)> {
    let mut accounts = base_accounts(f, Some(&f.expected_config()));
    match record {
        Some(r) => accounts.push((
            game_pda(&r.key, r.scheduled_kickoff).0,
            game_record_account(r),
        )),
        None => accounts.push((game_pda(&standard_key(), SCHEDULED).0, system_account(0))),
    }
    accounts
}

pub fn system_account(lamports: u64) -> Account {
    Account {
        lamports,
        data: vec![],
        owner: Pubkey::default(),
        executable: false,
        rent_epoch: 0,
    }
}

/// A `ProgramData` account whose upgrade authority is `authority`.
pub fn program_data_account(authority: Option<&Pubkey>) -> Account {
    let state = UpgradeableLoaderState::ProgramData {
        slot: 1,
        upgrade_authority_address: authority.map(to_a),
    };
    let mut data = bincode::serialize(&state).expect("serialise ProgramData");
    // Real ProgramData accounts carry the ELF after the header; a few bytes stand in for it.
    data.extend_from_slice(&[0u8; 64]);
    Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data,
        owner: bpf_loader_upgradeable_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// Classic SPL Token mint.
pub fn legacy_mint_account(decimals: u8) -> Account {
    use anchor_lang::solana_program::program_pack::Pack;
    let mint = MintLegacy {
        mint_authority: None.into(),
        supply: 0,
        decimals,
        is_initialized: true,
        freeze_authority: None.into(),
    };
    let mut data = vec![0u8; MintLegacy::LEN];
    MintLegacy::pack(mint, &mut data).expect("pack mint");
    Account {
        lamports: LAMPORTS_PER_SOL,
        data,
        owner: token_program_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// Token-2022 mint with the given extensions initialised (default-valued).
pub fn token_2022_mint_account(decimals: u8, extensions: &[ExtensionType]) -> Account {
    let len = ExtensionType::try_calculate_account_len::<Mint2022>(extensions).expect("len");
    let mut data = vec![0u8; len];
    {
        let mut state =
            StateWithExtensionsMut::<Mint2022>::unpack_uninitialized(&mut data).expect("unpack");
        for ext in extensions {
            match ext {
                ExtensionType::TransferFeeConfig => {
                    state
                        .init_extension::<TransferFeeConfig>(true)
                        .expect("init");
                }
                ExtensionType::TransferHook => {
                    state.init_extension::<TransferHook>(true).expect("init");
                }
                ExtensionType::MetadataPointer => {
                    state.init_extension::<MetadataPointer>(true).expect("init");
                }
                other => panic!("fixture does not support {other:?}"),
            }
        }
        state.base = Mint2022 {
            mint_authority: None.into(),
            supply: 0,
            decimals,
            is_initialized: true,
            freeze_authority: None.into(),
        };
        state.pack_base();
        state.init_account_type().expect("account type");
    }
    Account {
        lamports: LAMPORTS_PER_SOL,
        data,
        owner: token_2022_program_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// The §3.1 initial configuration, as the Step 14 deploy will pass it.
pub struct Fixture {
    pub admin: Pubkey,
    pub keeper: Pubkey,
    pub entropy_provider: Pubkey,
    pub fee_wallet: Pubkey,
    pub ore_mint: Pubkey,
    /// A funded key that is neither admin nor keeper.
    pub stranger: Pubkey,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            admin: Pubkey::new_unique(),
            keeper: Pubkey::new_unique(),
            entropy_provider: Pubkey::new_unique(),
            fee_wallet: Pubkey::new_unique(),
            ore_mint: ore_mint(),
            stranger: Pubkey::new_unique(),
        }
    }

    /// ARCHITECTURE › Buying table: SOL 0.05 / 0.05 / 1 at 9 decimals; max_sponsorship 25 × max.
    pub fn sol_rule() -> TokenRule {
        TokenRule {
            enabled: true,
            mint: APubkey::default(),
            token_program: APubkey::default(),
            decimals: 9,
            min_price: 50_000_000,
            step: 50_000_000,
            max_price: 1_000_000_000,
            max_sponsorship: 25_000_000_000,
        }
    }

    /// SKR ships as a disabled placeholder until its mint is known (ARCHITECTURE › Solana program).
    pub fn skr_placeholder() -> TokenRule {
        TokenRule {
            enabled: false,
            mint: APubkey::default(),
            token_program: APubkey::default(),
            decimals: 0,
            min_price: 1,
            step: 1,
            max_price: 1,
            max_sponsorship: 1,
        }
    }

    /// ARCHITECTURE › Buying table: ORE 0.05 / 0.05 / 1 at 11 decimals, classic Token program.
    pub fn ore_rule(&self) -> TokenRule {
        TokenRule {
            enabled: true,
            mint: to_a(&self.ore_mint),
            token_program: anchor_spl::token::ID,
            decimals: 11,
            min_price: 5_000_000_000,
            step: 5_000_000_000,
            max_price: 100_000_000_000,
            max_sponsorship: 2_500_000_000_000,
        }
    }

    /// PROGRAM §3.1 "initial" column: 500 / 500 / 500, preset 0, 3 open pools, 5 own boxes.
    pub fn initialize_params(&self) -> mybarpool::InitializeParams {
        mybarpool::InitializeParams {
            score_authority: to_a(&self.keeper),
            entropy_provider: to_a(&self.entropy_provider),
            fee_wallet: to_a(&self.fee_wallet),
            platform_bps: 500,
            creator_bps: 500,
            addon_budget_bps: 500,
            default_preset: 0,
            max_open_pools: 3,
            max_own_boxes: 5,
            preseason_enabled: false,
            paused: false,
            tokens: [Self::sol_rule(), Self::skr_placeholder(), self.ore_rule()],
        }
    }

    /// The config as `initialize` would write it from `initialize_params`.
    pub fn expected_config(&self) -> PlatformConfig {
        let p = self.initialize_params();
        PlatformConfig {
            admin: to_a(&self.admin),
            score_authority: p.score_authority,
            entropy_provider: p.entropy_provider,
            fee_wallet: p.fee_wallet,
            platform_bps: p.platform_bps,
            creator_bps: p.creator_bps,
            addon_budget_bps: p.addon_budget_bps,
            default_preset: p.default_preset,
            max_open_pools: p.max_open_pools,
            max_own_boxes: p.max_own_boxes,
            preseason_enabled: p.preseason_enabled,
            paused: p.paused,
            tokens: p.tokens,
            bump: config_pda().1,
            reserved: [0u8; 256],
        }
    }

    pub fn config_account(&self, config: &PlatformConfig) -> Account {
        account_for(config, &program_id(), PlatformConfig::SIZE)
    }

    pub fn ore_mint_account(&self) -> Account {
        legacy_mint_account(11)
    }
}

pub fn no_update() -> mybarpool::UpdateConfigParams {
    mybarpool::UpdateConfigParams {
        admin: None,
        score_authority: None,
        entropy_provider: None,
        fee_wallet: None,
        platform_bps: None,
        creator_bps: None,
        addon_budget_bps: None,
        default_preset: None,
        max_open_pools: None,
        max_own_boxes: None,
        preseason_enabled: None,
        paused: None,
        tokens: [None, None, None],
    }
}

/// Serialise an Anchor account (discriminator + borsh) into an account of `size` bytes.
pub fn account_for<T: anchor_lang::AccountSerialize>(
    value: &T,
    owner: &Pubkey,
    size: usize,
) -> Account {
    let mut data = Vec::with_capacity(size);
    value.try_serialize(&mut data).expect("serialise");
    data.resize(size, 0);
    Account {
        lamports: LAMPORTS_PER_SOL,
        data,
        owner: *owner,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn decode_config(account: &Account) -> PlatformConfig {
    let mut slice: &[u8] = &account.data;
    <PlatformConfig as anchor_lang::AccountDeserialize>::try_deserialize(&mut slice)
        .expect("decode config")
}

pub fn decode_override(account: &Account) -> WalletOverride {
    let mut slice: &[u8] = &account.data;
    <WalletOverride as anchor_lang::AccountDeserialize>::try_deserialize(&mut slice)
        .expect("decode override")
}

/// Convert Anchor account metas (Anchor pubkey type) to Mollusk's.
pub fn metas(anchor_metas: Vec<anchor_lang::prelude::AccountMeta>) -> Vec<AccountMeta> {
    anchor_metas
        .into_iter()
        .map(|m| AccountMeta {
            pubkey: to_m(&m.pubkey),
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        })
        .collect()
}

/// The fixture accounts for the `event_cpi` pair every instruction ends with.
pub fn event_cpi_accounts() -> Vec<(Pubkey, Account)> {
    vec![
        (event_authority_pda(), system_account(0)),
        (
            program_id(),
            create_program_account_loader_v3(&program_id()),
        ),
    ]
}

/// Anchor's generated `accounts::*` structs already list `event_authority` and `program`.
pub fn instruction<A: ToAccountMetas, D: InstructionData>(accounts: A, data: D) -> Instruction {
    Instruction::new_with_bytes(
        program_id(),
        &data.data(),
        metas(accounts.to_account_metas(None)),
    )
}

/// Build `initialize`, with the ORE mint attached and SKR's slot filled by the program id
/// (Anchor's convention for an absent optional account).
pub fn initialize_ix(
    signer: &Pubkey,
    params: mybarpool::InitializeParams,
    mint_2: Option<&Pubkey>,
) -> Instruction {
    instruction(
        mybarpool::accounts::Initialize {
            admin: to_a(signer),
            config: to_a(&config_pda().0),
            program_data: to_a(&program_data_pda()),
            mint_1: None,
            mint_2: mint_2.map(to_a),
            system_program: anchor_lang::system_program::ID,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::Initialize { params },
    )
}

pub fn update_config_ix(
    signer: &Pubkey,
    params: mybarpool::UpdateConfigParams,
    mint_1: Option<&Pubkey>,
    mint_2: Option<&Pubkey>,
) -> Instruction {
    instruction(
        mybarpool::accounts::UpdateConfig {
            admin: to_a(signer),
            config: to_a(&config_pda().0),
            mint_1: mint_1.map(to_a),
            mint_2: mint_2.map(to_a),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::UpdateConfig { params },
    )
}

pub fn set_override_ix(
    signer: &Pubkey,
    wallet: &Pubkey,
    max_open_pools: u8,
    max_own_boxes: u8,
) -> Instruction {
    instruction(
        mybarpool::accounts::SetWalletOverride {
            admin: to_a(signer),
            config: to_a(&config_pda().0),
            wallet_override: to_a(&override_pda(wallet).0),
            system_program: anchor_lang::system_program::ID,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::SetWalletOverride {
            wallet: to_a(wallet),
            max_open_pools,
            max_own_boxes,
        },
    )
}

pub fn close_override_ix(signer: &Pubkey, wallet: &Pubkey) -> Instruction {
    instruction(
        mybarpool::accounts::CloseWalletOverride {
            admin: to_a(signer),
            config: to_a(&config_pda().0),
            wallet_override: to_a(&override_pda(wallet).0),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::CloseWalletOverride {
            wallet: to_a(wallet),
        },
    )
}

/// Everything the four instructions can touch, with the system program and the event-CPI pair.
pub fn base_accounts(f: &Fixture, config: Option<&PlatformConfig>) -> Vec<(Pubkey, Account)> {
    let mut accounts = vec![
        (f.admin, system_account(10 * LAMPORTS_PER_SOL)),
        (f.keeper, system_account(10 * LAMPORTS_PER_SOL)),
        (f.stranger, system_account(10 * LAMPORTS_PER_SOL)),
        (program_data_pda(), program_data_account(Some(&f.admin))),
        (f.ore_mint, f.ore_mint_account()),
        keyed_account_for_system_program(),
    ];
    accounts.extend(event_cpi_accounts());
    match config {
        Some(c) => accounts.push((config_pda().0, f.config_account(c))),
        None => accounts.push((config_pda().0, system_account(0))),
    }
    accounts
}

pub fn account_of<'a>(result: &'a InstructionResult, key: &Pubkey) -> &'a Account {
    &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| k == key)
        .expect("account in result")
        .1
}

/// The Anchor custom error code (6000 + index) of a Mollusk failure, if it is one.
pub fn custom_error(result: &InstructionResult) -> Option<u32> {
    use mollusk_svm::result::ProgramResult;
    match &result.program_result {
        ProgramResult::Failure(solana_program_error::ProgramError::Custom(code)) => Some(*code),
        _ => None,
    }
}

pub fn err(e: mybarpool::MybarpoolError) -> u32 {
    anchor_lang::error::ERROR_CODE_OFFSET + e as u32
}

/// The event-CPI payloads (event discriminator + body) among the result's inner instructions:
/// only self-CPIs whose program is ours and whose data starts with Anchor's event-CPI tag
/// (`anchor_lang::event::EVENT_IX_TAG_LE`, the same bytes `tests/helpers/mybarpool.ts` uses).
/// `init` CPIs the system program and Step 4 adds token CPIs; neither counts (Step 2 audit L2).
pub fn event_payloads(result: &InstructionResult) -> Vec<&[u8]> {
    let message = result.message.as_ref().expect("result carries its message");
    let keys = message.account_keys();
    result
        .inner_instructions
        .iter()
        .filter(|inner| {
            keys.get(usize::from(inner.instruction.program_id_index)) == Some(&program_id())
        })
        .map(|inner| inner.instruction.data.as_slice())
        .filter(|data| data.len() >= 16 && &data[..8] == anchor_lang::event::EVENT_IX_TAG_LE)
        .map(|data| &data[8..])
        .collect()
}

/// Decode the `emit_cpi!` event of type `E` from the result's inner instructions.
pub fn emitted_event<E: AnchorDeserialize + Discriminator>(
    result: &InstructionResult,
) -> Option<E> {
    event_payloads(result)
        .into_iter()
        .find(|payload| &payload[..8] == E::DISCRIMINATOR)
        .map(|payload| {
            let mut body = &payload[8..];
            E::deserialize(&mut body).expect("decode event")
        })
}

/// Inner instructions that are `emit_cpi!` self-calls.
pub fn emitted_event_count(result: &InstructionResult) -> usize {
    event_payloads(result).len()
}
