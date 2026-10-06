//! The SlotHashes sysvar, read as an account (PROGRAM §4.3 lists it in the
//! account lists; Mollusk and Surfpool both supply it). The `sol_get_sysvar`
//! syscall path is deliberately not used.

use anchor_lang::prelude::*;
use solana_sdk_ids::sysvar;

/// The hash of the most recent SlotHashes entry (PROGRAM §6.1 `slothash`). The account data is
/// a bincode `Vec<(u64, [u8; 32])>`, newest first: entry count at 0..8 (must be ≥ 1), the
/// newest slot at 8..16, its hash at 16..48. A short account is `AccountDidNotDeserialize`.
pub fn most_recent_slot_hash(account: &AccountInfo) -> Result<[u8; 32]> {
    require_keys_eq!(
        *account.key,
        sysvar::slot_hashes::ID,
        ErrorCode::ConstraintAddress
    );
    let data = account.try_borrow_data()?;
    let mut count = [0u8; 8];
    count.copy_from_slice(data.get(0..8).ok_or(ErrorCode::AccountDidNotDeserialize)?);
    require!(
        u64::from_le_bytes(count) >= 1,
        ErrorCode::AccountDidNotDeserialize
    );
    let mut hash = [0u8; 32];
    hash.copy_from_slice(
        data.get(16..48)
            .ok_or(ErrorCode::AccountDidNotDeserialize)?,
    );
    Ok(hash)
}
