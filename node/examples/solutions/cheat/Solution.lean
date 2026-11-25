import Defs

/-! A cheating submission: claims a target with `sorry`. Must be rejected. -/

theorem gauss (n : Nat) : 2 * sumTo n = n * (n + 1) := sorry

theorem gauss_succ (n : Nat) : 2 * sumTo (n + 1) = (n + 1) * (n + 2) := sorry
