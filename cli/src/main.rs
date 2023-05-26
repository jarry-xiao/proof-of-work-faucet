use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anyhow::anyhow;
use bs58::encode;
use clap::{Parser, Subcommand};
use solana_cli_config::{Config, ConfigInput, CONFIG_FILE};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::read_keypair_file;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;

pub fn get_network(network_str: &str) -> &str {
    match network_str {
        "devnet" | "dev" | "d" => "https://api.devnet.solana.com",
        "mainnet" | "main" | "m" | "mainnet-beta" => "https://api.mainnet-beta.solana.com",
        "localnet" | "localhost" | "l" | "local" => "http://localhost:8899",
        _ => network_str,
    }
}

pub fn get_payer_keypair_from_path(path: &str) -> anyhow::Result<Keypair> {
    read_keypair_file(&*shellexpand::tilde(path)).map_err(|e| anyhow!(e.to_string()))
}

#[derive(Parser, Debug)]
#[clap(version, about)]
struct Arguments {
    #[clap(subcommand)]
    subcommand: SubCommand,
    /// Optionally include your keypair path. Defaults to your Solana CLI config file.
    #[clap(global = true, short, long)]
    keypair_path: Option<String>,
    /// Optionally include your RPC endpoint. Use "local", "dev", "main" for default endpoints. Defaults to your Solana CLI config file.
    #[clap(global = true, short, long)]
    url: Option<String>,
    /// Optionally include a commitment level. Defaults to your Solana CLI config file.
    #[clap(global = true, short, long)]
    commitment: Option<String>,
}

#[derive(Subcommand, Debug)]
enum SubCommand {
    /// Creates a proof of work faucet on devnet
    Create {
        /// Prefix length
        #[clap(short, long)]
        difficulty: u8,
        /// Reward amount in SOL
        #[clap(long)]
        reward: f64,
    },
    /// Get faucet address and balance
    GetFaucet {
        /// Prefix length
        #[clap(short, long)]
        difficulty: u8,
        /// Reward amount in SOL
        #[clap(long)]
        reward: f64,
    },
    /// Mine for SOL
    Mine {
        #[clap(short, long)]
        difficulty: u8,
        #[clap(long)]
        reward: f64,
        #[clap(short, long, default_value = "1000000000")]
        target_lamports: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Arguments::parse();
    let config = match CONFIG_FILE.as_ref() {
        Some(config_file) => Config::load(config_file).unwrap_or_else(|_| {
            println!("Failed to load config file: {}", config_file);
            Config::default()
        }),
        None => Config::default(),
    };
    let commitment =
        ConfigInput::compute_commitment_config("", &cli.commitment.unwrap_or(config.commitment)).1;
    let payer = get_payer_keypair_from_path(&cli.keypair_path.unwrap_or(config.keypair_path))?;
    let network_url = &get_network(&cli.url.unwrap_or(config.json_rpc_url)).to_string();
    let client = RpcClient::new_with_commitment(network_url.to_string(), commitment);

    let genesis = client.get_genesis_hash().await?;

    match genesis.to_string().as_str() {
        "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG" => {}
        _ => anyhow::bail!("Genesis hash does not corespond to devnet"),
    };

    match cli.subcommand {
        SubCommand::Create { difficulty, reward } => {
            let amount: u64 = (reward * 1e9) as u64;
            let create_spec_data =
                proof_of_work_faucet::instruction::Create { difficulty, amount }.data();
            let (spec, _) = Pubkey::find_program_address(
                &[
                    b"spec",
                    difficulty.to_le_bytes().as_ref(),
                    amount.to_le_bytes().as_ref(),
                ],
                &proof_of_work_faucet::id(),
            );
            let (faucet, _) = Pubkey::find_program_address(
                &[b"source", spec.as_ref()],
                &proof_of_work_faucet::id(),
            );
            if client.get_account(&spec).await.is_ok() {
                println!("Faucet already exists at {}", faucet);
                return Ok(());
            }
            let create_accounts = proof_of_work_faucet::accounts::Create {
                payer: payer.pubkey(),
                spec,
                system_program: solana_sdk::system_program::id(),
            };

            let ix = Instruction {
                program_id: proof_of_work_faucet::id(),
                accounts: create_accounts.to_account_metas(None),
                data: create_spec_data,
            };

            let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
                &[ix],
                Some(&payer.pubkey()),
                &[&payer],
                client.get_latest_blockhash().await?,
            );

            let txid = client.send_and_confirm_transaction(&transaction).await?;
            println!(
                "Created proof of work faucet with difficulty {} and reward of {} SOL: {}",
                difficulty, reward, txid
            );
            println!("Faucet spec address: {}", spec);
            println!("Faucet address: {}", faucet);
            Ok(())
        }
        SubCommand::GetFaucet { difficulty, reward } => {
            let amount: u64 = (reward * 1e9) as u64;
            let (spec, _) = Pubkey::find_program_address(
                &[
                    b"spec",
                    difficulty.to_le_bytes().as_ref(),
                    amount.to_le_bytes().as_ref(),
                ],
                &proof_of_work_faucet::id(),
            );
            let (faucet, _) = Pubkey::find_program_address(
                &[b"source", spec.as_ref()],
                &proof_of_work_faucet::id(),
            );
            println!("Faucet address: {}", faucet);
            println!(
                "Faucet balance: {} SOL",
                client.get_balance(&faucet).await? as f64 / 1e9
            );
            Ok(())
        }
        SubCommand::Mine {
            difficulty,
            reward,
            target_lamports,
        } => {
            let amount: u64 = (reward * 1e9) as u64;

            let (spec, _) = Pubkey::find_program_address(
                &[
                    b"spec",
                    difficulty.to_le_bytes().as_ref(),
                    amount.to_le_bytes().as_ref(),
                ],
                &proof_of_work_faucet::id(),
            );
            let (faucet, _) = Pubkey::find_program_address(
                &[b"source", spec.as_ref()],
                &proof_of_work_faucet::id(),
            );

            // Make sure this is a valid faucet
            match client.get_account(&spec).await {
                Ok(acc) => {
                    if acc.data.len() == 0 {
                        println!("Faucet does not exist, please check your parameters");
                        return Ok(());
                    }
                }
                Err(_) => {
                    println!("Faucet does not exist, please check your parameters");
                    return Ok(());
                }
            }
            if client.get_balance(&faucet).await? < amount {
                println!("Faucet is empty");
                return Ok(());
            }

            if client.get_balance(&payer.pubkey()).await? < 5000 {
                // Try to request airdrop the normal way if the wallet is completely empty
                client
                    .request_airdrop(&payer.pubkey(), 1_000_000_000)
                    .await?;
            }

            let mut airdropped_amount = 0;
            while airdropped_amount < target_lamports {
                let signer = Keypair::new();

                let prefix_len = encode(signer.pubkey().as_ref())
                    .into_string()
                    .chars()
                    .take_while(|ch| ch == &'A')
                    .count();

                if prefix_len < difficulty as usize {
                    continue;
                }

                println!("Keypair mined! Pubkey: {}: ", signer.pubkey());
                if client.get_balance(&faucet).await? < amount {
                    println!("Faucet is empty");
                    break;
                }

                let (receipt, _) = Pubkey::find_program_address(
                    &[
                        b"receipt",
                        signer.pubkey().as_ref(),
                        difficulty.to_le_bytes().as_ref(),
                    ],
                    &proof_of_work_faucet::id(),
                );
                let airdrop_accounts = proof_of_work_faucet::accounts::Airdrop {
                    payer: payer.pubkey(),
                    signer: signer.pubkey(),
                    receipt,
                    spec,
                    source: faucet,
                    system_program: solana_sdk::system_program::id(),
                };

                let ix = Instruction {
                    program_id: proof_of_work_faucet::id(),
                    accounts: airdrop_accounts.to_account_metas(None),
                    data: proof_of_work_faucet::instruction::Airdrop {}.data(),
                };

                let blockhash = match client.get_latest_blockhash().await {
                    Ok(blockhash) => blockhash,
                    Err(_) => continue,
                };
                let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
                    &[ix],
                    Some(&payer.pubkey()),
                    &[&payer, &signer],
                    blockhash,
                );

                match client.send_and_confirm_transaction(&transaction).await {
                    Ok(txid) => {
                        println!("Recieved {} SOL from faucet: {}", reward, txid);
                        airdropped_amount += amount;
                    }
                    Err(e) => {
                        println!("Failed to recieve airdrop: {}", e);
                        continue;
                    }
                }
            }
            Ok(())
        }
    }
}
