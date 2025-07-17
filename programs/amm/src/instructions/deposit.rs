use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token::{accessor::mint, mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer}};
use constant_product_curve::ConstantProduct;

use crate::{state::Config};
use crate::{error::AmmError};

// this is helpful for liquidity providers in order to deposit their tokens

/*
    accounts in the context struct:
    - user
    - mint_x, mint_y, mint_lp
    - config
    - vault_x, vault_y
    - user_x, user_y, user_lp
    - the three accounts
*/
#[derive(Accounts)]
#[instruction(seed:u64)]
pub struct Deposit<'info>{

    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Account<'info,Mint>,
    pub mint_y: Account<'info,Mint>,

    #[account(
        mut, // mutable because we will mint and change it's state
        mint::decimals = 6,
        mint::authority = config,
    )]
    pub mint_lp: Account<'info,Mint>, // lp tokens to be given to the users

    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [b"config", seed.to_le_bytes().as_ref()],
        bump = config.config_bump
    )]
    pub config: Account<'info,Config>,

    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config
    )]
    pub vault_x: Account<'info,TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config
    )]
    pub vault_y: Account<'info,TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Account<'info,TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Account<'info,TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_lp,
        associated_token::authority = user,
    )]
    pub user_lp: Account<'info,TokenAccount>,

    pub system_program: Program<'info,System>,
    pub token_program: Program<'info,Token>,
    pub associated_token_program: Program<'info,AssociatedToken>,

}

impl <'info> Deposit<'info> {

    // here amount is the user desired lp token amount
    // here users are basically DEPOSITING X AND Y TOKENS TO PROVIDE LIQUIDITY and quote their amount of lp tokens
    pub fn deposit(&mut self, amount: u64, max_x: u64, max_y: u64) -> Result<()> {

        // if required condition is not true, then returns the mentioned error
        require!(self.config.locked == false, AmmError::PoolLocked);
        require!(amount > 0, AmmError::InvalidAmount);

        let (x, y) = match self.mint_lp.supply == 0
            && self.vault_x.amount == 0
            && self.vault_y.amount == 0
        { // if we in the initial stage, then we can set max_x and max_y as x and y
            true => (max_x, max_y),
            false => { // we will fetch the x, y deposit amounts
                let amount = ConstantProduct::xy_deposit_amounts_from_l(
                    self.vault_x.amount,
                    self.vault_y.amount,
                    self.mint_lp.supply,
                    amount,
                    6,
                )
                .unwrap();
                (amount.x, amount.y)
            }
        };
        require!(x <= max_x && y <= max_y, AmmError::SlippageExceeded);
        self.deposit_tokens(true, x)?;
        self.deposit_tokens(false, y)?;
        self.mint_lp_token(amount)
    }
    
    pub fn deposit_tokens(&mut self, is_x:bool, amount:u64) -> Result<()>{

        let (from,to) = match is_x {
            true => (
                self.user_x.to_account_info(),
                self.vault_x.to_account_info()
            ),
            false => (
                self.user_y.to_account_info(),
                self.vault_y.to_account_info()
            )
        };

        let cpi_program = self.token_program.to_account_info();

        let cpi_accounts = Transfer{
            from,
            to,
            authority: self.user.to_account_info()
        };

        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        transfer(cpi_ctx, amount)?;

        Ok(())
    }

    pub fn mint_lp_token(&mut self, amount: u64)->Result<()>{

        let cpi_program = self.token_program.to_account_info();

        let cpi_accounts = MintTo {
    mint: self.mint_lp.to_account_info(),
    to: self.user_lp.to_account_info(),
    authority: self.config.to_account_info(), // Config is the mint authority
};

        let signer_seeds = &[
            &b"config"[..],
            &self.config.seed.to_le_bytes(),
            &[self.config.config_bump],
        ];

        let signer_seeds = &[&signer_seeds[..]];

        let cpi_context = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

        mint_to(cpi_context, amount)?;

        Ok(())
       
    }

    
}