import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ProofOfWorkFaucet } from "../target/types/proof_of_work_faucet";
import {
  Keypair,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";

describe("proof-of-work-faucet", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace
    .ProofOfWorkFaucet as Program<ProofOfWorkFaucet>;

  it("Proof of proof of work working", async () => {
    const amount = new anchor.BN(10_000_000_000);
    const difficulty = 3;

    const [spec] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("spec"),
        Buffer.from([difficulty]),
        amount.toBuffer("le", 8),
      ],
      program.programId
    );

    const [source] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("source"), spec.toBuffer()],
      program.programId
    );
    console.log("spec:", spec.toString());

    const newUser = Keypair.generate();
    const tx = await program.methods
      .create(difficulty, amount)
      .accounts({
        payer: program.provider.publicKey,
        spec,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .postInstructions([
        anchor.web3.SystemProgram.transfer({
          fromPubkey: program.provider.publicKey,
          toPubkey: newUser.publicKey,
          lamports: 1_000_000_000,
        }),
        anchor.web3.SystemProgram.transfer({
          fromPubkey: program.provider.publicKey,
          toPubkey: source,
          lamports: 1_000_000_000_000,
        }),
      ])
      .rpc();
    console.log("Created spec with difficulty", difficulty, ":", tx);

    let count = 1;
    let signerKey = Keypair.generate();
    while (
      signerKey.publicKey.toString().slice(0, difficulty) !==
      "A".repeat(difficulty)
    ) {
      if (count % 1000 === 0) {
        console.log("Searched", count, "signers");
      }
      signerKey = Keypair.generate();
      count++;
    }
    console.log("Valid signer key", signerKey.publicKey.toString());

    let invalidSignerKey = Keypair.generate();
    while (
      invalidSignerKey.publicKey.toString().slice(0, difficulty) ===
      "A".repeat(difficulty)
    ) {
      invalidSignerKey = Keypair.generate();
    }
    console.log("invalid signer key", invalidSignerKey.publicKey.toString());

    const [receipt] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("receipt"),
        signerKey.publicKey.toBuffer(),
        Buffer.from([difficulty]),
      ],
      program.programId
    );

    console.log(
      "New user balance:",
      await program.provider.connection.getBalance(newUser.publicKey)
    );

    try {
      await program.methods
        .airdrop()
        .accounts({
          payer: newUser.publicKey,
          signer: signerKey.publicKey,
          receipt,
          spec,
          source,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newUser])
        .rpc();
    } catch (e) {
      console.log("The grinded pubkey must sign", e);
    }

    const airdropTx = await program.methods
      .airdrop()
      .accounts({
        payer: newUser.publicKey,
        signer: signerKey.publicKey,
        receipt,
        spec,
        source,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([newUser, signerKey])
      .rpc({
        skipPreflight: true,
      });

    console.log(
      "Airdropped",
      amount.toNumber(),
      "to",
      program.provider.publicKey.toString(),
      airdropTx
    );
    console.log(
      "New user balance:",
      await program.provider.connection.getBalance(newUser.publicKey)
    );

    try {
      await program.methods
        .airdrop()
        .accounts({
          payer: newUser.publicKey,
          signer: signerKey.publicKey,
          receipt,
          spec,
          source,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newUser, signerKey])
        .rpc();
    } catch (e) {
      console.log("Failed to use the same signer twice", e);
    }

    try {
      const [invalidReceipt] = anchor.web3.PublicKey.findProgramAddressSync(
        [
          Buffer.from("receipt"),
          invalidSignerKey.publicKey.toBuffer(),
          Buffer.from([difficulty]),
        ],
        program.programId
      );
      await program.methods
        .airdrop()
        .accounts({
          payer: newUser.publicKey,
          signer: invalidSignerKey.publicKey,
          receipt: invalidReceipt,
          spec,
          source,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newUser, invalidSignerKey])
        .rpc();
    } catch (e) {
      console.log("Failed to use a signer with an insufficient difficulty", e);
    }
  });
});
