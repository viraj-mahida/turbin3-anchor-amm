use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const LP_SEED: &[u8] = b"lp";

#[constant]
pub const TREASURY_SEED: &[u8] = b"treasury";

// 100% in basis points
#[constant]
pub const MAX_BPS: u16 = 10_000;

// Scale the curve library uses for the deposit / withdraw ratios. It is the
// multiplier itself (1e6), not the number of decimals
pub const PRECISION: u32 = 1_000_000;
