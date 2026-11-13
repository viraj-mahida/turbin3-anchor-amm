use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program::ID as SYSTEM_PROGRAM_ID,
        InstructionData, ToAccountMetas,
    },
    anchor_spl::associated_token::{
        get_associated_token_address, ID as ASSOCIATED_TOKEN_PROGRAM_ID,
    },
    litesvm_token::spl_token::ID as TOKEN_PROGRAM_ID,
    solana_pubkey::Pubkey,
};

use crate::Pool;

pub fn create_swap_ix(
    pool: &Pool,
    user: Pubkey,
    is_x: bool,
    amount_in: u64,
    min_out: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Swap {
            is_x,
            amount_in,
            min_out,
        }
        .data(),
        ammq32026::accounts::Swap {
            user,
            mint_x: pool.mint_x,
            mint_y: pool.mint_y,
            config: pool.config,
            vault_x: pool.vault_x,
            vault_y: pool.vault_y,
            treasury: pool.treasury,
            treasury_x: pool.treasury_x,
            treasury_y: pool.treasury_y,
            user_x: get_associated_token_address(&user, &pool.mint_x),
            user_y: get_associated_token_address(&user, &pool.mint_y),
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}
