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

pub fn create_collect_fees_ix(pool: &Pool, authority: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::CollectFees {}.data(),
        ammq32026::accounts::CollectFees {
            authority,
            mint_x: pool.mint_x,
            mint_y: pool.mint_y,
            config: pool.config,
            treasury: pool.treasury,
            treasury_x: pool.treasury_x,
            treasury_y: pool.treasury_y,
            authority_x: get_associated_token_address(&authority, &pool.mint_x),
            authority_y: get_associated_token_address(&authority, &pool.mint_y),
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}
