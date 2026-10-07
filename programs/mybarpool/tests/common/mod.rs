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

use anchor_lang::prelude::SlotHashes;
use anchor_lang::solana_program::program_pack::Pack;
use anchor_spl::token::spl_token::state::{Account as TokenAccountLegacy, AccountState};
use anchor_spl::token_2022::spl_token_2022::state::Account as TokenAccount2022;
use mollusk_svm::sysvar::Sysvars;

use mybarpool::entropy::{expected_value, VAR_LEN, VAR_SEED};
use mybarpool::{
    constants::{
        PayoutPreset, BOXES, CONFIG_SEED, COUNTER_SEED, ENTROPY_PROGRAM, GAME_SEED, NO_WINNING_BOX,
        OVERRIDE_SEED, POOL_SEED, QUARTERS, SPONSORSHIP_SEED, VAULT_SEED,
    },
    AccessType, CreatePoolParams, CreatorCounter, GameKey, GameRecord, GameStatus, PlatformConfig,
    Pool, PoolStatus, Sponsorship, TokenRule, WalletOverride,
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
    /// Step 4 wallets, 100 SOL each in `base_accounts` so a 25 SOL sponsorship cap can be hit.
    pub creator: Pubkey,
    pub buyer: Pubkey,
    pub buyer_2: Pubkey,
    pub sponsor: Pubkey,
    pub integrator: Pubkey,
    /// A Token-2022 mint at 6 decimals standing in for SKR in tests (`skr_rule_2022`).
    pub skr_mint: Pubkey,
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
            creator: Pubkey::new_unique(),
            buyer: Pubkey::new_unique(),
            buyer_2: Pubkey::new_unique(),
            sponsor: Pubkey::new_unique(),
            integrator: Pubkey::new_unique(),
            skr_mint: Pubkey::new_unique(),
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

    /// The "SKR@6" ladder of the shared vectors: 100 / 100 / 5 000 whole tokens at 6 decimals,
    /// under Token-2022 (the audit-focus "Token vs Token-2022" case); `max_sponsorship = 25 × max`.
    pub fn skr_rule_2022(&self) -> TokenRule {
        TokenRule {
            enabled: true,
            mint: to_a(&self.skr_mint),
            token_program: anchor_spl::token_2022::ID,
            decimals: 6,
            min_price: 100_000_000,
            step: 100_000_000,
            max_price: 5_000_000_000,
            max_sponsorship: 125_000_000_000,
        }
    }

    /// `initialize_params` with the SKR slot enabled as a Token-2022 rule.
    pub fn initialize_params_with_skr(&self) -> mybarpool::InitializeParams {
        let mut p = self.initialize_params();
        p.tokens[1] = self.skr_rule_2022();
        p
    }

    /// `expected_config` with the SKR slot enabled as a Token-2022 rule.
    pub fn config_with_skr(&self) -> PlatformConfig {
        let mut c = self.expected_config();
        c.tokens[1] = self.skr_rule_2022();
        c
    }

    pub fn skr_mint_account(&self) -> Account {
        token_2022_mint_account(6, &[ExtensionType::MetadataPointer])
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
        (f.creator, system_account(100 * LAMPORTS_PER_SOL)),
        (f.buyer, system_account(100 * LAMPORTS_PER_SOL)),
        (f.buyer_2, system_account(100 * LAMPORTS_PER_SOL)),
        (f.sponsor, system_account(100 * LAMPORTS_PER_SOL)),
        (f.integrator, system_account(LAMPORTS_PER_SOL)),
        (program_data_pda(), program_data_account(Some(&f.admin))),
        (f.ore_mint, f.ore_mint_account()),
        (f.skr_mint, f.skr_mint_account()),
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

// ---------------------------------------------------------------------------
// Pool fixtures (Step 4)
// ---------------------------------------------------------------------------

/// ARCHITECTURE › Buying: the SOL minimum, 0.05 SOL.
pub const PRICE: u64 = 50_000_000;
/// ARCHITECTURE › Buying: the ORE minimum, 0.05 ORE at 11 decimals.
pub const PRICE_ORE: u64 = 5_000_000_000;
/// The SKR@6 minimum: 100 SKR at 6 decimals.
pub const PRICE_SKR: u64 = 100_000_000;
pub const NONCE: u64 = 7;
/// A known slot hash for the SlotHashes fixture.
pub const SLOT_HASH: [u8; 32] = [0x5A; 32];
pub const SLOT: u64 = 454_000_000;

pub fn pool_pda(game: &Pubkey, creator: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            POOL_SEED,
            game.as_ref(),
            creator.as_ref(),
            &nonce.to_le_bytes(),
        ],
        &program_id(),
    )
}

pub fn vault_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref()], &program_id())
}

pub fn counter_pda(creator: &Pubkey, game: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[COUNTER_SEED, creator.as_ref(), game.as_ref()],
        &program_id(),
    )
}

pub fn sponsorship_pda(pool: &Pubkey, wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SPONSORSHIP_SEED, pool.as_ref(), wallet.as_ref()],
        &program_id(),
    )
}

/// The standard game's address.
pub fn standard_game() -> Pubkey {
    game_pda(&standard_key(), SCHEDULED).0
}

/// `CreatePoolParams` for a SOL pool: `PRICE`, `Standard`, `Public`, 2 % add-on, no integrator.
pub fn sol_params(initial_boxes: u8) -> CreatePoolParams {
    CreatePoolParams {
        nonce: NONCE,
        token: 0,
        price: PRICE,
        preset: PayoutPreset::Standard,
        access_type: AccessType::Public,
        gate_key: APubkey::default(),
        allowlist_root: [0u8; 32],
        creator_addon_bps: 200,
        integrator: APubkey::default(),
        integrator_bps: 0,
        initial_boxes,
    }
}

/// `CreatePoolParams` for an ORE pool (token 2).
pub fn ore_params(initial_boxes: u8) -> CreatePoolParams {
    CreatePoolParams {
        token: 2,
        price: PRICE_ORE,
        ..sol_params(initial_boxes)
    }
}

/// `CreatePoolParams` for an SKR pool (token 1, Token-2022 in tests).
pub fn skr_params(initial_boxes: u8) -> CreatePoolParams {
    CreatePoolParams {
        token: 1,
        price: PRICE_SKR,
        ..sol_params(initial_boxes)
    }
}

/// A `Pool` as `create_pool` writes it for `creator` on the standard game with `params`, under
/// `config`'s fee bps, before any initial boxes. `created_at = T0`.
pub fn fresh_pool(f: &Fixture, config: &PlatformConfig, params: &CreatePoolParams) -> Pool {
    let game = standard_game();
    let (pool, bump) = pool_pda(&game, &f.creator, params.nonce);
    let (vault, vault_bump) = vault_pda(&pool);
    let rule = config.tokens[usize::from(params.token)];
    let fees = mybarpool::money::fee_amounts(
        params.price,
        config.platform_bps,
        config.creator_bps,
        params.creator_addon_bps,
        params.integrator_bps,
    )
    .expect("fees");
    Pool {
        game: to_a(&game),
        creator: to_a(&f.creator),
        nonce: params.nonce,
        token: params.token,
        mint: rule.mint,
        token_program: rule.token_program,
        vault: to_a(&vault),
        price: params.price,
        preset: params.preset,
        access_type: params.access_type,
        gate_key: params.gate_key,
        allowlist_root: params.allowlist_root,
        creator_addon_bps: params.creator_addon_bps,
        integrator: params.integrator,
        integrator_bps: params.integrator_bps,
        platform_fee: fees.platform_fee,
        creator_fee: fees.creator_fee,
        integrator_fee: fees.integrator_fee,
        status: PoolStatus::Open,
        sold: 0,
        owners: [APubkey::default(); BOXES as usize],
        creator_boxes: 0,
        sponsored_total: 0,
        sponsor_count: 0,
        sponsorships_open: 0,
        var: APubkey::default(),
        var_end_at: 0,
        sampled_slot: 0,
        sampled_hash: [0u8; 32],
        var_replacements: 0,
        drawn: false,
        home_axis: [0u8; 10],
        away_axis: [0u8; 10],
        prize_pool: 0,
        quarter_prize: [0u64; QUARTERS as usize],
        quarters_settled: 0,
        winning_box: [NO_WINNING_BOX; QUARTERS as usize],
        fees_paid: false,
        unpaid_prize_pool: 0,
        returned: 0,
        split_amount: 0,
        cancelled_by_admin: false,
        abandoned: false,
        created_at: T0,
        locked_at: 0,
        bump,
        vault_bump,
        reserved: [0u8; 128],
    }
}

/// `fresh_pool` with `sold` boxes owned by `owner` (the first `sold` indices) and a status.
pub fn pool_with(f: &Fixture, status: PoolStatus, sold: u8, owner: &Pubkey) -> Pool {
    let mut pool = fresh_pool(f, &f.expected_config(), &sol_params(0));
    for i in 0..usize::from(sold) {
        pool.owners[i] = to_a(owner);
    }
    pool.sold = sold;
    if *owner == f.creator {
        pool.creator_boxes = sold;
    }
    pool.status = status;
    if status != PoolStatus::Open {
        pool.locked_at = T0 - 3_600;
    }
    pool
}

pub fn pool_account(pool: &Pool) -> Account {
    account_for(pool, &program_id(), Pool::SIZE)
}

pub fn counter_account(creator: &Pubkey, game: &Pubkey, open_count: u8) -> Account {
    let counter = CreatorCounter {
        creator: to_a(creator),
        game: to_a(game),
        open_count,
        bump: counter_pda(creator, game).1,
    };
    account_for(&counter, &program_id(), CreatorCounter::SIZE)
}

pub fn override_account(wallet: &Pubkey, max_open_pools: u8, max_own_boxes: u8) -> Account {
    let value = WalletOverride {
        wallet: to_a(wallet),
        max_open_pools,
        max_own_boxes,
        bump: override_pda(wallet).1,
    };
    account_for(&value, &program_id(), WalletOverride::SIZE)
}

/// A system-owned SOL vault holding `lamports` (rent floor + purchases).
pub fn sol_vault_account(lamports: u64) -> Account {
    system_account(lamports)
}

/// Rent floor of a zero-byte account under Mollusk's default rent.
pub fn rent_for(size: usize) -> u64 {
    anchor_lang::prelude::Rent::default().minimum_balance(size)
}

/// A classic Token account for `mint` owned by `owner` holding `amount`.
pub fn token_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    let state = TokenAccountLegacy {
        mint: to_a(mint),
        owner: to_a(owner),
        amount,
        delegate: None.into(),
        state: AccountState::Initialized,
        is_native: None.into(),
        delegated_amount: 0,
        close_authority: None.into(),
    };
    let mut data = vec![0u8; TokenAccountLegacy::LEN];
    TokenAccountLegacy::pack(state, &mut data).expect("pack token account");
    Account {
        lamports: rent_for(TokenAccountLegacy::LEN),
        data,
        owner: token_program_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// A Token-2022 account (no extensions) for `mint` owned by `owner` holding `amount`.
pub fn token_2022_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    let state = TokenAccount2022 {
        mint: to_a(mint),
        owner: to_a(owner),
        amount,
        delegate: None.into(),
        state: anchor_spl::token_2022::spl_token_2022::state::AccountState::Initialized,
        is_native: None.into(),
        delegated_amount: 0,
        close_authority: None.into(),
    };
    let mut data = vec![0u8; TokenAccount2022::LEN];
    TokenAccount2022::pack(state, &mut data).expect("pack token-2022 account");
    Account {
        lamports: rent_for(TokenAccount2022::LEN),
        data,
        owner: token_2022_program_id(),
        executable: false,
        rent_epoch: 0,
    }
}

pub fn decode_token_amount(account: &Account) -> u64 {
    TokenAccountLegacy::unpack_from_slice(&account.data[..TokenAccountLegacy::LEN])
        .expect("token account")
        .amount
}

pub fn decode_token_owner(account: &Account) -> Pubkey {
    to_m(
        &TokenAccountLegacy::unpack_from_slice(&account.data[..TokenAccountLegacy::LEN])
            .expect("token account")
            .owner,
    )
}

pub fn decode_pool(account: &Account) -> Pool {
    let mut slice: &[u8] = &account.data;
    <Pool as anchor_lang::AccountDeserialize>::try_deserialize(&mut slice).expect("decode pool")
}

pub fn decode_counter(account: &Account) -> CreatorCounter {
    let mut slice: &[u8] = &account.data;
    <CreatorCounter as anchor_lang::AccountDeserialize>::try_deserialize(&mut slice)
        .expect("decode counter")
}

pub fn decode_sponsorship(account: &Account) -> Sponsorship {
    let mut slice: &[u8] = &account.data;
    <Sponsorship as anchor_lang::AccountDeserialize>::try_deserialize(&mut slice)
        .expect("decode sponsorship")
}

/// A Mollusk at `unix_timestamp` with the Token and Token-2022 programs loaded and the
/// SlotHashes sysvar holding one entry `(SLOT, hash)`.
pub fn mollusk_for_pools(unix_timestamp: i64, hash: [u8; 32]) -> Mollusk {
    let mut m = mollusk_at(unix_timestamp);
    mollusk_svm_programs_token::token::add_program(&mut m);
    mollusk_svm_programs_token::token2022::add_program(&mut m);
    set_slot_hash(&mut m, hash);
    m
}

pub fn set_slot_hash(m: &mut Mollusk, hash: [u8; 32]) {
    let Sysvars { slot_hashes, .. } = &mut m.sysvars;
    *slot_hashes = SlotHashes::new(&[(SLOT, solana_hash::Hash::new_from_array(hash))]);
}

/// The SlotHashes keyed account from the Mollusk's sysvars.
pub fn slot_hashes_account(m: &Mollusk) -> (Pubkey, Account) {
    m.sysvars.keyed_account_for_slot_hashes_sysvar()
}

/// `base_accounts` with `config`, plus the standard game record, the Step 4 token programs,
/// the SlotHashes account, an empty `wallet_override` slot for the creator, and an empty slot
/// for the standard pool, its vault and the creator's counter (so `create_pool` can `init`).
pub fn pool_base(f: &Fixture, m: &Mollusk, config: &PlatformConfig) -> Vec<(Pubkey, Account)> {
    let mut accounts = base_accounts(f, Some(config));
    accounts.push((standard_game(), game_record_account(&standard_record())));
    accounts.push(mollusk_svm_programs_token::token::keyed_account());
    accounts.push(mollusk_svm_programs_token::token2022::keyed_account());
    accounts.push(slot_hashes_account(m));
    accounts.push((override_pda(&f.creator).0, system_account(0)));
    let pool = pool_pda(&standard_game(), &f.creator, NONCE).0;
    accounts.push((pool, system_account(0)));
    accounts.push((vault_pda(&pool).0, system_account(0)));
    accounts.push((
        counter_pda(&f.creator, &standard_game()).0,
        system_account(0),
    ));
    accounts
}

/// Replace (or add) the account at `key`.
pub fn set_account(accounts: &mut Vec<(Pubkey, Account)>, key: Pubkey, account: Account) {
    if let Some(slot) = accounts.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = account;
    } else {
        accounts.push((key, account));
    }
}

/// `pool_base` with an existing pool, its SOL vault, the counter at `open_count`, and an empty
/// sponsorship slot for `f.sponsor` and `f.buyer`.
pub fn pool_accounts(
    f: &Fixture,
    m: &Mollusk,
    config: &PlatformConfig,
    pool: &Pool,
    vault_lamports: u64,
    open_count: u8,
) -> Vec<(Pubkey, Account)> {
    let mut accounts = pool_base(f, m, config);
    let pool_key = pool_pda(&standard_game(), &f.creator, pool.nonce).0;
    set_account(&mut accounts, pool_key, pool_account(pool));
    set_account(
        &mut accounts,
        vault_pda(&pool_key).0,
        sol_vault_account(vault_lamports),
    );
    set_account(
        &mut accounts,
        counter_pda(&f.creator, &standard_game()).0,
        counter_account(&f.creator, &standard_game(), open_count),
    );
    accounts.push((sponsorship_pda(&pool_key, &f.sponsor).0, system_account(0)));
    accounts.push((sponsorship_pda(&pool_key, &f.buyer).0, system_account(0)));
    accounts.push((sponsorship_pda(&pool_key, &f.creator).0, system_account(0)));
    accounts
}

/// The three optional token-path accounts, `None` for SOL.
#[derive(Clone, Copy, Default)]
pub struct TokenPath {
    pub mint: Option<Pubkey>,
    pub token_account: Option<Pubkey>,
    pub token_program: Option<Pubkey>,
}

pub fn create_pool_ix(
    creator: &Pubkey,
    game: &Pubkey,
    params: CreatePoolParams,
    token: TokenPath,
) -> Instruction {
    let pool = pool_pda(game, creator, params.nonce).0;
    instruction(
        mybarpool::accounts::CreatePool {
            creator: to_a(creator),
            config: to_a(&config_pda().0),
            game: to_a(game),
            pool: to_a(&pool),
            vault: to_a(&vault_pda(&pool).0),
            counter: to_a(&counter_pda(creator, game).0),
            wallet_override: to_a(&override_pda(creator).0),
            mint: token.mint.map(|k| to_a(&k)),
            creator_token_account: token.token_account.map(|k| to_a(&k)),
            token_program: token.token_program.map(|k| to_a(&k)),
            slot_hashes: to_a(&solana_sdk_ids::sysvar::slot_hashes::ID),
            system_program: anchor_lang::system_program::ID,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::CreatePool { params },
    )
}

pub fn buy_ix(buyer: &Pubkey, pool: &Pool, count: u8, token: TokenPath) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::Buy {
            buyer: to_a(buyer),
            config: to_a(&config_pda().0),
            game: pool.game,
            pool: to_a(&pool_key),
            vault: to_a(&vault_pda(&pool_key).0),
            counter: to_a(&counter_pda(&to_m(&pool.creator), &to_m(&pool.game)).0),
            wallet_override: to_a(&override_pda(&to_m(&pool.creator)).0),
            mint: token.mint.map(|k| to_a(&k)),
            buyer_token_account: token.token_account.map(|k| to_a(&k)),
            token_program: token.token_program.map(|k| to_a(&k)),
            slot_hashes: to_a(&solana_sdk_ids::sysvar::slot_hashes::ID),
            system_program: anchor_lang::system_program::ID,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::Buy { count },
    )
}

pub fn sponsor_ix(sponsor: &Pubkey, pool: &Pool, amount: u64, token: TokenPath) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::Sponsor {
            sponsor: to_a(sponsor),
            config: to_a(&config_pda().0),
            game: pool.game,
            pool: to_a(&pool_key),
            vault: to_a(&vault_pda(&pool_key).0),
            sponsorship: to_a(&sponsorship_pda(&pool_key, sponsor).0),
            mint: token.mint.map(|k| to_a(&k)),
            sponsor_token_account: token.token_account.map(|k| to_a(&k)),
            token_program: token.token_program.map(|k| to_a(&k)),
            system_program: anchor_lang::system_program::ID,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::Sponsor { amount },
    )
}

pub fn rotate_gate_key_ix(creator: &Pubkey, pool: &Pool, new_key: &Pubkey) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::RotateGateKey {
            creator: to_a(creator),
            pool: to_a(&pool_key),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::RotateGateKey {
            new_key: to_a(new_key),
        },
    )
}

pub fn close_counter_ix(creator: &Pubkey, game: &Pubkey, fee_wallet: &Pubkey) -> Instruction {
    instruction(
        mybarpool::accounts::CloseCounter {
            counter: to_a(&counter_pda(creator, game).0),
            config: to_a(&config_pda().0),
            fee_wallet: to_a(fee_wallet),
        },
        mybarpool::instruction::CloseCounter {},
    )
}

/// All event payloads of a result as `(event discriminator, body)` for order assertions.
pub fn event_names(result: &InstructionResult) -> Vec<&'static str> {
    use anchor_lang::Discriminator;
    event_payloads(result)
        .into_iter()
        .map(|p| {
            let d = &p[..8];
            if d == mybarpool::PoolCreated::DISCRIMINATOR {
                "PoolCreated"
            } else if d == mybarpool::BoxesBought::DISCRIMINATOR {
                "BoxesBought"
            } else if d == mybarpool::PoolLocked::DISCRIMINATOR {
                "PoolLocked"
            } else if d == mybarpool::Sponsored::DISCRIMINATOR {
                "Sponsored"
            } else if d == mybarpool::GateKeyRotated::DISCRIMINATOR {
                "GateKeyRotated"
            } else if d == mybarpool::VarSet::DISCRIMINATOR {
                "VarSet"
            } else if d == mybarpool::VarSampled::DISCRIMINATOR {
                "VarSampled"
            } else if d == mybarpool::VarReplaced::DISCRIMINATOR {
                "VarReplaced"
            } else if d == mybarpool::DigitsDrawn::DISCRIMINATOR {
                "DigitsDrawn"
            } else {
                "other"
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Entropy fixtures (Step 5)
// ---------------------------------------------------------------------------

/// The deployed Entropy bytecode, dumped from mainnet (ProgramData bytes 45.., trailing zeros
/// stripped); `tests/entropy.rs` checks its SHA-256 against the `verify.osec.io` report.
pub const ENTROPY_ELF: &[u8] = include_bytes!("../fixtures/entropy-f26ae03.so");
/// The live ORE `Var` `BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E`, fetched at slot 454,331,258.
pub const LIVE_VAR: &[u8] = include_bytes!("../fixtures/var-BWCaDY96.bin");

pub fn entropy_id() -> Pubkey {
    to_m(&ENTROPY_PROGRAM)
}

/// The ELF's true length: its section-header table ends at `e_shoff + e_shentsize × e_shnum`
/// (98,368 + 576 for this file). The committed fixture is the trailing-zero-stripped form
/// `verify.osec.io` hashes (98,929 bytes), and the last fifteen bytes of that table happen to
/// be zero, so the loader needs them back; they are padding and nothing else.
pub const ENTROPY_ELF_LOADABLE_LEN: usize = 98_944;

/// Load the real Entropy program under loader v3, as a second program beside ours.
pub fn with_entropy(m: &mut Mollusk) {
    let mut elf = ENTROPY_ELF.to_vec();
    assert!(elf.len() <= ENTROPY_ELF_LOADABLE_LEN);
    elf.resize(ENTROPY_ELF_LOADABLE_LEN, 0);
    m.add_program_with_loader_and_elf(
        &entropy_id(),
        &mollusk_svm::program::loader_keys::LOADER_V3,
        &elf,
    );
}

/// The Entropy program's keyed account (executable), for contexts that take it by address.
pub fn entropy_program_account() -> (Pubkey, Account) {
    (
        entropy_id(),
        create_program_account_loader_v3(&entropy_id()),
    )
}

/// `["var", authority, id LE]` under the Entropy program.
pub fn var_pda(authority: &Pubkey, id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[VAR_SEED, authority.as_ref(), &id.to_le_bytes()],
        &entropy_id(),
    )
}

/// The brief's standard window and values.
pub const END_AT: u64 = 1_000;
pub const END_HASH: [u8; 32] = [0x5B; 32];
pub const SEED: [u8; 32] = [0x11; 32];
pub const VAR_ID: u64 = 7;

/// `keccak(SEED)`: the commit `Open` would have written.
pub fn commit_of(seed: &[u8; 32]) -> [u8; 32] {
    solana_keccak_hasher::hashv(&[seed]).to_bytes()
}

/// Every field of a `Var`, for the planter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VarFields {
    pub authority: Pubkey,
    pub id: u64,
    pub provider: Pubkey,
    pub commit: [u8; 32],
    pub seed: [u8; 32],
    pub slot_hash: [u8; 32],
    pub value: [u8; 32],
    pub samples: u64,
    pub is_auto: u64,
    pub start_at: u64,
    pub end_at: u64,
}

/// The 240 bytes of a `Var`, written from the brief's offset table independently of the
/// program's decoder (the decoder test proves the two agree).
pub fn var_bytes(f: &VarFields) -> [u8; VAR_LEN] {
    let mut d = [0u8; VAR_LEN];
    d[8..40].copy_from_slice(f.authority.as_ref());
    d[40..48].copy_from_slice(&f.id.to_le_bytes());
    d[48..80].copy_from_slice(f.provider.as_ref());
    d[80..112].copy_from_slice(&f.commit);
    d[112..144].copy_from_slice(&f.seed);
    d[144..176].copy_from_slice(&f.slot_hash);
    d[176..208].copy_from_slice(&f.value);
    d[208..216].copy_from_slice(&f.samples.to_le_bytes());
    d[216..224].copy_from_slice(&f.is_auto.to_le_bytes());
    d[224..232].copy_from_slice(&f.start_at.to_le_bytes());
    d[232..240].copy_from_slice(&f.end_at.to_le_bytes());
    d
}

pub fn var_account(f: &VarFields) -> Account {
    Account {
        lamports: rent_for(VAR_LEN),
        data: var_bytes(f).to_vec(),
        owner: entropy_id(),
        executable: false,
        rent_epoch: 0,
    }
}

impl Fixture {
    /// A `Var` as `Open` leaves it: committed, zero seed/hash/value, one sample, manual,
    /// authority the keeper, provider the configured one, `id` 7.
    pub fn fresh_var(&self, end_at: u64) -> VarFields {
        VarFields {
            authority: self.keeper,
            id: VAR_ID,
            provider: self.entropy_provider,
            commit: commit_of(&SEED),
            seed: [0u8; 32],
            slot_hash: [0u8; 32],
            value: [0u8; 32],
            samples: 1,
            is_auto: 0,
            start_at: END_AT.saturating_sub(150),
            end_at,
        }
    }

    /// `fresh_var` after `Sample` wrote `hash`.
    pub fn sampled_var(&self, end_at: u64, hash: [u8; 32]) -> VarFields {
        VarFields {
            slot_hash: hash,
            ..self.fresh_var(end_at)
        }
    }

    /// `sampled_var` after `Reveal(seed)`: `value = keccak(hash ‖ seed ‖ samples)`.
    pub fn revealed_var(&self, end_at: u64, hash: [u8; 32], seed: &[u8; 32]) -> VarFields {
        VarFields {
            commit: commit_of(seed),
            seed: *seed,
            value: expected_value(&hash, seed, 1),
            ..self.sampled_var(end_at, hash)
        }
    }

    /// The standard `Var`'s address.
    pub fn var_key(&self) -> Pubkey {
        var_pda(&self.keeper, VAR_ID).0
    }
}

/// `VALUE`: the revealed value of the standard `Var` (`END_HASH`, `SEED`, one sample).
pub fn standard_value() -> [u8; 32] {
    expected_value(&END_HASH, &SEED, 1)
}

/// A `Locked` pool (all 25 boxes sold to `buyer_2`) with `var` bound at `end_at`.
pub fn locked_pool_with_var(f: &Fixture, var: &Pubkey, end_at: u64) -> Pool {
    let mut pool = pool_with(f, PoolStatus::Locked, 25, &f.buyer_2);
    pool.var = to_a(var);
    pool.var_end_at = end_at;
    pool
}

/// `locked_pool_with_var` after `sample_var` recorded `(slot, hash)`.
pub fn sampled_pool(f: &Fixture, var: &Pubkey, end_at: u64, slot: u64, hash: [u8; 32]) -> Pool {
    let mut pool = locked_pool_with_var(f, var, end_at);
    pool.sampled_slot = slot;
    pool.sampled_hash = hash;
    pool
}

/// A `Locked` pool planted with `drawn = true` (unreachable through the program; defence tests).
pub fn drawn_pool(f: &Fixture, var: &Pubkey, end_at: u64) -> Pool {
    let mut pool = sampled_pool(f, var, end_at, END_AT + 3, END_HASH);
    pool.drawn = true;
    pool
}

/// A Mollusk at `(slot, unix_timestamp)` with SlotHashes holding `entries` newest-first.
pub fn mollusk_for_draw(slot: u64, entries: &[(u64, [u8; 32])]) -> Mollusk {
    let mut m = mollusk_at(T0);
    m.sysvars.clock.slot = slot;
    let hashes: Vec<(u64, solana_hash::Hash)> = entries
        .iter()
        .map(|(s, h)| (*s, solana_hash::Hash::new_from_array(*h)))
        .collect();
    m.sysvars.slot_hashes = SlotHashes::new(&hashes);
    m
}

/// The standard SlotHashes window around `END_AT`: `[(END_AT + 2, h2), (END_AT + 1, h1), (END_AT, END_HASH)]`.
pub fn standard_window() -> Vec<(u64, [u8; 32])> {
    vec![
        (END_AT + 2, [0xE3; 32]),
        (END_AT + 1, [0xE2; 32]),
        (END_AT, END_HASH),
    ]
}

/// `pool_accounts` for a draw test: the pool with a SOL vault holding 25 boxes, the counter at
/// 0, the `Var` at `var_key`, the SlotHashes account, the Entropy program account.
pub fn draw_accounts(
    f: &Fixture,
    m: &Mollusk,
    pool: &Pool,
    var: Option<(&Pubkey, &VarFields)>,
) -> Vec<(Pubkey, Account)> {
    let mut accounts = pool_accounts(
        f,
        m,
        &f.expected_config(),
        pool,
        rent_for(0) + 25 * PRICE,
        0,
    );
    if let Some((key, fields)) = var {
        set_account(&mut accounts, *key, var_account(fields));
    }
    accounts.push(entropy_program_account());
    accounts
}

pub fn set_var_ix(signer: &Pubkey, pool: &Pool, var: &Pubkey) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::SetVar {
            score_authority: to_a(signer),
            config: to_a(&config_pda().0),
            pool: to_a(&pool_key),
            var: to_a(var),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::SetVar {},
    )
}

pub fn sample_var_ix(sampler: &Pubkey, pool: &Pool, var: &Pubkey) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::SampleVar {
            sampler: to_a(sampler),
            pool: to_a(&pool_key),
            var: to_a(var),
            slot_hashes: to_a(&solana_sdk_ids::sysvar::slot_hashes::ID),
            entropy_program: ENTROPY_PROGRAM,
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::SampleVar {},
    )
}

pub fn draw_ix(signer: &Pubkey, pool: &Pool, var: &Pubkey) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::Draw {
            score_authority: to_a(signer),
            config: to_a(&config_pda().0),
            pool: to_a(&pool_key),
            var: to_a(var),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::Draw {},
    )
}

pub fn replace_var_ix(signer: &Pubkey, pool: &Pool, new_var: &Pubkey) -> Instruction {
    let pool_key = pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0;
    instruction(
        mybarpool::accounts::ReplaceVar {
            admin: to_a(signer),
            config: to_a(&config_pda().0),
            pool: to_a(&pool_key),
            new_var: to_a(new_var),
            event_authority: to_a(&event_authority_pda()),
            program: mybarpool::ID,
        },
        mybarpool::instruction::ReplaceVar {},
    )
}
