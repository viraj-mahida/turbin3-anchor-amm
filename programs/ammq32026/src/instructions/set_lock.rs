use anchor_lang::prelude::*;

use crate::{constants::CONFIG_SEED, error::AmmError, state::Config};

#[derive(Accounts)]
pub struct SetLock<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED, config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
}

impl<'info> SetLock<'info> {
    pub fn set_lock(&mut self, locked: bool) -> Result<()> {
        let authority = self.config.authority.ok_or(AmmError::NoAuthoritySet)?;
        require_keys_eq!(authority, self.authority.key(), AmmError::InvalidAuthority);

        self.config.locked = locked;
        Ok(())
    }
}
