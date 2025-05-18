use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

/// Client-side instructions for interacting with the deposit/withdraw program
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum DepositInstruction {
    /// Deposit SOL into the account
    Deposit {
        /// Amount to deposit in lamports
        amount: u64,
    },
    
    /// Withdraw SOL from the account
    Withdraw {
        /// Amount to withdraw in lamports
        amount: u64,
    },
}

/// Helper function to check account balance
pub fn get_balance(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let deposit_account_info = next_account_info(account_info_iter)?;
    
    // Verify the deposit account is owned by our program
    if deposit_account_info.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    
    // Deserialize the deposit account data
    let deposit_account_data = DepositAccount::try_from_slice(&deposit_account_info.data.borrow())?;
    
    // Log the balance
    msg!("Account balance: {} lamports", deposit_account_data.balance);
    
    Ok(())
}

/// Define the state of the deposit account
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct DepositAccount {
    pub owner: Pubkey,
    pub balance: u64,
}
