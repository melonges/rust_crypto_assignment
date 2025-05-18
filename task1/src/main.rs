use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::File;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Config {
    wallets: Vec<String>,
}

#[derive(Debug, Serialize)]
struct WalletBalance {
    address: String,
    balance: f64,
}

async fn get_balance(client: &Client, rpc_url: &str, wallet: &str) -> Result<f64> {
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBalance",
        "params": [wallet]
    });

    let response = client
        .post(rpc_url)
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to Solana RPC")?;

    let response_json: Value = response
        .json()
        .await
        .context("Failed to parse response as JSON")?;

    let balance = response_json["result"]["value"]
        .as_u64()
        .context("Failed to extract balance from response")?;

    // Convert lamports to SOL (1 SOL = 1,000,000,000 lamports)
    Ok(balance as f64 / 1_000_000_000.0)
}

async fn get_multiple_balances(config_path: &Path) -> Result<Vec<WalletBalance>> {
    let config_file = File::open(config_path).context("Failed to open config file")?;
    let config: Config = serde_yaml::from_reader(config_file).context("Failed to parse config file")?;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    let rpc_url = "https://api.mainnet-beta.solana.com";
    
    let mut wallet_balances = Vec::new();
    
    // Create a vector to hold all the futures
    let mut futures = Vec::new();
    
    // Create futures for all wallet balance requests
    for wallet in &config.wallets {
        let wallet_clone = wallet.clone();
        let client_clone = client.clone();
        let rpc_url_clone = rpc_url.to_string();
        
        let future = async move {
            let balance = get_balance(&client_clone, &rpc_url_clone, &wallet_clone).await?;
            Ok::<WalletBalance, anyhow::Error>(WalletBalance {
                address: wallet_clone,
                balance,
            })
        };
        
        futures.push(future);
    }
    
    // Execute all futures concurrently
    let results = futures::future::join_all(futures).await;
    
    // Process results
    for result in results {
        match result {
            Ok(wallet_balance) => wallet_balances.push(wallet_balance),
            Err(e) => eprintln!("Error getting balance: {}", e),
        }
    }

    Ok(wallet_balances)
}

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = Path::new("config.yaml");
    
    let wallet_balances = get_multiple_balances(config_path).await?;
    
    println!("Wallet Balances:");
    for wb in wallet_balances {
        println!("{}: {} SOL", wb.address, wb.balance);
    }
    
    Ok(())
}
