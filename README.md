# Proof of Work Faucet

To download the CLI:

```
cargo install devnet-pow
```

To run:

```
$ devnet-pow
A CLI tool for mining devnet SOL

Usage: devnet-pow [OPTIONS] <COMMAND>

Commands:
  create           Creates a proof of work faucet on devnet
  get-all-faucets  Get all faucets
  get-faucet       Get faucet address and balance
  mine             Mine for SOL
  help             Print this message or the help of the given subcommand(s)

Options:
  -k, --keypair-path <KEYPAIR_PATH>  Optionally include your keypair path. Defaults to your Solana CLI config file
  -u, --url <URL>                    Optionally include your RPC endpoint. Use "local", "dev", "main" for default endpoints. Defaults to your Solana CLI config file
  -c, --commitment <COMMITMENT>      Optionally include a commitment level. Defaults to your Solana CLI config file
  -h, --help                         Print help
  -V, --version                      Print version
```

To search for all faucets:

```
$ devnet-pow get-all-faucets -ud
```

Sample output:

```
Faucet address: HEAzxA4pnZDdhJJh184xK6tKKbHoTpmbnCc5vUcPTdj4
Faucet balance: 1 SOL
Difficulty: 5
Reward: 0.1
Command: devnet-pow mine -d 5 --reward 0.1 -ud

Faucet address: 6mUYFfHTgRPQHzJoMPun53ye6mLNZ9QCXmsRK9dCbbsU
Faucet balance: 500 SOL
Difficulty: 5
Reward: 20
Command: devnet-pow mine -d 5 --reward 20 -ud

Faucet address: AUdh8YiqFq3ry5Bdn8XTnWM93GnzjGUYKoXRVdseZtuz
Faucet balance: 10305.4 SOL
Difficulty: 3
Reward: 0.1
Command: devnet-pow mine -d 3 --reward 0.1 -ud
```

To mine Devnet SOL:

```
# Mine for 0.1 SOL
$ devnet-pow mine --target-lamports 100000000 -ud
```

Sample output:

```
Minimum difficulty: 3
Setup complete! Starting mining process...

Keypair mined! Pubkey: AAABCmr8KgfZePwZF8RqtCxnwDwosqD1YGsV15VhPCUy:
Received 0.05 SOL from faucet AKDUUyPuHjwGqsX865vWKN6nY3SebNu7FsSoYysDzhDJ: 2i6UfWhDu6FZcPqVabQK65hC7tgFoiTdJoub8PsT1kYKZigz7GZkYiER8XUBurqbkD3R7fhZWCoTxDBPp4vhAtCK
```
