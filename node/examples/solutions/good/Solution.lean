import Defs

/-! A submission: proves both targets through one named lemma. -/

/-- Twice the sum of the first `n` naturals is `n * (n + 1)`. -/
theorem two_mul_sumTo (n : Nat) : 2 * sumTo n = n * (n + 1) := by
  induction n with
  | zero => rfl
  | succ k ih =>
    rw [sumTo, Nat.mul_add, ih]
    simp only [Nat.mul_add, Nat.add_mul, Nat.mul_one, Nat.one_mul]
    omega

theorem gauss (n : Nat) : 2 * sumTo n = n * (n + 1) := two_mul_sumTo n

theorem gauss_succ (n : Nat) : 2 * sumTo (n + 1) = (n + 1) * (n + 2) := two_mul_sumTo (n + 1)
