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

// precision used by the curve library for the deposit / withdraw ratios
pub const PRECISION: u32 = 6;
