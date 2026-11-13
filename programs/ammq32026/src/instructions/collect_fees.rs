use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};

use crate::{
    constants::{CONFIG_SEED, TREASURY_SEED},
    error::AmmError,
    state::Config,
};

#[derive(Accounts)]
pub struct CollectFees<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,
    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [CONFIG_SEED, config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [TREASURY_SEED, config.key().as_ref()],
        bump = config.treasury_bump,
    )]
    pub treasury: SystemAccount<'info>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = treasury,
    )]
    pub treasury_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = treasury,
    )]
    pub treasury_y: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = mint_x,
        associated_token::authority = authority,
    )]
    pub authority_x: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = mint_y,
        associated_token::authority = authority,
    )]
    pub authority_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> CollectFees<'info> {
    pub fn collect_fees(&mut self) -> Result<()> {
        let authority = self.config.authority.ok_or(AmmError::NoAuthoritySet)?;
        require_keys_eq!(authority, self.authority.key(), AmmError::InvalidAuthority);

        let amount_x = self.treasury_x.amount;
        let amount_y = self.treasury_y.amount;
        require!(amount_x > 0 || amount_y > 0, AmmError::NothingToCollect);

        if amount_x > 0 {
            self.collect(true, amount_x)?;
        }
        if amount_y > 0 {
            self.collect(false, amount_y)?;
        }
        Ok(())
    }

    // treasury -> authority, signed by the treasury PDA
    fn collect(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = match is_x {
            true => (
                self.treasury_x.to_account_info(),
                self.authority_x.to_account_info(),
            ),
            false => (
                self.treasury_y.to_account_info(),
                self.authority_y.to_account_info(),
            ),
        };

        let cpi_accounts = Transfer {
            from,
            to,
            authority: self.treasury.to_account_info(),
        };

        let config_key = self.config.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            TREASURY_SEED,
            config_key.as_ref(),
            &[self.config.treasury_bump],
        ]];

        let ctx = CpiContext::new_with_signer(self.token_program.key(), cpi_accounts, signer_seeds);

        transfer(ctx, amount)
    }
}
