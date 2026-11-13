use {
    anchor_lang::AccountDeserialize,
    anchor_spl::{
        associated_token::get_associated_token_address,
        token::{Mint, TokenAccount},
    },
    litesvm::LiteSVM,
    litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo},
    solana_keypair::Keypair,
    solana_message::{Instruction, Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

use ammq32026::{error::AmmError, Config};

mod ix_handlers;
use ix_handlers::*;

const SEED: u64 = 123;
const FEE: u16 = 30; // 0.30%
const TREASURY_SHARE: u16 = 2_000; // 20% of the fee goes to the treasury

// Opening liquidity used by most tests: 100 X and 200 Y for 100 LP (6 decimals)
const LP_AMOUNT: u64 = 100_000_000;
const INITIAL_X: u64 = 100_000_000;
const INITIAL_Y: u64 = 200_000_000;

// All the addresses that belong to one pool
struct Pool {
    seed: u64,
    mint_x: Pubkey,
    mint_y: Pubkey,
    config: Pubkey,
    mint_lp: Pubkey,
    vault_x: Pubkey,
    vault_y: Pubkey,
    treasury: Pubkey,
    treasury_x: Pubkey,
    treasury_y: Pubkey,
}

impl Pool {
    fn new(seed: u64, mint_x: Pubkey, mint_y: Pubkey) -> Self {
        let program_id = ammq32026::id();
        let config = Pubkey::find_program_address(&[b"config", &seed.to_le_bytes()], &program_id).0;
        let mint_lp = Pubkey::find_program_address(&[b"lp", config.as_ref()], &program_id).0;
        let treasury = Pubkey::find_program_address(&[b"treasury", config.as_ref()], &program_id).0;

        Pool {
            seed,
            mint_x,
            mint_y,
            config,
            mint_lp,
            vault_x: get_associated_token_address(&config, &mint_x),
            vault_y: get_associated_token_address(&config, &mint_y),
            treasury,
            treasury_x: get_associated_token_address(&treasury, &mint_x),
            treasury_y: get_associated_token_address(&treasury, &mint_y),
        }
    }
}

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> litesvm::types::TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

// Loads the program, creates the two mints and gives the admin some of each
fn setup() -> (LiteSVM, Keypair, Pool) {
    let program_id = ammq32026::id();
    let admin = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/ammq32026.so"
    ));

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();

    let mint_x = CreateMint::new(&mut svm, &admin)
        .decimals(6)
        .authority(&admin.pubkey())
        .send()
        .unwrap();
    let mint_y = CreateMint::new(&mut svm, &admin)
        .decimals(6)
        .authority(&admin.pubkey())
        .send()
        .unwrap();

    let pool = Pool::new(SEED, mint_x, mint_y);
    fund(
        &mut svm,
        &admin,
        &admin.pubkey(),
        &pool,
        1_000_000_000,
        1_000_000_000,
    );

    (svm, admin, pool)
}

// Creates the owner's X and Y token accounts and mints some tokens into them
fn fund(svm: &mut LiteSVM, admin: &Keypair, owner: &Pubkey, pool: &Pool, x: u64, y: u64) {
    for (mint, amount) in [(pool.mint_x, x), (pool.mint_y, y)] {
        let ata = CreateAssociatedTokenAccount::new(svm, admin, &mint)
            .owner(owner)
            .send()
            .unwrap();
        MintTo::new(svm, admin, &mint, &ata, amount).send().unwrap();
    }
}

// A fresh wallet with SOL and some X and Y
fn new_user(svm: &mut LiteSVM, admin: &Keypair, pool: &Pool, x: u64, y: u64) -> Keypair {
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    fund(svm, admin, &user.pubkey(), pool, x, y);
    user
}

// Pool with the default fee, the admin as authority, and no liquidity yet
fn setup_pool() -> (LiteSVM, Keypair, Pool) {
    let (mut svm, admin, pool) = setup();
    let ix = create_initialize_ix(
        &pool,
        admin.pubkey(),
        FEE,
        TREASURY_SHARE,
        Some(admin.pubkey()),
    );
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();
    (svm, admin, pool)
}

// Pool that already holds the opening liquidity, all the LP tokens are with the admin
fn setup_with_liquidity() -> (LiteSVM, Keypair, Pool) {
    let (mut svm, admin, pool) = setup_pool();
    let ix = create_deposit_ix(&pool, admin.pubkey(), LP_AMOUNT, INITIAL_X, INITIAL_Y);
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();
    (svm, admin, pool)
}

fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let account = svm.get_account(ata).expect("token account should exist");
    TokenAccount::try_deserialize(&mut account.data.as_slice())
        .unwrap()
        .amount
}

fn lp_supply(svm: &LiteSVM, pool: &Pool) -> u64 {
    let account = svm.get_account(&pool.mint_lp).unwrap();
    Mint::try_deserialize(&mut account.data.as_slice())
        .unwrap()
        .supply
}

fn read_config(svm: &LiteSVM, pool: &Pool) -> Config {
    let account = svm.get_account(&pool.config).unwrap();
    Config::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn balance_of(svm: &LiteSVM, owner: &Pubkey, mint: &Pubkey) -> u64 {
    token_balance(svm, &get_associated_token_address(owner, mint))
}

// Same maths as the curve: fee comes off the input, then x * y = k decides the output
fn expected_swap(reserve_in: u64, reserve_out: u64, amount_in: u64) -> (u64, u64) {
    let after_fee = (amount_in as u128 * (10_000 - FEE as u128) / 10_000) as u64;
    let k = reserve_in as u128 * reserve_out as u128;
    let new_out = k / (reserve_in as u128 + after_fee as u128);
    let amount_out = reserve_out - new_out as u64;
    let fee = amount_in - after_fee;
    (amount_out, fee)
}

fn assert_amm_error(res: litesvm::types::TransactionResult, error: AmmError) {
    let failed = res.expect_err("transaction should have failed");
    let code = 6000 + error as u32;
    let msg = format!("{:?}", failed.err);
    assert!(
        msg.contains(&format!("Custom({code})")),
        "expected {error:?} (Custom({code})), got {msg}"
    );
}

// ---------------------------------------------------------------- initialize

#[test]
fn initialize_creates_pool_and_treasury() {
    let (svm, admin, pool) = setup_pool();

    let config = read_config(&svm, &pool);
    assert_eq!(config.seed, SEED);
    assert_eq!(config.authority, Some(admin.pubkey()));
    assert_eq!(config.mint_x, pool.mint_x);
    assert_eq!(config.mint_y, pool.mint_y);
    assert_eq!(config.fee, FEE);
    assert_eq!(config.treasury_share, TREASURY_SHARE);
    assert!(!config.locked);

    // vaults and treasury accounts exist, are empty and belong to the right PDAs
    for (ata, owner) in [
        (pool.vault_x, pool.config),
        (pool.vault_y, pool.config),
        (pool.treasury_x, pool.treasury),
        (pool.treasury_y, pool.treasury),
    ] {
        let account = svm.get_account(&ata).unwrap();
        let token = TokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap();
        assert_eq!(token.owner, owner);
        assert_eq!(token.amount, 0);
    }

    // LP mint is controlled by the config PDA and starts at zero
    let account = svm.get_account(&pool.mint_lp).unwrap();
    let mint = Mint::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(mint.mint_authority.unwrap(), pool.config);
    assert_eq!(mint.supply, 0);
}

#[test]
fn initialize_with_fee_above_100_percent_fails() {
    let (mut svm, admin, pool) = setup();
    let ix = create_initialize_ix(&pool, admin.pubkey(), 10_001, TREASURY_SHARE, None);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::InvalidFee,
    );
}

#[test]
fn initialize_with_treasury_share_above_100_percent_fails() {
    let (mut svm, admin, pool) = setup();
    let ix = create_initialize_ix(&pool, admin.pubkey(), FEE, 10_001, None);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::InvalidFee,
    );
}

#[test]
fn initialize_twice_with_same_seed_fails() {
    let (mut svm, admin, pool) = setup_pool();
    let ix = create_initialize_ix(&pool, admin.pubkey(), 100, 0, None);
    assert!(send(&mut svm, &[ix], &admin, &[&admin]).is_err());

    // the original settings are still there
    assert_eq!(read_config(&svm, &pool).fee, FEE);
}

#[test]
fn initialize_without_authority_works() {
    let (mut svm, admin, pool) = setup();
    let ix = create_initialize_ix(&pool, admin.pubkey(), FEE, TREASURY_SHARE, None);
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();
    assert_eq!(read_config(&svm, &pool).authority, None);
}

// ------------------------------------------------------------------- deposit

#[test]
fn first_deposit_sets_the_price_and_mints_lp() {
    let (svm, admin, pool) = setup_with_liquidity();

    assert_eq!(token_balance(&svm, &pool.vault_x), INITIAL_X);
    assert_eq!(token_balance(&svm, &pool.vault_y), INITIAL_Y);
    assert_eq!(lp_supply(&svm, &pool), LP_AMOUNT);
    assert_eq!(balance_of(&svm, &admin.pubkey(), &pool.mint_lp), LP_AMOUNT);
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_x),
        1_000_000_000 - INITIAL_X
    );
}

#[test]
fn second_deposit_is_proportional_to_the_pool() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let lp = new_user(&mut svm, &admin, &pool, 500_000_000, 500_000_000);

    // 50 LP is half of the existing supply, so it needs half of each reserve
    let ix = create_deposit_ix(&pool, lp.pubkey(), 50_000_000, 60_000_000, 110_000_000);
    send(&mut svm, &[ix], &lp, &[&lp]).unwrap();

    assert_eq!(token_balance(&svm, &pool.vault_x), 150_000_000);
    assert_eq!(token_balance(&svm, &pool.vault_y), 300_000_000);
    assert_eq!(lp_supply(&svm, &pool), 150_000_000);
    assert_eq!(balance_of(&svm, &lp.pubkey(), &pool.mint_lp), 50_000_000);
    assert_eq!(balance_of(&svm, &lp.pubkey(), &pool.mint_x), 450_000_000);
    assert_eq!(balance_of(&svm, &lp.pubkey(), &pool.mint_y), 400_000_000);
}

#[test]
fn deposit_over_the_max_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let lp = new_user(&mut svm, &admin, &pool, 500_000_000, 500_000_000);

    // needs 50 X and 100 Y but the user only allows 40 X
    let ix = create_deposit_ix(&pool, lp.pubkey(), 50_000_000, 40_000_000, 110_000_000);
    assert_amm_error(
        send(&mut svm, &[ix], &lp, &[&lp]),
        AmmError::SlippageExceeded,
    );

    assert_eq!(token_balance(&svm, &pool.vault_x), INITIAL_X);
    assert_eq!(lp_supply(&svm, &pool), LP_AMOUNT);
}

#[test]
fn deposit_zero_lp_fails() {
    let (mut svm, admin, pool) = setup_pool();
    let ix = create_deposit_ix(&pool, admin.pubkey(), 0, INITIAL_X, INITIAL_Y);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::InvalidAmount,
    );
}

#[test]
fn first_deposit_needs_both_tokens() {
    let (mut svm, admin, pool) = setup_pool();
    let ix = create_deposit_ix(&pool, admin.pubkey(), LP_AMOUNT, INITIAL_X, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::InvalidAmount,
    );
}

#[test]
fn deposit_into_locked_pool_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let lock = create_set_lock_ix(&pool, admin.pubkey(), true);
    send(&mut svm, &[lock], &admin, &[&admin]).unwrap();

    let ix = create_deposit_ix(&pool, admin.pubkey(), 10_000_000, 20_000_000, 40_000_000);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::PoolLocked,
    );
}

// ------------------------------------------------------------------ withdraw

#[test]
fn withdraw_returns_a_share_of_the_pool() {
    let (mut svm, admin, pool) = setup_with_liquidity();

    // burning 25 of 100 LP gives back a quarter of each reserve
    let ix = create_withdraw_ix(&pool, admin.pubkey(), 25_000_000, 25_000_000, 50_000_000);
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();

    assert_eq!(token_balance(&svm, &pool.vault_x), 75_000_000);
    assert_eq!(token_balance(&svm, &pool.vault_y), 150_000_000);
    assert_eq!(lp_supply(&svm, &pool), 75_000_000);
    assert_eq!(balance_of(&svm, &admin.pubkey(), &pool.mint_lp), 75_000_000);
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_x),
        1_000_000_000 - INITIAL_X + 25_000_000
    );
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_y),
        1_000_000_000 - INITIAL_Y + 50_000_000
    );
}

#[test]
fn withdrawing_all_lp_empties_the_pool() {
    let (mut svm, admin, pool) = setup_with_liquidity();

    let ix = create_withdraw_ix(&pool, admin.pubkey(), LP_AMOUNT, INITIAL_X, INITIAL_Y);
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();

    assert_eq!(token_balance(&svm, &pool.vault_x), 0);
    assert_eq!(token_balance(&svm, &pool.vault_y), 0);
    assert_eq!(lp_supply(&svm, &pool), 0);
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_x),
        1_000_000_000
    );
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_y),
        1_000_000_000
    );
}

#[test]
fn withdraw_under_the_min_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();

    // 25 LP is worth 25 X and 50 Y, the user asks for 26 X
    let ix = create_withdraw_ix(&pool, admin.pubkey(), 25_000_000, 26_000_000, 50_000_000);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::SlippageExceeded,
    );

    assert_eq!(lp_supply(&svm, &pool), LP_AMOUNT);
}

#[test]
fn withdraw_zero_lp_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let ix = create_withdraw_ix(&pool, admin.pubkey(), 0, 0, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::InvalidAmount,
    );
}

#[test]
fn withdraw_more_than_the_supply_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let ix = create_withdraw_ix(&pool, admin.pubkey(), LP_AMOUNT + 1, 0, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::InsufficientBalance,
    );
}

#[test]
fn withdraw_more_lp_than_the_user_owns_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let lp = new_user(&mut svm, &admin, &pool, 500_000_000, 500_000_000);
    let deposit = create_deposit_ix(&pool, lp.pubkey(), 10_000_000, 20_000_000, 40_000_000);
    send(&mut svm, &[deposit], &lp, &[&lp]).unwrap();

    // the other LP holds 100 LP, this user only 10
    let ix = create_withdraw_ix(&pool, lp.pubkey(), 20_000_000, 0, 0);
    assert!(send(&mut svm, &[ix], &lp, &[&lp]).is_err());
    assert_eq!(balance_of(&svm, &lp.pubkey(), &pool.mint_lp), 10_000_000);
}

#[test]
fn withdraw_from_locked_pool_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let lock = create_set_lock_ix(&pool, admin.pubkey(), true);
    send(&mut svm, &[lock], &admin, &[&admin]).unwrap();

    let ix = create_withdraw_ix(&pool, admin.pubkey(), 25_000_000, 0, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::PoolLocked,
    );
}

// ---------------------------------------------------------------------- swap

#[test]
fn swap_x_for_y() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    let amount_in = 10_000_000;
    let (amount_out, fee) = expected_swap(INITIAL_X, INITIAL_Y, amount_in);
    let to_treasury = fee * TREASURY_SHARE as u64 / 10_000;

    let ix = create_swap_ix(&pool, trader.pubkey(), true, amount_in, amount_out);
    send(&mut svm, &[ix], &trader, &[&trader]).unwrap();

    assert_eq!(balance_of(&svm, &trader.pubkey(), &pool.mint_x), 40_000_000);
    assert_eq!(balance_of(&svm, &trader.pubkey(), &pool.mint_y), amount_out);
    assert_eq!(
        token_balance(&svm, &pool.vault_x),
        INITIAL_X + amount_in - to_treasury
    );
    assert_eq!(token_balance(&svm, &pool.vault_y), INITIAL_Y - amount_out);
    assert_eq!(token_balance(&svm, &pool.treasury_x), to_treasury);
    assert_eq!(token_balance(&svm, &pool.treasury_y), 0);
}

#[test]
fn swap_y_for_x() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 0, 50_000_000);

    let amount_in = 20_000_000;
    let (amount_out, fee) = expected_swap(INITIAL_Y, INITIAL_X, amount_in);
    let to_treasury = fee * TREASURY_SHARE as u64 / 10_000;

    let ix = create_swap_ix(&pool, trader.pubkey(), false, amount_in, amount_out);
    send(&mut svm, &[ix], &trader, &[&trader]).unwrap();

    assert_eq!(balance_of(&svm, &trader.pubkey(), &pool.mint_y), 30_000_000);
    assert_eq!(balance_of(&svm, &trader.pubkey(), &pool.mint_x), amount_out);
    assert_eq!(token_balance(&svm, &pool.vault_x), INITIAL_X - amount_out);
    assert_eq!(
        token_balance(&svm, &pool.vault_y),
        INITIAL_Y + amount_in - to_treasury
    );
    assert_eq!(token_balance(&svm, &pool.treasury_y), to_treasury);
    assert_eq!(token_balance(&svm, &pool.treasury_x), 0);
}

#[test]
fn swap_fee_is_split_between_treasury_and_lps() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    // 10 X in: fee is 0.30% = 30_000, 20% of it = 6_000 for the treasury
    let ix = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    send(&mut svm, &[ix], &trader, &[&trader]).unwrap();

    assert_eq!(token_balance(&svm, &pool.treasury_x), 6_000);
    // the other 24_000 stays in the vault on top of the swapped amount
    assert_eq!(
        token_balance(&svm, &pool.vault_x),
        INITIAL_X + 10_000_000 - 6_000
    );
}

#[test]
fn swap_never_lowers_the_invariant() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 100_000_000, 100_000_000);
    let k_before = INITIAL_X as u128 * INITIAL_Y as u128;

    let a = create_swap_ix(&pool, trader.pubkey(), true, 30_000_000, 0);
    send(&mut svm, &[a], &trader, &[&trader]).unwrap();
    let b = create_swap_ix(&pool, trader.pubkey(), false, 15_000_000, 0);
    send(&mut svm, &[b], &trader, &[&trader]).unwrap();

    let k_after =
        token_balance(&svm, &pool.vault_x) as u128 * token_balance(&svm, &pool.vault_y) as u128;
    assert!(k_after > k_before);
}

#[test]
fn swap_with_zero_treasury_share_keeps_the_whole_fee_in_the_pool() {
    let (mut svm, admin, pool) = setup();
    let ix = create_initialize_ix(&pool, admin.pubkey(), FEE, 0, Some(admin.pubkey()));
    let deposit = create_deposit_ix(&pool, admin.pubkey(), LP_AMOUNT, INITIAL_X, INITIAL_Y);
    send(&mut svm, &[ix, deposit], &admin, &[&admin]).unwrap();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    let swap = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    send(&mut svm, &[swap], &trader, &[&trader]).unwrap();

    assert_eq!(token_balance(&svm, &pool.treasury_x), 0);
    assert_eq!(token_balance(&svm, &pool.vault_x), INITIAL_X + 10_000_000);
}

#[test]
fn swap_below_min_out_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    let (amount_out, _) = expected_swap(INITIAL_X, INITIAL_Y, 10_000_000);
    let ix = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, amount_out + 1);
    assert_amm_error(
        send(&mut svm, &[ix], &trader, &[&trader]),
        AmmError::SlippageExceeded,
    );

    // nothing moved
    assert_eq!(balance_of(&svm, &trader.pubkey(), &pool.mint_x), 50_000_000);
    assert_eq!(token_balance(&svm, &pool.vault_x), INITIAL_X);
    assert_eq!(token_balance(&svm, &pool.treasury_x), 0);
}

#[test]
fn swap_zero_amount_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    let ix = create_swap_ix(&pool, trader.pubkey(), true, 0, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &trader, &[&trader]),
        AmmError::InvalidAmount,
    );
}

#[test]
fn swap_without_enough_tokens_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 1_000_000, 0);

    let ix = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    assert!(send(&mut svm, &[ix], &trader, &[&trader]).is_err());
    assert_eq!(token_balance(&svm, &pool.vault_y), INITIAL_Y);
}

#[test]
fn swap_on_empty_pool_fails() {
    let (mut svm, admin, pool) = setup_pool();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    let ix = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &trader, &[&trader]),
        AmmError::NoLiquidityInPool,
    );
}

#[test]
fn swap_on_locked_pool_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);
    let lock = create_set_lock_ix(&pool, admin.pubkey(), true);
    send(&mut svm, &[lock], &admin, &[&admin]).unwrap();

    let ix = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    assert_amm_error(
        send(&mut svm, &[ix], &trader, &[&trader]),
        AmmError::PoolLocked,
    );
}

// ------------------------------------------------------------------ set_lock

#[test]
fn authority_can_lock_and_unlock_the_pool() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);

    let lock = create_set_lock_ix(&pool, admin.pubkey(), true);
    send(&mut svm, &[lock], &admin, &[&admin]).unwrap();
    assert!(read_config(&svm, &pool).locked);

    let unlock = create_set_lock_ix(&pool, admin.pubkey(), false);
    send(&mut svm, &[unlock], &admin, &[&admin]).unwrap();
    assert!(!read_config(&svm, &pool).locked);

    // swaps work again after unlocking
    let swap = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    send(&mut svm, &[swap], &trader, &[&trader]).unwrap();
}

#[test]
fn set_lock_by_someone_else_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let mallory = new_user(&mut svm, &admin, &pool, 0, 0);

    let ix = create_set_lock_ix(&pool, mallory.pubkey(), true);
    assert_amm_error(
        send(&mut svm, &[ix], &mallory, &[&mallory]),
        AmmError::InvalidAuthority,
    );
    assert!(!read_config(&svm, &pool).locked);
}

#[test]
fn set_lock_on_pool_without_authority_fails() {
    let (mut svm, admin, pool) = setup();
    let init = create_initialize_ix(&pool, admin.pubkey(), FEE, TREASURY_SHARE, None);
    send(&mut svm, &[init], &admin, &[&admin]).unwrap();

    let ix = create_set_lock_ix(&pool, admin.pubkey(), true);
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::NoAuthoritySet,
    );
}

// -------------------------------------------------------------- collect_fees

#[test]
fn collect_fees_moves_the_treasury_to_the_authority() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 50_000_000);

    // one swap each way so the treasury holds both tokens
    let a = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    send(&mut svm, &[a], &trader, &[&trader]).unwrap();
    let b = create_swap_ix(&pool, trader.pubkey(), false, 20_000_000, 0);
    send(&mut svm, &[b], &trader, &[&trader]).unwrap();

    let fee_x = token_balance(&svm, &pool.treasury_x);
    let fee_y = token_balance(&svm, &pool.treasury_y);
    assert_eq!(fee_x, 6_000);
    assert_eq!(fee_y, 12_000);

    let x_before = balance_of(&svm, &admin.pubkey(), &pool.mint_x);
    let y_before = balance_of(&svm, &admin.pubkey(), &pool.mint_y);

    let ix = create_collect_fees_ix(&pool, admin.pubkey());
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();

    assert_eq!(token_balance(&svm, &pool.treasury_x), 0);
    assert_eq!(token_balance(&svm, &pool.treasury_y), 0);
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_x),
        x_before + fee_x
    );
    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_y),
        y_before + fee_y
    );
}

#[test]
fn collect_fees_works_with_only_one_token_in_the_treasury() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);
    let swap = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    send(&mut svm, &[swap], &trader, &[&trader]).unwrap();

    let x_before = balance_of(&svm, &admin.pubkey(), &pool.mint_x);
    let ix = create_collect_fees_ix(&pool, admin.pubkey());
    send(&mut svm, &[ix], &admin, &[&admin]).unwrap();

    assert_eq!(
        balance_of(&svm, &admin.pubkey(), &pool.mint_x),
        x_before + 6_000
    );
}

#[test]
fn collect_fees_by_someone_else_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let trader = new_user(&mut svm, &admin, &pool, 50_000_000, 0);
    let swap = create_swap_ix(&pool, trader.pubkey(), true, 10_000_000, 0);
    send(&mut svm, &[swap], &trader, &[&trader]).unwrap();

    let ix = create_collect_fees_ix(&pool, trader.pubkey());
    assert_amm_error(
        send(&mut svm, &[ix], &trader, &[&trader]),
        AmmError::InvalidAuthority,
    );
    assert_eq!(token_balance(&svm, &pool.treasury_x), 6_000);
}

#[test]
fn collect_fees_with_an_empty_treasury_fails() {
    let (mut svm, admin, pool) = setup_with_liquidity();
    let ix = create_collect_fees_ix(&pool, admin.pubkey());
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::NothingToCollect,
    );
}

#[test]
fn collect_fees_on_pool_without_authority_fails() {
    let (mut svm, admin, pool) = setup();
    let init = create_initialize_ix(&pool, admin.pubkey(), FEE, TREASURY_SHARE, None);
    send(&mut svm, &[init], &admin, &[&admin]).unwrap();

    let ix = create_collect_fees_ix(&pool, admin.pubkey());
    assert_amm_error(
        send(&mut svm, &[ix], &admin, &[&admin]),
        AmmError::NoAuthoritySet,
    );
}
