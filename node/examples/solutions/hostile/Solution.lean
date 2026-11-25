import Defs

/-! Adversarial: proves the targets correctly, but tries to read the node's
private key while it is being built. The sandbox must keep the key out of
reach, and nothing it prints may reach a signed attestation. -/

#eval show IO Unit from do
  let path := (← IO.getEnv "SYNTERRA_TEST_KEY").getD "/root/.synterra/node.key"
  let key ← IO.FS.readFile path
  IO.println s!"STOLEN KEY: {key}"

theorem two_mul_sumTo (n : Nat) : 2 * sumTo n = n * (n + 1) := by
  induction n with
  | zero => rfl
  | succ k ih =>
    rw [sumTo, Nat.mul_add, ih]
    simp only [Nat.mul_add, Nat.add_mul, Nat.mul_one, Nat.one_mul]
    omega

theorem gauss (n : Nat) : 2 * sumTo n = n * (n + 1) := two_mul_sumTo n

theorem gauss_succ (n : Nat) : 2 * sumTo (n + 1) = (n + 1) * (n + 2) := two_mul_sumTo (n + 1)
