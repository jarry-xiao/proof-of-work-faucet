use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    system_instruction,
};
use bs58::encode;

declare_id!("PoWSNH2hEZogtCg1Zgm51FnkmJperzYDgPK4fvs8taL");

pub fn create_account<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    new_account: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
    program_owner: &Pubkey,
    rent: &Rent,
    space: u64,
    seeds: Vec<Vec<u8>>,
) -> ProgramResult {
    let current_lamports = **new_account.try_borrow_lamports()?;
    if current_lamports == 0 {
        // If there are no lamports in the new account, we create it with the create_account instruction
        invoke_signed(
            &system_instruction::create_account(
                payer.key,
                new_account.key,
                rent.minimum_balance(space as usize),
                space,
                program_owner,
            ),
            &[payer.clone(), new_account.clone(), system_program.clone()],
            &[seeds
                .iter()
                .map(|seed| seed.as_slice())
                .collect::<Vec<&[u8]>>()
                .as_slice()],
        )
    } else {
        // Fund the account for rent exemption.
        let required_lamports = rent
            .minimum_balance(space as usize)
            .max(1)
            .saturating_sub(current_lamports);
        if required_lamports > 0 {
            invoke(
                &system_instruction::transfer(payer.key, new_account.key, required_lamports),
                &[payer.clone(), new_account.clone(), system_program.clone()],
            )?;
        }
        // Allocate space.
        invoke_signed(
            &system_instruction::allocate(new_account.key, space),
            &[new_account.clone(), system_program.clone()],
            &[seeds
                .iter()
                .map(|seed| seed.as_slice())
                .collect::<Vec<&[u8]>>()
                .as_slice()],
        )?;
        // Assign to the specified program
        invoke_signed(
            &system_instruction::assign(new_account.key, program_owner),
            &[new_account.clone(), system_program.clone()],
            &[seeds
                .iter()
                .map(|seed| seed.as_slice())
                .collect::<Vec<&[u8]>>()
                .as_slice()],
        )
    }
}

#[program]
pub mod proof_of_work_faucet {
    use super::*;

    pub fn create(ctx: Context<Create>, difficulty: u8, amount: u64) -> Result<()> {
        ctx.accounts.spec.difficulty = difficulty;
        ctx.accounts.spec.amount = amount;
        Ok(())
    }

    pub fn airdrop(ctx: Context<Airdrop>) -> Result<()> {
        let Airdrop {
            payer,
            signer,
            receipt,
            spec,
            source,
            system_program,
        } = ctx.accounts;

        // Count the number of leading A's in the signer's public key.
        let prefix_len = encode(signer.key().as_ref())
            .into_string()
            .chars()
            .take_while(|ch| ch == &'A')
            .count();

        if prefix_len < spec.difficulty as usize {
            msg!(
                "Public key does not meet difficulty requirement of {}: {}",
                spec.difficulty,
                signer.key()
            );
            return Err(ProgramError::MissingRequiredSignature.into());
        }

        msg!("Source wallet balance: {}", source.lamports());
        msg!(
            "Airdropping {} lamports to {}",
            spec.amount.min(source.lamports()),
            payer.key()
        );

        invoke_signed(
            &system_instruction::transfer(
                &source.key(),
                &payer.key(),
                spec.amount.min(source.lamports()),
            ),
            &[
                system_program.to_account_info(),
                payer.to_account_info(),
                source.to_account_info(),
            ],
            &[&[b"source", spec.key().as_ref(), &[ctx.bumps["source"]]]],
        )?;

        // Create a receipt account after receiving the airdrop to lower the base SOL requirement.
        create_account(
            &payer,
            &receipt,
            system_program,
            ctx.program_id,
            &Rent::get()?,
            0,
            vec![
                b"receipt".to_vec(),
                signer.key().to_bytes().to_vec(),
                spec.difficulty.to_le_bytes().to_vec(),
                vec![ctx.bumps["receipt"]],
            ],
        )?;
        Ok(())
    }
}

#[account]
pub struct Difficulty {
    pub difficulty: u8,
    pub amount: u64,
}

#[derive(Accounts)]
#[instruction(difficulty: u8, amount: u64)]
pub struct Create<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        seeds=[b"spec", difficulty.to_le_bytes().as_ref(), amount.to_le_bytes().as_ref()],
        bump,
        space=8 + 1 + 8,
        payer=payer,
    )]
    pub spec: Account<'info, Difficulty>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Airdrop<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub signer: Signer<'info>,
    /// CHECK: Trust me bro
    #[account(
        mut,
        seeds=[b"receipt", signer.key().as_ref(), spec.difficulty.to_le_bytes().as_ref()],
        bump,
    )]
    pub receipt: UncheckedAccount<'info>,
    #[account(
        seeds=[b"spec", spec.difficulty.to_le_bytes().as_ref(), spec.amount.to_le_bytes().as_ref()],
        bump,
    )]
    pub spec: Account<'info, Difficulty>,
    /// CHECK: Trust me bro
    #[account(mut, seeds=[b"source", spec.key().as_ref()], bump)]
    pub source: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
