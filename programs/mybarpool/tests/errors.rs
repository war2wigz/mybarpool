//! Error codes and order, PROGRAM §8 (+ `InvalidConfig` appended, see NOTES).

use anchor_lang::error::ERROR_CODE_OFFSET;
use mybarpool::MybarpoolError as E;

/// PROGRAM §8, in the listed order, then `InvalidConfig`.
const EXPECTED: [E; 60] = [
    E::Unauthorized,
    E::Paused,
    E::GameNotScheduled,
    E::GameAlreadyMarked,
    E::SalesClosed,
    E::KickoffInPast,
    E::KickoffOutOfBounds,
    E::KickoffUpdateTooLate,
    E::QuarterOutOfOrder,
    E::QuarterTooSoon,
    E::ScoreDecreased,
    E::FinalFlagMismatch,
    E::TokenDisabled,
    E::PriceOffLadder,
    E::InvalidPreset,
    E::InvalidAccessType,
    E::GateKeyMissing,
    E::GateKeyNotSigner,
    E::AllowlistProofInvalid,
    E::AddonBudgetExceeded,
    E::IntegratorMismatch,
    E::OpenPoolLimit,
    E::OwnBoxLimit,
    E::OverrideRequired,
    E::NothingToBuy,
    E::TooManyBoxes,
    E::PoolNotOpen,
    E::PoolNotLocked,
    E::PoolNotDrawn,
    E::SponsorshipTooSmall,
    E::SponsorshipCapExceeded,
    E::VarAlreadySet,
    E::VarNotSet,
    E::VarMismatch,
    E::VarNotEntropy,
    E::VarProviderMismatch,
    E::VarNotFresh,
    E::VarNotRevealed,
    E::VarNotSampledHere,
    E::SampleWindowMissed,
    E::VarFallbackHash,
    E::TooManyVarReplacements,
    E::AlreadyDrawn,
    E::ScoresNotPosted,
    E::WinnerMismatch,
    E::FeeAccountMismatch,
    E::NotReturnable,
    E::FeesAlreadyPaid,
    E::NotSuspended,
    E::NotSplittable,
    E::ReclaimTooEarly,
    E::NotOwner,
    E::NothingToReturn,
    E::SponsorshipsStillOpen,
    E::BoxesStillOutstanding,
    E::PoolNotTerminal,
    E::CounterNotEmpty,
    E::UnsupportedMintExtension,
    E::MathOverflow,
    E::InvalidConfig,
];

#[test]
fn anchor_custom_errors_start_at_6000() {
    assert_eq!(ERROR_CODE_OFFSET, 6000);
}

#[test]
fn the_five_pinned_codes() {
    assert_eq!(E::Unauthorized as u32, 0); // 6000
    assert_eq!(E::InvalidPreset as u32, 14); // 6014
    assert_eq!(E::UnsupportedMintExtension as u32, 57); // 6057
    assert_eq!(E::MathOverflow as u32, 58); // 6058
    assert_eq!(E::InvalidConfig as u32, 59); // 6059
}

#[test]
fn every_error_in_program_section_8_order() {
    for (index, variant) in EXPECTED.iter().enumerate() {
        assert_eq!(
            *variant as u32, index as u32,
            "{variant:?} is not at index {index}"
        );
    }
    assert_eq!(EXPECTED.len(), 60);
}
