use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    program::invoke_signed,
    system_instruction,
    rent::Rent,
    sysvar::Sysvar,
};
use thiserror::Error;

use crate::instruction::DepositInstruction;

/// Define the state of the deposit account
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct DepositAccount {
    pub owner: Pubkey,
    pub balance: u64,
}

/// Error types for the deposit/withdraw program
#[derive(Error, Debug)]
pub enum DepositError {
    #[error("Insufficient funds for withdrawal")]
    InsufficientFunds,
    
    #[error("Account not owned by expected program")]
    IncorrectProgramId,
    
    #[error("Invalid instruction data")]
    InvalidInstructionData,
}

impl From<DepositError> for ProgramError {
    fn from(e: DepositError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

pub struct Processor;

impl Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        // Deserialize the instruction data
        let instruction = DepositInstruction::try_from_slice(instruction_data)
            .map_err(|_| DepositError::InvalidInstructionData)?;
        
        // Process the instruction
        match instruction {
            DepositInstruction::Deposit { amount } => {
                Self::process_deposit(program_id, accounts, amount)
            },
            DepositInstruction::Withdraw { amount } => {
                Self::process_withdraw(program_id, accounts, amount)
            },
        }
    }

    // Process a deposit instruction
    fn process_deposit(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        // Get the account iterator
        let account_info_iter = &mut accounts.iter();
        
        // Get the accounts
        let funder_info = next_account_info(account_info_iter)?;
        let deposit_account_info = next_account_info(account_info_iter)?;
        
        // Verify the deposit account is owned by our program
        if deposit_account_info.owner != program_id {
            return Err(DepositError::IncorrectProgramId.into());
        }
        
        // Verify the funder signed the transaction
        if !funder_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        
        // Transfer SOL from funder to deposit account
        let instruction = system_instruction::transfer(
            funder_info.key,
            deposit_account_info.key,
            amount,
        );
        
        invoke_signed(
            &instruction,
            &[funder_info.clone(), deposit_account_info.clone()],
            &[],
        )?;
        
        // Update the deposit account state
        let mut deposit_account_data = if deposit_account_info.data_len() > 0 {
            DepositAccount::try_from_slice(&deposit_account_info.data.borrow())?
        } else {
            // Initialize new account
            DepositAccount {
                owner: *funder_info.key,
                balance: 0,
            }
        };
        
        // Update balance
        deposit_account_data.balance += amount;
        
        // Serialize the updated state back to the account
        deposit_account_data.serialize(&mut *deposit_account_info.data.borrow_mut())?;
        
        msg!("Deposit successful: {} lamports", amount);
        
        Ok(())
    }

    // Process a withdraw instruction
    fn process_withdraw(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        // Get the account iterator
        let account_info_iter = &mut accounts.iter();
        
        // Get the accounts
        let owner_info = next_account_info(account_info_iter)?;
        let deposit_account_info = next_account_info(account_info_iter)?;
        let destination_info = next_account_info(account_info_iter)?;
        
        // Verify the deposit account is owned by our program
        if deposit_account_info.owner != program_id {
            return Err(DepositError::IncorrectProgramId.into());
        }
        
        // Verify the owner signed the transaction
        if !owner_info.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        
        // Deserialize the deposit account data
        let mut deposit_account_data = DepositAccount::try_from_slice(&deposit_account_info.data.borrow())?;
        
        // Verify the owner is authorized
        if deposit_account_data.owner != *owner_info.key {
            return Err(ProgramError::InvalidAccountData);
        }
        
        // Check if there are sufficient funds
        if deposit_account_data.balance < amount {
            return Err(DepositError::InsufficientFunds.into());
        }
        
        // Calculate the rent-exempt amount that must remain in the account
        let rent = Rent::get()?;
        let min_balance = rent.minimum_balance(deposit_account_info.data_len());
        
        // Ensure the account will remain rent-exempt after withdrawal
        let available_for_withdrawal = deposit_account_info.lamports()
            .checked_sub(min_balance)
            .ok_or(DepositError::InsufficientFunds)?;
        
        if amount > available_for_withdrawal {
            return Err(DepositError::InsufficientFunds.into());
        }
        
        // Update the deposit account balance
        deposit_account_data.balance -= amount;
        
        // Transfer lamports from deposit account to destination
        **deposit_account_info.lamports.borrow_mut() -= amount;
        **destination_info.lamports.borrow_mut() += amount;
        
        // Serialize the updated state back to the account
        deposit_account_data.serialize(&mut *deposit_account_info.data.borrow_mut())?;
        
        msg!("Withdrawal successful: {} lamports", amount);
        
        Ok(())
    }
}
