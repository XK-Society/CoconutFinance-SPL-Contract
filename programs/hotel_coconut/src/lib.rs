use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("E26SowuKYen9ePnVirUyxq73hKaomHhwdPiRdVCKcu6d");

#[program]
pub mod hotel_coconut {
    use super::*;

    pub fn initialize_hotel(ctx: Context<Initialize>, room_count: u64, _transfer_fee_basis_points: u16) -> Result<()> {
        let hotel = &mut ctx.accounts.hotel;
        hotel.authority = ctx.accounts.authority.key();
        hotel.room_count = room_count;
        hotel.rooms_minted = 0;
        hotel.total_profit = 0;
        Ok(())
    }

    pub fn mint_room_token(ctx: Context<MintRoomToken>, room_number: u64) -> Result<()> {
        require!(room_number <= ctx.accounts.hotel.room_count, HotelError::InvalidRoomNumber);
        require!(ctx.accounts.hotel.rooms_minted < ctx.accounts.hotel.room_count, HotelError::AllRoomsMinted);

        token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::MintTo {
                    mint: ctx.accounts.room_mint.to_account_info(),
                    to: ctx.accounts.user_room_ata.to_account_info(),
                    authority: ctx.accounts.hotel.to_account_info(),
                },
            ),
            1,
        )?;

        ctx.accounts.hotel.rooms_minted += 1;
        Ok(())
    }

    pub fn book_room(ctx: Context<BookRoom>, room_number: u64, booking_price: u64) -> Result<()> {
        require!(room_number <= ctx.accounts.hotel.room_count, HotelError::InvalidRoomNumber);

        token::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::TransferChecked {
                    from: ctx.accounts.tourist_usdc_account.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.hotel_usdc_vault.to_account_info(),
                    authority: ctx.accounts.tourist.to_account_info(),
                },
            ),
            booking_price,
            ctx.accounts.usdc_mint.decimals,
        )?;

        ctx.accounts.hotel.total_profit += booking_price;

        emit!(BookingEvent {
            room_number,
            tourist: ctx.accounts.tourist.key(),
            price: booking_price,
        });

        Ok(())
    }

    pub fn distribute_profits(ctx: Context<DistributeProfits>) -> Result<()> {
        let total_profit = ctx.accounts.hotel.total_profit;
        require!(total_profit > 0, HotelError::NoProfitToDistribute);

        let total_supply = ctx.accounts.room_mint.supply;
        let profit_per_token = total_profit / total_supply;

        let user_token_balance = ctx.accounts.user_room_ata.amount;
        let user_profit = profit_per_token * user_token_balance;

        token::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::TransferChecked {
                    from: ctx.accounts.hotel_usdc_vault.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.user_usdc_account.to_account_info(),
                    authority: ctx.accounts.hotel.to_account_info(),
                },
                &[&[b"hotel", &[ctx.bumps.hotel]]],
            ),
            user_profit,
            ctx.accounts.usdc_mint.decimals,
        )?;

        ctx.accounts.hotel.total_profit -= user_profit;

        emit!(ProfitDistributionEvent {
            user: ctx.accounts.user.key(),
            amount: user_profit,
        });

        Ok(())
    }

    pub fn initialize_pool(ctx: Context<InitializePool>, fee_basis_points: u16) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.authority = ctx.accounts.authority.key();
        pool.usdc_mint = ctx.accounts.usdc_mint.key();
        pool.lp_token_mint = ctx.accounts.lp_token_mint.key();
        pool.total_liquidity = 0;
        pool.fee_basis_points = fee_basis_points;
        Ok(())
    }

    pub fn provide_liquidity(ctx: Context<ProvideLiquidity>, usdc_amount: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        token::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::TransferChecked {
                    from: ctx.accounts.user_usdc_account.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.pool_usdc_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            usdc_amount,
            ctx.accounts.usdc_mint.decimals,
        )?;

        let lp_tokens_to_mint = usdc_amount;

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::MintTo {
                    mint: ctx.accounts.lp_token_mint.to_account_info(),
                    to: ctx.accounts.user_lp_token_account.to_account_info(),
                    authority: pool.to_account_info(),
                },
                &[&[b"pool", &[ctx.bumps.pool]]],
            ),
            lp_tokens_to_mint,
        )?;

        pool.total_liquidity += usdc_amount;

        emit!(LiquidityProvidedEvent {
            user: ctx.accounts.user.key(),
            usdc_amount,
            lp_tokens_minted: lp_tokens_to_mint,
        });

        Ok(())
    }

    pub fn withdraw_liquidity(ctx: Context<WithdrawLiquidity>, lp_token_amount: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        let usdc_to_return = lp_token_amount;

        require!(pool.total_liquidity >= usdc_to_return, LiquidityPoolError::InsufficientLiquidity);

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Burn {
                    mint: ctx.accounts.lp_token_mint.to_account_info(),
                    from: ctx.accounts.user_lp_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            lp_token_amount,
        )?;

        token::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::TransferChecked {
                    from: ctx.accounts.pool_usdc_account.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.user_usdc_account.to_account_info(),
                    authority: pool.to_account_info(),
                },
                &[&[b"pool", &[ctx.bumps.pool]]],
            ),
            usdc_to_return,
            ctx.accounts.usdc_mint.decimals,
        )?;

        pool.total_liquidity -= usdc_to_return;

        emit!(LiquidityWithdrawnEvent {
            user: ctx.accounts.user.key(),
            lp_tokens_burned: lp_token_amount,
            usdc_returned: usdc_to_return,
        });

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(room_count: u64, transfer_fee_basis_points: u16)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 8 + 8 + 8)]
    pub hotel: Account<'info, Hotel>,
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        mint::decimals = 0,
        mint::authority = hotel,
        mint::freeze_authority = hotel,
    )]
    pub room_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(room_number: u64)]
pub struct MintRoomToken<'info> {
    #[account(mut)]
    pub hotel: Account<'info, Hotel>,
    #[account(mut)]
    pub room_mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = room_mint,
        associated_token::authority = user,
    )]
    pub user_room_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(room_number: u64, booking_price: u64)]
pub struct BookRoom<'info> {
    #[account(mut)]
    pub hotel: Account<'info, Hotel>,
    #[account(mut)]
    pub tourist: Signer<'info>,
    #[account(mut)]
    pub tourist_usdc_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub hotel_usdc_vault: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct DistributeProfits<'info> {
    #[account(mut, seeds = [b"hotel"], bump)]
    pub hotel: Account<'info, Hotel>,
    #[account(mut)]
    pub room_mint: Account<'info, Mint>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_room_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_usdc_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub hotel_usdc_vault: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(fee_basis_points: u16)]
pub struct InitializePool<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 32 + 32 + 8 + 2)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub usdc_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        mint::decimals = 9,
        mint::authority = pool,
    )]
    pub lp_token_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(usdc_amount: u64)]
pub struct ProvideLiquidity<'info> {
    #[account(mut, seeds = [b"pool"], bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_usdc_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_usdc_account: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    #[account(mut)]
    pub lp_token_mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = lp_token_mint,
        associated_token::authority = user,
    )]
    pub user_lp_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(lp_token_amount: u64)]
pub struct WithdrawLiquidity<'info> {
    #[account(mut, seeds = [b"pool"], bump)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_usdc_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_usdc_account: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    #[account(mut)]
    pub lp_token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub user_lp_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct Hotel {
    pub authority: Pubkey,
    pub room_count: u64,
    pub rooms_minted: u64,
    pub total_profit: u64,
}

#[account]
pub struct Pool {
    pub authority: Pubkey,
    pub usdc_mint: Pubkey,
    pub lp_token_mint: Pubkey,
    pub total_liquidity: u64,
    pub fee_basis_points: u16,
}

#[error_code]
pub enum HotelError {
    #[msg("Invalid room number")]
    InvalidRoomNumber,
    #[msg("All rooms have been minted")]
    AllRoomsMinted,
    #[msg("No profit to distribute")]
    NoProfitToDistribute,
}

#[error_code]
pub enum LiquidityPoolError {
    #[msg("Insufficient liquidity in the pool")]
    InsufficientLiquidity,
}

#[event]
pub struct BookingEvent {
    pub room_number: u64,
    pub tourist: Pubkey,
    pub price: u64,
}

#[event]
pub struct ProfitDistributionEvent {
    pub user: Pubkey,
    pub amount: u64,
}

#[event]
pub struct LiquidityProvidedEvent {
    pub user: Pubkey,
    pub usdc_amount: u64,
    pub lp_tokens_minted: u64,
}

#[event]
pub struct LiquidityWithdrawnEvent {
    pub user: Pubkey,
    pub lp_tokens_burned: u64,
    pub usdc_returned: u64,
}