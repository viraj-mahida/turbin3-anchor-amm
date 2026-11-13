use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};
use constant_product_curve::{ConstantProduct, LiquidityPair};

use crate::{
    constants::{CONFIG_SEED, MAX_BPS, PRECISION, TREASURY_SEED},
    error::AmmError,
    state::Config,
};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
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
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,
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
        payer = user,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Swap<'info> {
    pub fn swap(
        &mut self,
        is_x: bool,     // true: the user pays token X and receives token Y
        amount_in: u64, // Amount the user pays in
        min_out: u64,   // Minimum amount the user wants to receive
    ) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);
        require_neq!(amount_in, 0, AmmError::InvalidAmount);
        require!(
            self.vault_x.amount > 0 && self.vault_y.amount > 0,
            AmmError::NoLiquidityInPool
        );

        let mut curve = ConstantProduct::init(
            self.vault_x.amount,
            self.vault_y.amount,
            0,
            self.config.fee,
            Some(PRECISION as u8),
        )
        .map_err(AmmError::from)?;

        let pair = if is_x {
            LiquidityPair::X
        } else {
            LiquidityPair::Y
        };

        // The curve takes the fee out of amount_in before pricing the swap
        let result = curve
            .swap_unsafe(pair, amount_in)
            .map_err(AmmError::from)?;

        require_neq!(result.withdraw, 0, AmmError::InvalidAmount);
        require!(result.withdraw >= min_out, AmmError::SlippageExceeded);

        // Treasury gets its share of the fee, the rest of the fee stays in the vault for the LPs
        let treasury_cut = (result.fee as u128)
            .checked_mul(self.config.treasury_share as u128)
            .ok_or(AmmError::Overflow)?
            .checked_div(MAX_BPS as u128)
            .ok_or(AmmError::Overflow)? as u64;
        let to_vault = amount_in
            .checked_sub(treasury_cut)
            .ok_or(AmmError::Underflow)?;

        self.transfer_in(is_x, to_vault, false)?;
        if treasury_cut > 0 {
            self.transfer_in(is_x, treasury_cut, true)?;
        }
        self.transfer_out(is_x, result.withdraw)
    }

    // user -> vault (or treasury) for the token being paid in
    fn transfer_in(&self, is_x: bool, amount: u64, to_treasury: bool) -> Result<()> {
        let (from, vault, treasury) = match is_x {
            true => (
                self.user_x.to_account_info(),
                self.vault_x.to_account_info(),
                self.treasury_x.to_account_info(),
            ),
            false => (
                self.user_y.to_account_info(),
                self.vault_y.to_account_info(),
                self.treasury_y.to_account_info(),
            ),
        };

        let cpi_accounts = Transfer {
            from,
            to: if to_treasury { treasury } else { vault },
            authority: self.user.to_account_info(),
        };

        let ctx = CpiContext::new(self.token_program.key(), cpi_accounts);

        transfer(ctx, amount)
    }

    // vault -> user for the token being paid out
    fn transfer_out(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = match is_x {
            true => (
                self.vault_y.to_account_info(),
                self.user_y.to_account_info(),
            ),
            false => (
                self.vault_x.to_account_info(),
                self.user_x.to_account_info(),
            ),
        };

        let cpi_accounts = Transfer {
            from,
            to,
            authority: self.config.to_account_info(),
        };

        let signer_seeds: &[&[&[u8]]] = &[&[
            CONFIG_SEED,
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ]];

        let ctx = CpiContext::new_with_signer(self.token_program.key(), cpi_accounts, signer_seeds);

        transfer(ctx, amount)
    }
}
