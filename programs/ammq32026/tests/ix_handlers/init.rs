use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program::ID as SYSTEM_PROGRAM_ID,
        InstructionData, ToAccountMetas,
    },
    anchor_spl::associated_token::ID as ASSOCIATED_TOKEN_PROGRAM_ID,
    litesvm_token::spl_token::ID as TOKEN_PROGRAM_ID,
    solana_pubkey::Pubkey,
};

use crate::Pool;

pub fn create_initialize_ix(
    pool: &Pool,
    initializer: Pubkey,
    fee: u16,
    treasury_share: u16,
    authority: Option<Pubkey>,
) -> Instruction {
    Instruction::new_with_bytes(
        ammq32026::id(),
        &ammq32026::instruction::Initialize {
            seed: pool.seed,
            fee,
            treasury_share,
            authority,
        }
        .data(),
        ammq32026::accounts::Initialize {
            initializer,
            mint_x: pool.mint_x,
            mint_y: pool.mint_y,
            mint_lp: pool.mint_lp,
            vault_x: pool.vault_x,
            vault_y: pool.vault_y,
            treasury: pool.treasury,
            treasury_x: pool.treasury_x,
            treasury_y: pool.treasury_y,
            config: pool.config,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}
