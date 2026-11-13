use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    solana_pubkey::Pubkey,
};

use crate::Pool;

pub fn create_set_lock_ix(pool: &Pool, authority: Pubkey, locked: bool) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::SetLock { locked }.data(),
        ammq32026::accounts::SetLock {
            authority,
            config: pool.config,
        }
        .to_account_metas(None),
    )
}
