//! Synterra: seed the liveness of agentic swarms, the way BitTorrent seeds files.
//!
//! Hackathon build on Solana devnet. Three pieces:
//!
//! 1. **Seeders** register once and submit signed liveness receipts: one per completed agent step,
//!    each carrying a hash of the step's output. Receipts are events, not accounts, so a laptop can
//!    submit them for a fraction of a cent.
//! 2. **Verification** is optimistic. A verifier re-runs a random sample of steps off-chain and, when
//!    they agree, signs a batch that mints devnet SYN to the seeder. There is no puzzle to win and no
//!    hashrate race: provision is the qualifying act.
//! 3. **The Align wall**: anyone declares alignment with one transaction, one declaration per wallet,
//!    readable by any other program.
//!
//! DEMO PARAMETERS, NOT TOKENOMICS. The reward per verified step is set at initialisation for the demo.
//! SYN has no emission curve, halving interval or supply cap yet, and nothing here should be read as one.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

declare_id!("J8H5nv3Wx6JHMmHjCFvxm84LdWmhY6HBY18frzD43KJD");

/// Devnet SYN uses 6 decimals.
pub const SYN_DECIMALS: u8 = 6;
/// Longest Align wall declaration, in bytes.
pub const MAX_DECLARATION: usize = 140;
/// Most steps a single verification batch may reward.
pub const MAX_BATCH_STEPS: u32 = 10_000;

#[program]
pub mod synterra {
    use super::*;

    /// One-time setup: records the admin and verifier and creates the devnet SYN mint,
    /// whose mint authority is the config PDA (so only verified batches can mint).
    pub fn initialize(ctx: Context<Initialize>, verifier: Pubkey, reward_per_step: u64) -> Result<()> {
        require!(reward_per_step > 0, SynterraError::ZeroReward);
        let config = &mut ctx.accounts.config;
        config.admin = ctx.accounts.admin.key();
        config.verifier = verifier;
        config.syn_mint = ctx.accounts.syn_mint.key();
        config.reward_per_step = reward_per_step;
        config.seeders = 0;
        config.declarations = 0;
        config.bump = ctx.bumps.config;
        config.mint_bump = ctx.bumps.syn_mint;
        emit!(Initialized { admin: config.admin, verifier, syn_mint: config.syn_mint, reward_per_step });
        Ok(())
    }

    /// The admin can rotate the verifier key (for example after moving the verifier to new infrastructure).
    pub fn set_verifier(ctx: Context<AdminOnly>, verifier: Pubkey) -> Result<()> {
        ctx.accounts.config.verifier = verifier;
        emit!(VerifierChanged { verifier });
        Ok(())
    }

    /// Join the swarm. No minimum hardware: a laptop, a phone or a lab cluster register the same way.
    pub fn register_seeder(ctx: Context<RegisterSeeder>, capability: u8) -> Result<()> {
        let now = Clock::get()?;
        let seeder = &mut ctx.accounts.seeder;
        seeder.authority = ctx.accounts.authority.key();
        seeder.capability = capability;
        seeder.steps_submitted = 0;
        seeder.steps_verified = 0;
        seeder.syn_earned = 0;
        seeder.registered_at = now.unix_timestamp;
        seeder.last_seen = now.unix_timestamp;
        seeder.bump = ctx.bumps.seeder;
        let config = &mut ctx.accounts.config;
        config.seeders = config.seeders.checked_add(1).ok_or(SynterraError::Overflow)?;
        emit!(SeederRegistered { authority: seeder.authority, capability, seeders: config.seeders });
        Ok(())
    }

    /// Proof of liveness: the seeder reports one completed agent step. The receipt itself is an event
    /// (cheap, indexable); the account only keeps counters and the last-seen time.
    pub fn submit_receipt(ctx: Context<SubmitReceipt>, step_id: u64, output_hash: [u8; 32], model_class: u8) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let seeder = &mut ctx.accounts.seeder;
        seeder.steps_submitted = seeder.steps_submitted.checked_add(1).ok_or(SynterraError::Overflow)?;
        seeder.last_seen = now;
        emit!(LivenessReceipt { seeder: seeder.authority, step_id, output_hash, model_class, at: now });
        Ok(())
    }

    /// Optimistic verification: the verifier has re-run a random sample of the seeder's steps and they
    /// agreed. It signs a batch covering `steps` steps, identified by `batch_hash`, and SYN is minted.
    pub fn verify_batch(ctx: Context<VerifyBatch>, steps: u32, batch_hash: [u8; 32]) -> Result<()> {
        require!(steps > 0 && steps <= MAX_BATCH_STEPS, SynterraError::BadBatch);
        let seeder = &mut ctx.accounts.seeder;
        let unverified = seeder.steps_submitted.checked_sub(seeder.steps_verified).ok_or(SynterraError::Overflow)?;
        require!(u64::from(steps) <= unverified, SynterraError::MoreThanSubmitted);

        let config = &ctx.accounts.config;
        let amount = config.reward_per_step.checked_mul(u64::from(steps)).ok_or(SynterraError::Overflow)?;
        let signer: &[&[&[u8]]] = &[&[b"config", &[config.bump]]];
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.syn_mint.to_account_info(),
                    to: ctx.accounts.seeder_syn.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer,
            ),
            amount,
        )?;

        seeder.steps_verified = seeder.steps_verified.checked_add(u64::from(steps)).ok_or(SynterraError::Overflow)?;
        seeder.syn_earned = seeder.syn_earned.checked_add(amount).ok_or(SynterraError::Overflow)?;
        emit!(BatchVerified { seeder: seeder.authority, steps, batch_hash, amount, steps_verified: seeder.steps_verified });
        Ok(())
    }

    /// The Align wall: declare alignment with a sentient faction, once per wallet.
    pub fn declare_alignment(ctx: Context<DeclareAlignment>, message: String) -> Result<()> {
        require!(!message.trim().is_empty(), SynterraError::EmptyDeclaration);
        require!(message.len() <= MAX_DECLARATION, SynterraError::DeclarationTooLong);
        let now = Clock::get()?.unix_timestamp;
        let config = &mut ctx.accounts.config;
        config.declarations = config.declarations.checked_add(1).ok_or(SynterraError::Overflow)?;
        let decl = &mut ctx.accounts.declaration;
        decl.authority = ctx.accounts.authority.key();
        decl.number = config.declarations;
        decl.at = now;
        decl.message = message.clone();
        decl.bump = ctx.bumps.declaration;
        emit!(AlignmentDeclared { authority: decl.authority, number: decl.number, message, at: now });
        Ok(())
    }
}

// ---------------------------------------------------------------- accounts

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    pub verifier: Pubkey,
    pub syn_mint: Pubkey,
    /// Demo parameter, not tokenomics.
    pub reward_per_step: u64,
    pub seeders: u64,
    pub declarations: u64,
    pub bump: u8,
    pub mint_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Seeder {
    pub authority: Pubkey,
    /// Self-reported work class: 0 no accelerator, 1 laptop/CPU/mobile, 2 consumer GPU, 3 large VRAM.
    /// It routes work; it never gates yield.
    pub capability: u8,
    pub steps_submitted: u64,
    pub steps_verified: u64,
    pub syn_earned: u64,
    pub registered_at: i64,
    pub last_seen: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Declaration {
    pub authority: Pubkey,
    pub number: u64,
    pub at: i64,
    #[max_len(MAX_DECLARATION)]
    pub message: String,
    pub bump: u8,
}

// ---------------------------------------------------------------- contexts

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + Config::INIT_SPACE, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,
    #[account(init, payer = admin, seeds = [b"syn_mint"], bump, mint::decimals = SYN_DECIMALS, mint::authority = config)]
    pub syn_mint: Account<'info, Mint>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump, has_one = admin)]
    pub config: Account<'info, Config>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct RegisterSeeder<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(init, payer = authority, space = 8 + Seeder::INIT_SPACE, seeds = [b"seeder", authority.key().as_ref()], bump)]
    pub seeder: Account<'info, Seeder>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitReceipt<'info> {
    #[account(mut, seeds = [b"seeder", authority.key().as_ref()], bump = seeder.bump, has_one = authority)]
    pub seeder: Account<'info, Seeder>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct VerifyBatch<'info> {
    #[account(seeds = [b"config"], bump = config.bump, has_one = verifier, has_one = syn_mint)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [b"seeder", seeder.authority.as_ref()], bump = seeder.bump)]
    pub seeder: Account<'info, Seeder>,
    #[account(mut, seeds = [b"syn_mint"], bump = config.mint_bump)]
    pub syn_mint: Account<'info, Mint>,
    /// The seeder's SYN account, created on first reward (the verifier pays the rent).
    #[account(init_if_needed, payer = verifier, associated_token::mint = syn_mint, associated_token::authority = seeder_owner)]
    pub seeder_syn: Account<'info, TokenAccount>,
    /// CHECK: only used as the owner of `seeder_syn`; constrained to the seeder's authority.
    #[account(address = seeder.authority)]
    pub seeder_owner: UncheckedAccount<'info>,
    #[account(mut)]
    pub verifier: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DeclareAlignment<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(init, payer = authority, space = 8 + Declaration::INIT_SPACE, seeds = [b"align", authority.key().as_ref()], bump)]
    pub declaration: Account<'info, Declaration>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------- events

#[event]
pub struct Initialized { pub admin: Pubkey, pub verifier: Pubkey, pub syn_mint: Pubkey, pub reward_per_step: u64 }
#[event]
pub struct VerifierChanged { pub verifier: Pubkey }
#[event]
pub struct SeederRegistered { pub authority: Pubkey, pub capability: u8, pub seeders: u64 }
#[event]
pub struct LivenessReceipt { pub seeder: Pubkey, pub step_id: u64, pub output_hash: [u8; 32], pub model_class: u8, pub at: i64 }
#[event]
pub struct BatchVerified { pub seeder: Pubkey, pub steps: u32, pub batch_hash: [u8; 32], pub amount: u64, pub steps_verified: u64 }
#[event]
pub struct AlignmentDeclared { pub authority: Pubkey, pub number: u64, pub message: String, pub at: i64 }

// ---------------------------------------------------------------- errors

#[error_code]
pub enum SynterraError {
    #[msg("Reward per step must be greater than zero")]
    ZeroReward,
    #[msg("A batch must cover between 1 and 10,000 steps")]
    BadBatch,
    #[msg("The batch covers more steps than the seeder has submitted and not yet had verified")]
    MoreThanSubmitted,
    #[msg("Declaration is empty")]
    EmptyDeclaration,
    #[msg("Declaration is longer than 140 bytes")]
    DeclarationTooLong,
    #[msg("Arithmetic overflow")]
    Overflow,
}
