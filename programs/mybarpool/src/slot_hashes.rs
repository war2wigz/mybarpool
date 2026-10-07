//! The SlotHashes sysvar, read as an account (PROGRAM §4.4 `sample_var` and
//! §6.1 `slothash`; Mollusk and Surfpool both supply it). The `sol_get_sysvar`
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

/// Bytes per SlotHashes entry: a `u64` slot and a 32-byte hash.
const ENTRY_LEN: usize = 40;

/// The sysvar's hash for `slot`, if it still holds one (PROGRAM §4.4 `sample_var`). Entries are
/// 40 bytes each from offset 8, newest first, so the scan stops once an entry's slot is below
/// the target; it is bounded by the sysvar's own 512-entry cap and `count` is trusted only as
/// far as the data length allows. `Ok(None)` when absent. A short account is
/// `AccountDidNotDeserialize`.
pub fn hash_for_slot(account: &AccountInfo, slot: u64) -> Result<Option<[u8; 32]>> {
    require_keys_eq!(
        *account.key,
        sysvar::slot_hashes::ID,
        ErrorCode::ConstraintAddress
    );
    let data = account.try_borrow_data()?;
    let mut count_bytes = [0u8; 8];
    count_bytes.copy_from_slice(data.get(0..8).ok_or(ErrorCode::AccountDidNotDeserialize)?);
    let declared = usize::try_from(u64::from_le_bytes(count_bytes))
        .map_err(|_| ErrorCode::AccountDidNotDeserialize)?;
    let available = data
        .len()
        .checked_sub(8)
        .ok_or(ErrorCode::AccountDidNotDeserialize)?
        / ENTRY_LEN;
    let count = declared.min(available);
    for i in 0..count {
        let at = 8 + i * ENTRY_LEN;
        let entry = data
            .get(at..at + ENTRY_LEN)
            .ok_or(ErrorCode::AccountDidNotDeserialize)?;
        let mut slot_bytes = [0u8; 8];
        slot_bytes.copy_from_slice(&entry[0..8]);
        let entry_slot = u64::from_le_bytes(slot_bytes);
        if entry_slot == slot {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&entry[8..40]);
            return Ok(Some(hash));
        }
        if entry_slot < slot {
            break;
        }
    }
    Ok(None)
}
