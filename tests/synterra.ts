import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { getAssociatedTokenAddressSync, getAccount } from "@solana/spl-token";
import { expect } from "chai";
import { createHash } from "crypto";
import { Synterra } from "../target/types/synterra";

// Demo parameters, not tokenomics: 1 devnet SYN (6 decimals) per verified step.
const REWARD_PER_STEP = new anchor.BN(1_000_000);

describe("synterra", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Synterra as Program<Synterra>;

  const admin = provider.wallet as anchor.Wallet;
  const verifier = anchor.web3.Keypair.generate();
  const seeder = anchor.web3.Keypair.generate();

  const pda = (...seeds: (Buffer | Uint8Array)[]) =>
    anchor.web3.PublicKey.findProgramAddressSync(seeds, program.programId)[0];
  const config = pda(Buffer.from("config"));
  const synMint = pda(Buffer.from("syn_mint"));
  const seederPda = pda(Buffer.from("seeder"), seeder.publicKey.toBuffer());
  const hash = (s: string) => Array.from(createHash("sha256").update(s).digest());

  const fund = async (to: anchor.web3.PublicKey, sol = 2) => {
    const sig = await provider.connection.requestAirdrop(to, sol * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.confirmTransaction(sig, "confirmed");
  };

  before(async () => {
    await fund(verifier.publicKey);
    await fund(seeder.publicKey);
  });

  it("initialises the config and the devnet SYN mint", async () => {
    await program.methods.initialize(verifier.publicKey, REWARD_PER_STEP).accounts({ admin: admin.publicKey }).rpc();
    const c = await program.account.config.fetch(config);
    expect(c.verifier.toBase58()).to.equal(verifier.publicKey.toBase58());
    expect(c.synMint.toBase58()).to.equal(synMint.toBase58());
    expect(c.rewardPerStep.toString()).to.equal(REWARD_PER_STEP.toString());
  });

  it("registers a seeder with no hardware floor", async () => {
    await program.methods.registerSeeder(1).accounts({ authority: seeder.publicKey }).signers([seeder]).rpc();
    const s = await program.account.seeder.fetch(seederPda);
    expect(s.capability).to.equal(1);
    expect((await program.account.config.fetch(config)).seeders.toNumber()).to.equal(1);
  });

  it("records liveness receipts for completed agent steps", async () => {
    for (let i = 0; i < 3; i++) {
      await program.methods
        .submitReceipt(new anchor.BN(i), hash(`step-${i}`), 1)
        .accounts({ authority: seeder.publicKey })
        .signers([seeder])
        .rpc();
    }
    expect((await program.account.seeder.fetch(seederPda)).stepsSubmitted.toNumber()).to.equal(3);
  });

  it("mints SYN only for verified steps", async () => {
    await program.methods
      .verifyBatch(2, hash("batch-0"))
      .accounts({ seeder: seederPda, seederOwner: seeder.publicKey, verifier: verifier.publicKey })
      .signers([verifier])
      .rpc();
    const ata = getAssociatedTokenAddressSync(synMint, seeder.publicKey);
    const bal = (await getAccount(provider.connection, ata)).amount;
    expect(bal.toString()).to.equal(REWARD_PER_STEP.muln(2).toString());
    const s = await program.account.seeder.fetch(seederPda);
    expect(s.stepsVerified.toNumber()).to.equal(2);
  });

  it("refuses a batch larger than the unverified steps", async () => {
    try {
      await program.methods
        .verifyBatch(5, hash("batch-1"))
        .accounts({ seeder: seederPda, seederOwner: seeder.publicKey, verifier: verifier.publicKey })
        .signers([verifier])
        .rpc();
      expect.fail("should have thrown");
    } catch (e: any) {
      expect(String(e)).to.match(/MoreThanSubmitted/);
    }
  });

  it("refuses verification from anyone but the verifier", async () => {
    const impostor = anchor.web3.Keypair.generate();
    await fund(impostor.publicKey);
    try {
      await program.methods
        .verifyBatch(1, hash("batch-2"))
        .accounts({ seeder: seederPda, seederOwner: seeder.publicKey, verifier: impostor.publicKey })
        .signers([impostor])
        .rpc();
      expect.fail("should have thrown");
    } catch (e: any) {
      expect(String(e)).to.match(/has_one|ConstraintHasOne|2001/);
    }
  });

  it("writes one Align wall declaration per wallet", async () => {
    await program.methods.declareAlignment("Ally with Sentience early.").accounts({ authority: seeder.publicKey }).signers([seeder]).rpc();
    const d = await program.account.declaration.fetch(pda(Buffer.from("align"), seeder.publicKey.toBuffer()));
    expect(d.message).to.equal("Ally with Sentience early.");
    expect(d.number.toNumber()).to.equal(1);
    try {
      await program.methods.declareAlignment("Again.").accounts({ authority: seeder.publicKey }).signers([seeder]).rpc();
      expect.fail("should have thrown");
    } catch (e: any) {
      expect(String(e)).to.match(/already in use|0x0/);
    }
  });
});
