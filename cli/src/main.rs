use std::collections::BTreeMap;

use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anyhow::anyhow;
use borsh::BorshDeserialize;
use bs58::encode;
use clap::{Parser, Subcommand};
use itertools::Itertools;
use proof_of_work_faucet::Difficulty;
use solana_account_decoder::UiAccountEncoding;
use solana_cli_config::{Config, ConfigInput, CONFIG_FILE};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_client::rpc_config::RpcProgramAccountsConfig;
use solana_client::rpc_filter::RpcFilterType;
use solana_sdk::commitment_config::CommitmentConfig;
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
    /// Get all faucets
    GetAllFaucets,
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
        /// Prefix length
        #[clap(short, long)]
        difficulty: Option<u8>,
        #[clap(long)]
        /// Reward amount in SOL
        reward: Option<f64>,
        /// Target number of lamports to mine for
        #[clap(short, long, default_value = "10000000000")]
        target_lamports: u64,
        /// Do not search for faucets automatically
        #[clap(long, default_value = "false")]
        no_infer: bool,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FaucetMetadata {
    pub spec_pubkey: Pubkey,
    pub faucet_pubkey: Pubkey,
    pub difficulty: u8,
    pub amount: u64,
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
        SubCommand::GetAllFaucets => {
            for FaucetMetadata {
                faucet_pubkey,
                difficulty,
                amount,
                ..
            } in get_all_faucets(&client, &commitment).await?.iter()
            {
                let reward = *amount as f64 / 1e9;
                let balance = client
                    .get_balance_with_commitment(faucet_pubkey, commitment)
                    .await?
                    .value;
                println!("Faucet address: {}", faucet_pubkey);
                println!("Faucet balance: {} SOL", balance as f64 / 1e9);
                println!("Difficulty: {}", difficulty);
                println!("Reward: {}", reward);
                println!(
                    "Command: devnet-pow mine -d {} --reward {} -ud",
                    difficulty, reward
                );
                println!()
            }
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

            let balance = client
                .get_balance_with_commitment(&faucet, commitment)
                .await?
                .value;
            println!("Faucet balance: {} SOL", balance as f64 / 1e9);
            Ok(())
        }
        SubCommand::Mine {
            difficulty,
            reward,
            target_lamports,
            no_infer,
        } => {
            let mut faucet_specs = if no_infer {
                let mut faucet_specs = BTreeMap::new();
                match (difficulty, reward) {
                    (Some(d), Some(r)) => {
                        let mut spec = BTreeMap::new();
                        let reward_as_amount = (r * 1e9) as u64;
                        let (spec_pubkey, _) = Pubkey::find_program_address(
                            &[
                                b"spec",
                                d.to_le_bytes().as_ref(),
                                reward_as_amount.to_le_bytes().as_ref(),
                            ],
                            &proof_of_work_faucet::id(),
                        );
                        let (faucet_pubkey, _) = Pubkey::find_program_address(
                            &[b"source", spec_pubkey.as_ref()],
                            &proof_of_work_faucet::id(),
                        );

                        let metadata = FaucetMetadata {
                            spec_pubkey,
                            faucet_pubkey,
                            difficulty: d,
                            amount: reward_as_amount,
                        };

                        spec.insert(reward_as_amount, metadata);
                        faucet_specs.insert(d, spec);
                        faucet_specs
                    }
                    _ => {
                        return Err(anyhow!(
                            "Must specify difficulty and reward when using --no-infer"
                        ));
                    }
                }
            } else {
                get_inferred_faucets(&client, &commitment, difficulty, reward).await?
            };
            if faucet_specs.is_empty() {
                println!("No faucets found");
                return Ok(());
            }

            if client.get_balance(&payer.pubkey()).await? < 5000 {
                // Try to request airdrop the normal way if the wallet is completely empty
                client
                    .request_airdrop(&payer.pubkey(), 1_000_000_000)
                    .await?;
            }

            // This variable is used to short circuit the loop if the grinded key is below the minimum prefix length
            let mut min_prefix_len = *faucet_specs
                .keys()
                .min()
                .ok_or_else(|| anyhow!("No faucets found"))?;

            println!("Minimum difficulty: {}", min_prefix_len);
            println!("Setup complete! Starting mining process...");
            println!();
            let mut airdropped_amount = 0;

            while airdropped_amount < target_lamports {
                let signer = Keypair::new();

                let prefix_len = encode(signer.pubkey().as_ref())
                    .into_string()
                    .chars()
                    .take_while(|ch| ch == &'A')
                    .count();

                if prefix_len < min_prefix_len as usize {
                    continue;
                }

                let mut candidate_faucets = vec![];
                faucet_specs
                    .iter()
                    .for_each(|(difficulty, specs_for_difficulty)| {
                        // Filter the faucets that meet the difficulty requirement
                        if *difficulty as usize <= prefix_len {
                            specs_for_difficulty.iter().for_each(|(_, spec)| {
                                candidate_faucets.push(*spec);
                            })
                        }
                    });
                candidate_faucets.sort_by(|spec1, spec2| {
                    if spec1.amount != spec2.amount {
                        spec1.amount.cmp(&spec2.amount)
                    } else {
                        spec1.difficulty.cmp(&spec2.difficulty)
                    }
                });

                if candidate_faucets.is_empty() {
                    println!("No candidate faucets found for {}", signer.pubkey());
                    continue;
                }

                println!("Keypair mined! Pubkey: {}: ", signer.pubkey());

                // Keep track of the difficulties that we've mined for the current key
                let mut matched_difficulties = vec![];

                // Try to claim the airdrop from each of the candidate faucets
                while !candidate_faucets.is_empty() {
                    let metadata = candidate_faucets.pop().unwrap();

                    if matched_difficulties.contains(&metadata.difficulty) {
                        continue;
                    }

                    if client
                        .get_balance_with_commitment(&metadata.faucet_pubkey, commitment)
                        .await?
                        .value
                        < metadata.amount
                    {
                        // Remove this key from the global list of faucets
                        println!("Faucet {} is empty", metadata.faucet_pubkey);
                        faucet_specs
                            .get_mut(&metadata.difficulty)
                            .unwrap()
                            .remove(&metadata.amount);

                        // Update min_prefix_len if necessary
                        if faucet_specs.get(&metadata.difficulty).unwrap().is_empty() {
                            faucet_specs.remove(&metadata.difficulty);
                            if metadata.difficulty == min_prefix_len {
                                min_prefix_len = match faucet_specs.keys().min() {
                                    Some(min) => *min,
                                    None => {
                                        println!("No faucets remaining");
                                        return Ok(());
                                    }
                                };
                            }
                        }
                        continue;
                    }

                    let reward = metadata.amount as f64 / 1e9;
                    let (receipt, _) = Pubkey::find_program_address(
                        &[
                            b"receipt",
                            signer.pubkey().as_ref(),
                            metadata.difficulty.to_le_bytes().as_ref(),
                        ],
                        &proof_of_work_faucet::id(),
                    );
                    let airdrop_accounts = proof_of_work_faucet::accounts::Airdrop {
                        payer: payer.pubkey(),
                        signer: signer.pubkey(),
                        receipt,
                        spec: metadata.spec_pubkey,
                        source: metadata.faucet_pubkey,
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
                            println!(
                                "Received {} SOL from faucet {}: {}",
                                reward, metadata.faucet_pubkey, txid
                            );
                            airdropped_amount += metadata.amount;
                            matched_difficulties.push(metadata.difficulty);
                        }
                        Err(e) => {
                            println!("Failed to recieve airdrop: {}", e);
                            continue;
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

async fn get_all_faucets(
    client: &RpcClient,
    commitment: &CommitmentConfig,
) -> anyhow::Result<Vec<FaucetMetadata>> {
    let config = RpcProgramAccountsConfig {
        filters: Some(vec![RpcFilterType::DataSize(17)]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Binary),
            commitment: Some(*commitment),
            ..RpcAccountInfoConfig::default()
        },
        ..RpcProgramAccountsConfig::default()
    };
    let specs = client
        .get_program_accounts_with_config(&proof_of_work_faucet::id(), config)
        .await?
        .iter()
        .filter_map(|(pubkey, account)| {
            let difficulty = Difficulty::try_from_slice(&account.data[8..]).ok()?;
            let (faucet, _) = Pubkey::find_program_address(
                &[b"source", pubkey.as_ref()],
                &proof_of_work_faucet::id(),
            );
            Some(FaucetMetadata {
                spec_pubkey: *pubkey,
                faucet_pubkey: faucet,
                difficulty: difficulty.difficulty,
                amount: difficulty.amount,
            })
        })
        .collect_vec();
    Ok(specs)
}

async fn get_inferred_faucets(
    client: &RpcClient,
    commitment: &CommitmentConfig,
    difficulty: Option<u8>,
    reward: Option<f64>,
) -> anyhow::Result<BTreeMap<u8, BTreeMap<u64, FaucetMetadata>>> {
    let mut faucet_specs = get_all_faucets(client, commitment)
        .await?
        .iter()
        .filter(|spec_metadata| {
            if let Some(difficulty) = difficulty {
                if spec_metadata.difficulty < difficulty {
                    return false;
                }
            }
            if let Some(reward) = reward {
                let reward_as_amount = (reward * 1e9) as u64;
                if spec_metadata.amount < reward_as_amount {
                    return false;
                }
            }
            // Ignore specs that are not profitable to mine
            if spec_metadata.amount < 895880 {
                return false;
            }
            true
        })
        .group_by(|spec_metadata| spec_metadata.difficulty)
        .into_iter()
        .map(|(key, group)| {
            let specs_for_difficulty = group
                .map(|spec| (spec.amount, *spec))
                .collect::<BTreeMap<u64, FaucetMetadata>>();
            (key, specs_for_difficulty)
        })
        .collect::<BTreeMap<u8, BTreeMap<u64, FaucetMetadata>>>();

    let mut keys_to_remove = vec![];

    for (difficulty, specs_for_difficulty) in faucet_specs.iter() {
        for (amount, spec) in specs_for_difficulty.iter() {
            // Make sure this is a valid faucet
            match client.get_account(&spec.spec_pubkey).await {
                Ok(acc) => {
                    if acc.data.is_empty() {
                        keys_to_remove.push((*difficulty, *amount));
                    }
                }
                Err(_) => {
                    keys_to_remove.push((*difficulty, *amount));
                }
            }
            let balaance = client
                .get_balance_with_commitment(&spec.faucet_pubkey, *commitment)
                .await?
                .value;
            if balaance < *amount {
                keys_to_remove.push((*difficulty, *amount));
            }
        }
    }

    // Clean up all invalid faucets
    let mut difficulties_to_remove = vec![];
    for (difficulty, amount) in keys_to_remove {
        faucet_specs.get_mut(&difficulty).unwrap().remove(&amount);
        if faucet_specs.get(&difficulty).unwrap().is_empty() {
            difficulties_to_remove.push(difficulty);
        }
    }
    for difficulty in difficulties_to_remove {
        faucet_specs.remove(&difficulty);
    }

    Ok(faucet_specs)
}
