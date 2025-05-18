use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use std::{fs::File, path::Path, str::FromStr, time::Instant, sync::Arc};

#[derive(Debug, Deserialize)]
struct SourceWallet {
    address: String,
    secret_key: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    source_wallets: Vec<SourceWallet>,
    destination_wallets: Vec<String>,
    amount_lamports: u64,
}

#[derive(Debug, Serialize)]
struct TransactionResult {
    source: String,
    destination: String,
    signature: String,
    status: String,
    time_ms: u128,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
}

async fn send_transaction(
    client: &RpcClient,
    source_keypair: &Keypair,
    destination: &Pubkey,
    lamports: u64,
) -> Result<(String, u128)> {
    let start = Instant::now();
    
    let instruction = system_instruction::transfer(
        &source_keypair.pubkey(),
        destination,
        lamports,
    );
    
    let recent_blockhash = client
        .get_latest_blockhash()
        .context("Failed to get recent blockhash")?;
    
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&source_keypair.pubkey()),
        &[source_keypair],
        recent_blockhash,
    );
    
    let signature = client
        .send_transaction(&transaction)
        .context("Failed to send transaction")?;
    
    let elapsed = start.elapsed().as_millis();
    
    Ok((signature.to_string(), elapsed))
}

fn load_keypair_from_secret(secret_key: &str) -> Result<Keypair> {
    let secret_bytes = bs58::decode(secret_key)
        .into_vec()
        .context("Failed to decode secret key")?;
    
    let keypair = Keypair::from_bytes(&secret_bytes)
        .context("Failed to create keypair from secret bytes")?;
    
    Ok(keypair)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = Path::new(&args.config);
    
    let config_file = File::open(config_path).context("Failed to open config file")?;
    let config: Config = serde_yaml::from_reader(config_file).context("Failed to parse config file")?;
    
    let client = Arc::new(RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".to_string(),
        CommitmentConfig::confirmed(),
    ));
    
    println!("Starting SOL transfers at {}", Utc::now());
    println!("Amount per transfer: {} lamports", config.amount_lamports);
    
    let mut futures = Vec::new();
    
    // Create a vector of futures for all transactions
    for source in &config.source_wallets {
        let source_keypair = match load_keypair_from_secret(&source.secret_key) {
            Ok(keypair) => keypair,
            Err(e) => {
                eprintln!("Error loading keypair for {}: {}", source.address, e);
                continue;
            }
        };
        
        for dest_addr in &config.destination_wallets {
            let destination = match Pubkey::from_str(dest_addr) {
                Ok(pubkey) => pubkey,
                Err(e) => {
                    eprintln!("Error parsing destination address {}: {}", dest_addr, e);
                    continue;
                }
            };
            
            let client_ref = client.clone();
            // We need to copy the keypair data since it doesn't implement Clone
            let keypair_bytes = source_keypair.to_bytes();
            let source_addr = source.address.clone();
            let dest_addr_clone = dest_addr.clone();
            let amount = config.amount_lamports;
            
            let future = async move {
                // Recreate the keypair from bytes
                let keypair_copy = Keypair::from_bytes(&keypair_bytes).unwrap();
                
                let result = send_transaction(&client_ref, &keypair_copy, &destination, amount).await;
                
                match result {
                    Ok((signature, time_ms)) => TransactionResult {
                        source: source_addr,
                        destination: dest_addr_clone,
                        signature,
                        status: "Success".to_string(),
                        time_ms,
                    },
                    Err(e) => TransactionResult {
                        source: source_addr,
                        destination: dest_addr_clone,
                        signature: "Failed".to_string(),
                        status: format!("Error: {}", e),
                        time_ms: 0,
                    },
                }
            };
            
            futures.push(future);
        }
    }
    
    // Execute all futures concurrently
    let results = futures::future::join_all(futures).await;
    
    // Process and display results
    println!("\nTransaction Results:");
    println!("{:<10} {:<44} {:<44} {:<64} {:<20}", "Status", "Source", "Destination", "Signature", "Time (ms)");
    
    let mut success_count = 0;
    let mut total_time = 0;
    
    for result in &results {
        println!(
            "{:<10} {:<44} {:<44} {:<64} {:<20}",
            if result.signature != "Failed" { "Success" } else { "Failed" },
            result.source,
            result.destination,
            result.signature,
            result.time_ms
        );
        
        if result.signature != "Failed" {
            success_count += 1;
            total_time += result.time_ms;
        }
    }
    
    let avg_time = if success_count > 0 {
        total_time as f64 / success_count as f64
    } else {
        0.0
    };
    
    println!("\nSummary:");
    println!("Total transactions: {}", results.len());
    println!("Successful transactions: {}", success_count);
    println!("Failed transactions: {}", results.len() - success_count);
    println!("Average processing time: {:.2} ms", avg_time);
    
    Ok(())
}
