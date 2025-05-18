use anyhow::{Context, Result};
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
use std::{fs::File, path::Path, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tonic::transport::Channel;

// Include the generated gRPC code
pub mod geyser {
    tonic::include_proto!("geyser");
}

use geyser::{
    geyser_client::GeyserClient,
    Filter, SubscribeRequest, SubscribeUpdate,
    filter::Filter as FilterEnum,
    subscribe_update::Update,
    BlocksFilter,
};

#[derive(Debug, Deserialize)]
struct SourceWallet {
    address: String,
    secret_key: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    source_wallet: SourceWallet,
    destination_wallet: String,
    amount_lamports: u64,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
    
    /// GRPC endpoint
    #[arg(short, long, default_value = "https://grpc.ny.shyft.to")]
    grpc_endpoint: String,
}

async fn send_transaction(
    client: &RpcClient,
    source_keypair: &Keypair,
    destination: &Pubkey,
    lamports: u64,
) -> Result<String> {
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
    
    Ok(signature.to_string())
}

fn load_keypair_from_secret(secret_key: &str) -> Result<Keypair> {
    let secret_bytes = bs58::decode(secret_key)
        .into_vec()
        .context("Failed to decode secret key")?;
    
    let keypair = Keypair::from_bytes(&secret_bytes)
        .context("Failed to create keypair from secret bytes")?;
    
    Ok(keypair)
}

async fn subscribe_to_blocks(
    grpc_endpoint: &str,
    tx: mpsc::Sender<u64>,
) -> Result<()> {
    // Connect to the gRPC server
    let channel = Channel::from_shared(grpc_endpoint.to_string())
        .context("Failed to create channel")?
        .connect()
        .await
        .context("Failed to connect to gRPC endpoint")?;
    
    let mut client = GeyserClient::new(channel);
    
    // Create a subscription request for new blocks
    let blocks_filter = BlocksFilter {
        account_include: false,
    };
    
    let filter = Filter {
        filter: Some(FilterEnum::Blocks(blocks_filter)),
    };
    
    let request = SubscribeRequest {
        filters: vec![filter],
    };
    
    // Subscribe to updates
    let mut stream = client
        .subscribe(request)
        .await
        .context("Failed to subscribe to gRPC stream")?
        .into_inner();
    
    println!("Successfully subscribed to block updates");
    
    // Process incoming updates
    while let Some(update) = stream.message().await? {
        if let Some(Update::Block(block)) = update.update {
            println!("New block detected: Slot {}", block.slot);
            tx.send(block.slot).await.ok();
        }
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_path = Path::new(&args.config);
    
    // Load configuration
    let config_file = File::open(config_path).context("Failed to open config file")?;
    let config: Config = serde_yaml::from_reader(config_file).context("Failed to parse config file")?;
    
    // Set up Solana client
    let rpc_client = Arc::new(RpcClient::new_with_commitment(
        "https://api.devnet.solana.com".to_string(),
        CommitmentConfig::confirmed(),
    ));
    
    // Load source keypair
    let source_keypair = load_keypair_from_secret(&config.source_wallet.secret_key)
        .context("Failed to load source keypair")?;
    
    // Parse destination pubkey
    let destination = Pubkey::from_str(&config.destination_wallet)
        .context("Failed to parse destination wallet address")?;
    
    // Create a channel for block notifications
    let (tx, mut rx) = mpsc::channel::<u64>(100);
    
    // Spawn a task to subscribe to block updates
    let grpc_task = tokio::spawn(async move {
        if let Err(e) = subscribe_to_blocks(&args.grpc_endpoint, tx).await {
            eprintln!("Error in gRPC subscription: {}", e);
        }
    });
    
    println!("Waiting for new blocks...");
    println!("When a new block is detected, will send {} lamports from {} to {}",
        config.amount_lamports,
        config.source_wallet.address,
        config.destination_wallet
    );
    
    // Process block notifications and send transactions
    while let Some(slot) = rx.recv().await {
        println!("Processing block at slot: {}", slot);
        
        // Clone references for the async block
        let rpc_client_clone = rpc_client.clone();
        let keypair_bytes = source_keypair.to_bytes();
        let destination_clone = destination;
        let amount = config.amount_lamports;
        
        // Execute transaction in a separate task
        tokio::spawn(async move {
            // Recreate keypair from bytes
            let keypair_copy = match Keypair::from_bytes(&keypair_bytes) {
                Ok(kp) => kp,
                Err(e) => {
                    eprintln!("Error recreating keypair: {}", e);
                    return;
                }
            };
            
            match send_transaction(&rpc_client_clone, &keypair_copy, &destination_clone, amount).await {
                Ok(signature) => {
                    println!("Transaction sent successfully for block {}", slot);
                    println!("Signature: {}", signature);
                }
                Err(e) => {
                    eprintln!("Failed to send transaction for block {}: {}", slot, e);
                }
            }
        });
        
        // Add a small delay to avoid rate limiting
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Wait for the gRPC task to complete (this will likely never happen in normal operation)
    grpc_task.await?;
    
    Ok(())
}
