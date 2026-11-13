use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::{
    constants::{CONFIG_SEED, LP_SEED, MAX_BPS, TREASURY_SEED},
    error::AmmError,
    state::Config,
};

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub initializer: Signer<'info>,
    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,
    #[account(
        init,
        payer = initializer,
        seeds = [LP_SEED, config.key().as_ref()],
        bump,
        mint::decimals = 6,
        mint::authority = config,
    )]
    pub mint_lp: Account<'info, Mint>,
    #[account(
        init,
        payer = initializer,
        associated_token::mint = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,
    #[account(
        init,
        payer = initializer,
        associated_token::mint = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,
    #[account(
        seeds = [TREASURY_SEED, config.key().as_ref()],
        bump,
    )]
    pub treasury: SystemAccount<'info>,
    #[account(
        init,
        payer = initializer,
        associated_token::mint = mint_x,
        associated_token::authority = treasury,
    )]
    pub treasury_x: Box<Account<'info, TokenAccount>>,
    #[account(
        init,
        payer = initializer,
        associated_token::mint = mint_y,
        associated_token::authority = treasury,
    )]
    pub treasury_y: Box<Account<'info, TokenAccount>>,
    #[account(
        init,
        payer = initializer,
        seeds = [CONFIG_SEED, seed.to_le_bytes().as_ref()],
        bump,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
    )]
    pub config: Account<'info, Config>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn init(
        &mut self,
        seed: u64,
        fee: u16,
        treasury_share: u16,
        authority: Option<Pubkey>,
        bumps: InitializeBumps,
    ) -> Result<()> {
        require!(fee <= MAX_BPS, AmmError::InvalidFee);
        require!(treasury_share <= MAX_BPS, AmmError::InvalidFee);
        require_keys_neq!(self.mint_x.key(), self.mint_y.key(), AmmError::InvalidToken);

        self.config.set_inner(Config {
            seed,
            authority,
            mint_x: self.mint_x.key(),
            mint_y: self.mint_y.key(),
            fee,
            treasury_share,
            locked: false,
            config_bump: bumps.config,
            lp_bump: bumps.mint_lp,
            treasury_bump: bumps.treasury,
        });

        Ok(())
    }
}
